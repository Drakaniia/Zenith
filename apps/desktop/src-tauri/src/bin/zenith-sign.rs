//! Signs a release artifact with `TAURI_SIGNING_PRIVATE_KEY` + `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
//! Replaces `minisign.exe` in CI because `minisign.exe` ignores piped stdin on Windows.

use std::fs::{self, File};
use std::path::Path;
use std::process;

use base64::Engine as _;
use minisign::SecretKeyBox;

fn decode_key(raw: &str) -> Result<String, String> {
    if raw.contains("untrusted comment:") {
        return Ok(raw.to_string());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw.trim())
        .map_err(|e| format!("decode TAURI_SIGNING_PRIVATE_KEY base64: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("TAURI_SIGNING_PRIVATE_KEY utf8: {e}"))
}

fn sign_file(raw_key: &str, password: Option<String>, input: &Path, output: &Path) -> Result<(), String> {
    let key_text = decode_key(raw_key)?;
    let sk_box = SecretKeyBox::from_string(&key_text)
        .map_err(|e| format!("parse secret key: {e}"))?;
    let sk = sk_box
        .into_secret_key(password)
        .map_err(|e| format!("decrypt secret key: {e}"))?;
    let file = File::open(input).map_err(|e| format!("open {}: {e}", input.display()))?;
    let sig_box = minisign::sign(None, &sk, file, None, None)
        .map_err(|e| format!("sign {}: {e}", input.display()))?;
    fs::write(output, sig_box.into_string())
        .map_err(|e| format!("write {}: {e}", output.display()))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: zenith-sign <input-file> <output-sig>");
        process::exit(2);
    }
    let raw_key = std::env::var("TAURI_SIGNING_PRIVATE_KEY").unwrap_or_default();
    if raw_key.trim().is_empty() {
        eprintln!("TAURI_SIGNING_PRIVATE_KEY is not set");
        process::exit(1);
    }
    let pw = std::env::var("TAURI_SIGNING_PRIVATE_KEY_PASSWORD").ok().filter(|s| !s.is_empty());
    if let Err(err) = sign_file(&raw_key, pw, Path::new(&args[1]), Path::new(&args[2])) {
        eprintln!("zenith-sign: {err}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minisign::KeyPair;
    use minisign_verify::{PublicKey, Signature};

    #[test]
    fn signs_with_encrypted_b64_key_and_verifies() {
        let KeyPair { pk, sk } = KeyPair::generate_encrypted_keypair(Some("s3cr3t!".to_string()))
            .expect("generate keypair");
        let sk_text = sk.to_box(None).expect("secret key box").to_string();
        let sk_b64 = base64::engine::general_purpose::STANDARD.encode(sk_text.as_bytes());
        let pk_text = pk.to_box().expect("public key box").to_string();

        let dir = std::env::temp_dir().join(format!("zenith-sign-test-{}", process::id()));
        fs::create_dir_all(&dir).expect("create temp dir");
        let msg_path = dir.join("payload.bin");
        let sig_path = dir.join("payload.bin.sig");
        let payload = b"zenith installer bytes";
        fs::write(&msg_path, payload).expect("write payload");

        sign_file(&sk_b64, Some("s3cr3t!".to_string()), &msg_path, &sig_path)
            .expect("sign_file");

        let sig_text = fs::read_to_string(&sig_path).expect("read sig");
        let verifier_pk = PublicKey::decode(&pk_text).expect("decode pk");
        let verifier_sig = Signature::decode(&sig_text).expect("decode sig");
        verifier_pk.verify(payload, &verifier_sig, false).expect("verify sig");

        let _ = fs::remove_dir_all(&dir);
    }
}
