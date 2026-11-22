//! Win32 host window, child-pane layout/resize benchmark, overlay z-order check, and process-tree memory sampling.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use serde::Serialize;
use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetStockObject, BLACK_BRUSH, HBRUSH};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetTopWindow, GetWindow,
    RegisterClassW, SetWindowPos, ShowWindow, CW_USEDEFAULT, GW_HWNDNEXT, HWND_TOP,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_SHOWNOACTIVATE, WNDCLASSW, WS_CHILD,
    WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

#[derive(Debug, Clone, Default, Serialize)]
pub struct MemorySnapshot {
    pub process_count: usize,
    pub host_private_mb: f64,
    pub total_private_mb: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ResizeStats {
    pub iterations: usize,
    pub mean_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
}

pub struct HostWindow {
    pub hwnd: HWND,
    pub overlay_hwnd: HWND,
}

unsafe extern "system" fn host_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

impl HostWindow {
    pub fn create(width: i32, height: i32) -> Result<Self, String> {
        unsafe {
            let hinstance = GetModuleHandleW(None).map_err(|e| format!("GetModuleHandleW: {e}"))?;
            let class_name = w!("ZenithCefPhase2Host");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(host_wnd_proc),
                hInstance: hinstance.into(),
                lpszClassName: class_name,
                hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);

            let hwnd = CreateWindowExW(
                Default::default(),
                class_name,
                w!("Zenith CEF Phase 2 Host"),
                WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_CLIPSIBLINGS | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                width,
                height,
                None,
                None,
                Some(hinstance.into()),
                None,
            )
            .map_err(|e| format!("CreateWindowExW host: {e}"))?;

            // Native child overlay window simulating Zenith's floating chrome/overlay lease.
            let overlay_hwnd = CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!("Zenith Overlay Lease"),
                WS_CHILD | WS_CLIPSIBLINGS | WS_VISIBLE,
                16,
                16,
                260,
                44,
                Some(hwnd),
                None,
                Some(hinstance.into()),
                None,
            )
            .map_err(|e| format!("CreateWindowExW overlay: {e}"))?;

            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            Ok(Self { hwnd, overlay_hwnd })
        }
    }

    pub fn client_size(&self) -> (i32, i32) {
        let mut r = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut r);
        }
        ((r.right - r.left).max(1), (r.bottom - r.top).max(1))
    }

    /// Raises the overlay child window to HWND_TOP and checks whether it precedes all page HWNDs in z-order.
    pub fn raise_and_verify_overlay(&self, page_hwnds: &[HWND]) -> bool {
        if page_hwnds.is_empty() {
            return false;
        }
        unsafe {
            if SetWindowPos(
                self.overlay_hwnd,
                Some(HWND_TOP),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
            .is_err()
            {
                return false;
            }
            let mut cur = GetTopWindow(Some(self.hwnd)).ok();
            let mut seen_overlay = false;
            let mut pages_below = 0usize;
            while let Some(h) = cur {
                if h == self.overlay_hwnd {
                    seen_overlay = true;
                } else if page_hwnds.contains(&h) && seen_overlay {
                    pages_below += 1;
                }
                cur = GetWindow(h, GW_HWNDNEXT).ok();
            }
            seen_overlay && pages_below == page_hwnds.len()
        }
    }

    /// Benchmarks 2x2 split-pane SetWindowPos updates across `iterations` layout steps.
    pub fn benchmark_split_resize(&self, pane_hwnds: &[HWND], iterations: usize) -> ResizeStats {
        if pane_hwnds.len() < 4 || iterations == 0 {
            return ResizeStats::default();
        }
        let (cw, ch) = self.client_size();
        let mut samples_ms = Vec::with_capacity(iterations);
        for i in 0..iterations {
            let split_x = (cw / 2) + ((i as i32 % 7) - 3) * 18;
            let split_y = (ch / 2) + ((i as i32 % 5) - 2) * 14;
            let rects = [
                (0, 0, split_x, split_y),
                (split_x, 0, cw - split_x, split_y),
                (0, split_y, split_x, ch - split_y),
                (split_x, split_y, cw - split_x, ch - split_y),
            ];
            let t0 = Instant::now();
            unsafe {
                for (hwnd, (x, y, w, h)) in pane_hwnds.iter().take(4).zip(rects.iter()) {
                    let _ = SetWindowPos(*hwnd, None, *x, *y, *w, *h, SWP_NOACTIVATE);
                }
            }
            samples_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        samples_ms.sort_by(|a, b| a.total_cmp(b));
        let mean_ms = samples_ms.iter().sum::<f64>() / samples_ms.len() as f64;
        let p95_idx = ((samples_ms.len() as f64 * 0.95).ceil() as usize)
            .saturating_sub(1)
            .min(samples_ms.len() - 1);
        let p95_ms = samples_ms[p95_idx];
        let max_ms = *samples_ms.last().unwrap_or(&0.0);
        ResizeStats {
            iterations,
            mean_ms: (mean_ms * 1000.0).round() / 1000.0,
            p95_ms: (p95_ms * 1000.0).round() / 1000.0,
            max_ms: (max_ms * 1000.0).round() / 1000.0,
        }
    }
}

impl Drop for HostWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

fn process_private_bytes(pid: u32) -> Option<u64> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, false, pid).ok()?;
        let mut pmc = PROCESS_MEMORY_COUNTERS_EX::default();
        let ok = GetProcessMemoryInfo(
            handle,
            &mut pmc as *mut PROCESS_MEMORY_COUNTERS_EX as *mut _,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        )
        .is_ok();
        let _ = CloseHandle(handle);
        if ok {
            Some(pmc.PrivateUsage as u64)
        } else {
            None
        }
    }
}

/// Samples private bytes for this browser process and all descendant CEF subprocesses.
pub fn sample_process_tree_memory() -> MemorySnapshot {
    let root_pid = unsafe { GetCurrentProcessId() };
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    unsafe {
        if let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            if Process32FirstW(snap, &mut entry).is_ok() {
                loop {
                    children
                        .entry(entry.th32ParentProcessID)
                        .or_default()
                        .push(entry.th32ProcessID);
                    if Process32NextW(snap, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
        }
    }

    let mut visited = HashSet::new();
    let mut stack = vec![root_pid];
    while let Some(pid) = stack.pop() {
        if visited.insert(pid) {
            if let Some(kids) = children.get(&pid) {
                stack.extend(kids.iter().copied());
            }
        }
    }

    let host_bytes = process_private_bytes(root_pid).unwrap_or(0);
    let total_bytes: u64 = visited
        .iter()
        .filter_map(|&pid| process_private_bytes(pid))
        .sum();
    let mb = |b: u64| ((b as f64 / (1024.0 * 1024.0)) * 100.0).round() / 100.0;

    MemorySnapshot {
        process_count: visited.len(),
        host_private_mb: mb(host_bytes),
        total_private_mb: mb(total_bytes),
    }
}
