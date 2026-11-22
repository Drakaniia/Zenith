//! Minimal Chrome Native Messaging stdio echo host (`--native-host`).
//! Protocol: 4-byte little-endian u32 length prefix followed by UTF-8 JSON payload.

use std::io::{self, Read, Write};

const MAX_MSG_BYTES: usize = 1024 * 1024;

pub fn read_frame<R: Read>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > MAX_MSG_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid native messaging payload length: {len}"),
        ));
    }
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload)?;
    Ok(Some(payload))
}

pub fn write_frame<W: Write>(writer: &mut W, payload: &[u8]) -> io::Result<()> {
    let len = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "payload too large"))?;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(payload)?;
    writer.flush()
}

pub fn handle_message(raw: &[u8]) -> serde_json::Value {
    let parsed: serde_json::Value =
        serde_json::from_slice(raw).unwrap_or_else(|_| serde_json::json!({ "raw_invalid": true }));
    serde_json::json!({
        "ok": true,
        "host": "com.zenith.phase2.echo",
        "echo": parsed,
    })
}

pub fn run_stdio_host() -> i32 {
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    loop {
        match read_frame(&mut stdin) {
            Ok(Some(raw)) => {
                let resp = handle_message(&raw);
                let Ok(bytes) = serde_json::to_vec(&resp) else {
                    return 2;
                };
                if write_frame(&mut stdout, &bytes).is_err() {
                    return 3;
                }
            }
            Ok(None) => return 0,
            Err(_) => return 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn round_trips_length_prefixed_json() {
        let input = br#"{"ping":"hello-cef"}"#;
        let mut wire = Vec::new();
        write_frame(&mut wire, input).unwrap();

        let mut cursor = Cursor::new(wire);
        let read_back = read_frame(&mut cursor).unwrap().unwrap();
        assert_eq!(read_back, input);
        assert!(read_frame(&mut cursor).unwrap().is_none());

        let resp = handle_message(&read_back);
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["echo"]["ping"], "hello-cef");
    }

    #[test]
    fn rejects_oversized_frame() {
        let too_big = ((MAX_MSG_BYTES + 1) as u32).to_le_bytes();
        let mut cursor = Cursor::new(too_big.to_vec());
        assert!(read_frame(&mut cursor).is_err());
    }
}
