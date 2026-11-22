//! Per-profile single-instance lock + command-line handoff over a Win32 Named Pipe.
//! Replaces `tauri-plugin-single-instance`.

use std::io::Write;
use std::sync::Arc;

use windows::core::HSTRING;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_WRITE, FILE_SHARE_NONE, OPEN_EXISTING,
    PIPE_ACCESS_INBOUND,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows::Win32::System::Threading::CreateMutexW;

const MAX_ARGV_BYTES: usize = 64 * 1024;

pub struct SingleInstanceGuard {
    mutex: HANDLE,
}

unsafe impl Send for SingleInstanceGuard {}
unsafe impl Sync for SingleInstanceGuard {}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.mutex);
        }
    }
}

fn fnv64(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in text.to_ascii_lowercase().as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn names_for(identifier: &str) -> (String, String) {
    let h = fnv64(identifier);
    (
        format!("Local\\ZenithSingleInstance_{h:016x}"),
        format!(r"\\.\pipe\ZenithSingleInstance_{h:016x}"),
    )
}

/// Acquires the per-profile single-instance mutex. If another instance already holds it, forwards
/// `argv` to that instance over its named pipe and returns `Ok(None)`.
pub fn acquire_or_forward(
    identifier: &str,
    argv: &[String],
    on_second: Arc<dyn Fn(Vec<String>) + Send + Sync + 'static>,
) -> Result<Option<SingleInstanceGuard>, String> {
    let (mutex_name, pipe_name) = names_for(identifier);
    let handle = unsafe { CreateMutexW(None, false, &HSTRING::from(&mutex_name)) }
        .map_err(|e| format!("CreateMutexW: {e}"))?;
    let already = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    if already {
        unsafe {
            let _ = CloseHandle(handle);
        }
        forward_argv(&pipe_name, argv);
        return Ok(None);
    }

    std::thread::Builder::new()
        .name("zenith-single-instance".into())
        .spawn(move || listen_pipe(&pipe_name, on_second))
        .map_err(|e| format!("spawn single-instance listener: {e}"))?;

    Ok(Some(SingleInstanceGuard { mutex: handle }))
}

fn forward_argv(pipe_name: &str, argv: &[String]) {
    let Ok(payload) = serde_json::to_vec(argv) else { return };
    if payload.len() > MAX_ARGV_BYTES {
        return;
    }
    let wide = HSTRING::from(pipe_name);
    for _ in 0..10 {
        let handle = unsafe {
            CreateFileW(
                &wide,
                FILE_GENERIC_WRITE.0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        };
        if let Ok(h) = handle {
            let mut written = 0u32;
            unsafe {
                let _ = WriteFile(h, Some(&payload), Some(&mut written), None);
                let _ = CloseHandle(h);
            }
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn listen_pipe(pipe_name: &str, on_second: Arc<dyn Fn(Vec<String>) + Send + Sync + 'static>) {
    let wide = HSTRING::from(pipe_name);
    loop {
        let pipe = unsafe {
            CreateNamedPipeW(
                &wide,
                PIPE_ACCESS_INBOUND,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                4,
                0,
                MAX_ARGV_BYTES as u32,
                0,
                None,
            )
        };
        if pipe == INVALID_HANDLE_VALUE || pipe.is_invalid() {
            std::thread::sleep(std::time::Duration::from_millis(200));
            continue;
        }
        let connected = unsafe { ConnectNamedPipe(pipe, None) }.is_ok()
            || unsafe { GetLastError() }.0 == 535; // ERROR_PIPE_CONNECTED
        if connected {
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let mut read = 0u32;
                let ok = unsafe { ReadFile(pipe, Some(&mut chunk), Some(&mut read), None) }.is_ok();
                if !ok || read == 0 {
                    break;
                }
                let _ = buf.write_all(&chunk[..read as usize]);
                if buf.len() > MAX_ARGV_BYTES {
                    buf.clear();
                    break;
                }
            }
            if !buf.is_empty() {
                if let Ok(argv) = serde_json::from_slice::<Vec<String>>(&buf) {
                    on_second(argv);
                }
            }
            unsafe {
                let _ = DisconnectNamedPipe(pipe);
            }
        }
        unsafe {
            let _ = CloseHandle(pipe);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn second_instance_forwards_argv_and_exits() {
        let id = format!("zenith-test-si-{}", crate::storage::now_ms());
        let (tx, rx) = mpsc::channel();
        let first = acquire_or_forward(&id, &["zenith.exe".into()], Arc::new(move |argv| {
            let _ = tx.send(argv);
        }))
        .unwrap();
        assert!(first.is_some());
        std::thread::sleep(std::time::Duration::from_millis(50));

        let second = acquire_or_forward(
            &id,
            &["zenith.exe".into(), "https://example.com".into()],
            Arc::new(|_| {}),
        )
        .unwrap();
        assert!(second.is_none());
        let received = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        assert_eq!(received, vec!["zenith.exe", "https://example.com"]);
        drop(first);
    }
}
