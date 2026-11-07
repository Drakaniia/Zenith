//! Primitives for the password vault: Argon2id key derivation, AES-256-GCM sealing, a keyed
//! SHA-256 hash, Windows DPAPI for the metadata index, recovery keys and password generation.
//! Secrets are held in `Zeroizing` buffers so they are wiped when dropped.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

pub type Key = Zeroizing<[u8; 32]>;

/// Argon2id cost. Stored in the vault file, so it can be raised later without breaking old vaults.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct KdfParams {
    /// Memory in KiB.
    pub m: u32,
    /// Passes.
    pub t: u32,
    /// Lanes.
    pub p: u32,
}

impl KdfParams {
    /// OWASP's Argon2id recommendation (46 MiB, 2 passes, 1 lane): about half a second.
    pub const DEFAULT: Self = Self { m: 47_104, t: 2, p: 1 };
    /// Unit tests only: the smallest valid cost.
    #[cfg(test)]
    pub const FAST: Self = Self { m: 64, t: 1, p: 1 };
}

/// Bytes from the operating system's secure random generator.
pub fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("the OS random generator failed");
    b
}

pub fn random_key() -> Key {
    Zeroizing::new(random::<32>())
}

/// Derives a 256-bit key from a password or recovery key.
pub fn derive(secret: &[u8], salt: &[u8], p: KdfParams) -> Result<Key, String> {
    let params = argon2::Params::new(p.m, p.t, p.p, Some(32)).map_err(|e| e.to_string())?;
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut out = Zeroizing::new([0u8; 32]);
    argon.hash_password_into(secret, salt, &mut *out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// AES-256-GCM ciphertext with its nonce.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Sealed {
    #[serde(with = "b64")]
    pub nonce: Vec<u8>,
    #[serde(with = "b64")]
    pub ct: Vec<u8>,
}

/// Encrypts `plain` with a fresh random nonce. `aad` is authenticated but not encrypted: it binds
/// the ciphertext to its context (an entry's id, site and username), so it can't be moved.
pub fn seal(key: &[u8; 32], plain: &[u8], aad: &[u8]) -> Sealed {
    let cipher = Aes256Gcm::new_from_slice(key).expect("a 32-byte key");
    let nonce = random::<12>();
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce), Payload { msg: plain, aad })
        .expect("AES-GCM encryption cannot fail for in-memory buffers");
    Sealed { nonce: nonce.to_vec(), ct }
}

/// Decrypts and authenticates; `None` for a wrong key, wrong context or tampered data.
pub fn open(key: &[u8; 32], sealed: &Sealed, aad: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    if sealed.nonce.len() != 12 {
        return None;
    }
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    cipher
        .decrypt(Nonce::from_slice(&sealed.nonce), Payload { msg: &sealed.ct, aad })
        .ok()
        .map(Zeroizing::new)
}

/// Unwraps a sealed 32-byte key.
pub fn open_key(kek: &[u8; 32], sealed: &Sealed, aad: &[u8]) -> Option<Key> {
    let plain = open(kek, sealed, aad)?;
    let bytes: [u8; 32] = plain.as_slice().try_into().ok()?;
    Some(Zeroizing::new(bytes))
}

/// HMAC-SHA256 (RFC 2104).
pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut k = Zeroizing::new([0u8; BLOCK]);
    if key.len() > BLOCK {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(k.iter().map(|b| b ^ 0x36).collect::<Vec<u8>>());
    inner.update(msg);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(k.iter().map(|b| b ^ 0x5c).collect::<Vec<u8>>());
    outer.update(inner);
    outer.finalize().into()
}

/// A short keyed fingerprint of a password, kept next to the (locked) metadata so Zenith can tell
/// "already saved" from "changed" without the master password. Only 32 bits: far too few to
/// confirm a guessed password, but a false match is a 1 in 4 billion event.
pub fn fingerprint(check_key: &[u8], password: &str) -> u32 {
    let mac = hmac_sha256(check_key, password.as_bytes());
    u32::from_le_bytes([mac[0], mac[1], mac[2], mac[3]])
}

// ---------------------------------------------------------------------------------------------
// Recovery keys

/// Crockford base32: no I, L, O or U, so a written-down key is hard to misread.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A new recovery key: 160 random bits as 32 characters in groups of four.
pub fn new_recovery_key() -> Zeroizing<String> {
    let bytes = Zeroizing::new(random::<20>());
    let mut out = String::with_capacity(40);
    let mut acc: u32 = 0;
    let mut bits = 0;
    let mut n = 0;
    for &b in bytes.iter() {
        acc = (acc << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            if n > 0 && n % 4 == 0 {
                out.push('-');
            }
            out.push(ALPHABET[((acc >> bits) & 31) as usize] as char);
            n += 1;
        }
    }
    // 160 bits are exactly 32 characters: nothing is left over.
    Zeroizing::new(out)
}

/// Canonical form of a typed recovery key: dashes and spaces dropped, upper case, and the
/// characters people confuse mapped as Crockford base32 does (O→0, I/L→1).
pub fn normalize_recovery_key(input: &str) -> Zeroizing<String> {
    let mut out = String::with_capacity(32);
    for c in input.chars() {
        let c = c.to_ascii_uppercase();
        match c {
            '-' | ' ' | '\t' | '\r' | '\n' => {}
            'O' => out.push('0'),
            'I' | 'L' => out.push('1'),
            _ => out.push(c),
        }
    }
    Zeroizing::new(out)
}

// ---------------------------------------------------------------------------------------------
// Password generator

const LOWER: &[u8] = b"abcdefghijkmnopqrstuvwxyz";
const UPPER: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
const DIGITS: &[u8] = b"23456789";
const SYMBOLS: &[u8] = b"-_.!@#$%*?";

/// A uniformly random index below `n` (rejection sampling, no modulo bias).
fn below(n: usize) -> usize {
    let limit = u32::MAX - (u32::MAX % n as u32);
    loop {
        let v = u32::from_le_bytes(random::<4>());
        if v < limit {
            return (v % n as u32) as usize;
        }
    }
}

/// A strong password with at least one character of each kind. Look-alike characters are left out.
pub fn generate_password(len: usize) -> Zeroizing<String> {
    let len = len.clamp(8, 128);
    let sets: [&[u8]; 4] = [LOWER, UPPER, DIGITS, SYMBOLS];
    let all: Vec<u8> = sets.concat();
    let mut chars = Zeroizing::new(Vec::with_capacity(len));
    for set in sets {
        chars.push(set[below(set.len())]);
    }
    while chars.len() < len {
        chars.push(all[below(all.len())]);
    }
    // Fisher-Yates, so the guaranteed characters are not always first.
    for i in (1..chars.len()).rev() {
        let j = below(i + 1);
        chars.swap(i, j);
    }
    Zeroizing::new(chars.iter().map(|&b| b as char).collect())
}

// ---------------------------------------------------------------------------------------------
// Windows DPAPI: ties the metadata index to this Windows account.

const DPAPI_ENTROPY: &[u8] = b"zenith-vault-index-v1";

#[cfg(windows)]
pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
    use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};
    unsafe {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let entropy = CRYPT_INTEGER_BLOB { cbData: DPAPI_ENTROPY.len() as u32, pbData: DPAPI_ENTROPY.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB::default();
        CryptProtectData(&input, None, Some(&entropy), None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
            .map_err(|e| format!("Windows could not protect the data: {e}"))?;
        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        free_blob(out);
        Ok(bytes)
    }
}

#[cfg(windows)]
pub fn unprotect(data: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
    use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};
    unsafe {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let entropy = CRYPT_INTEGER_BLOB { cbData: DPAPI_ENTROPY.len() as u32, pbData: DPAPI_ENTROPY.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB::default();
        CryptUnprotectData(&input, None, Some(&entropy), None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
            .map_err(|e| format!("Windows could not unprotect the data: {e}"))?;
        let slice = std::slice::from_raw_parts_mut(out.pbData, out.cbData as usize);
        let bytes = Zeroizing::new(slice.to_vec());
        slice.fill(0);
        free_blob(out);
        Ok(bytes)
    }
}

#[cfg(windows)]
unsafe fn free_blob(blob: windows::Win32::Security::Cryptography::CRYPT_INTEGER_BLOB) {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    unsafe {
        let _ = LocalFree(Some(HLOCAL(blob.pbData as *mut core::ffi::c_void)));
    }
}

#[cfg(not(windows))]
pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
    Ok(data.to_vec())
}

#[cfg(not(windows))]
pub fn unprotect(data: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
    Ok(Zeroizing::new(data.to_vec()))
}

/// Serde helper: bytes as standard base64 strings.
pub mod b64 {
    use base64::Engine as _;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD
            .decode(s.as_bytes())
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_round_trip_and_context_binding() {
        let key = random_key();
        let s = seal(&key, b"hunter2", b"ctx");
        assert_eq!(open(&key, &s, b"ctx").unwrap().as_slice(), b"hunter2");
        assert!(open(&key, &s, b"other").is_none(), "wrong context must fail");
        let other = random_key();
        assert!(open(&other, &s, b"ctx").is_none(), "wrong key must fail");
        let mut tampered = s.clone();
        tampered.ct[0] ^= 1;
        assert!(open(&key, &tampered, b"ctx").is_none(), "tampering must fail");
    }

    #[test]
    fn derive_is_deterministic_per_salt() {
        let a = derive(b"pw", b"saltsaltsaltsalt", KdfParams::FAST).unwrap();
        let b = derive(b"pw", b"saltsaltsaltsalt", KdfParams::FAST).unwrap();
        let c = derive(b"pw", b"othersaltothersa", KdfParams::FAST).unwrap();
        assert_eq!(*a, *b);
        assert_ne!(*a, *c);
    }

    #[test]
    fn hmac_matches_rfc4231_case_2() {
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        let hex: String = mac.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    }

    #[test]
    fn recovery_keys_normalize() {
        let key = new_recovery_key();
        assert_eq!(key.len(), 39, "32 characters plus 7 dashes");
        let typed = key.to_lowercase().replace('-', " ");
        assert_eq!(*normalize_recovery_key(&typed), key.replace('-', ""));
        assert_eq!(*normalize_recovery_key("o0-il"), "0011");
    }

    #[test]
    fn generated_passwords_have_every_kind() {
        for _ in 0..50 {
            let p = generate_password(20);
            assert_eq!(p.len(), 20);
            assert!(p.bytes().any(|b| LOWER.contains(&b)));
            assert!(p.bytes().any(|b| UPPER.contains(&b)));
            assert!(p.bytes().any(|b| DIGITS.contains(&b)));
            assert!(p.bytes().any(|b| SYMBOLS.contains(&b)));
        }
        assert_ne!(*generate_password(20), *generate_password(20));
    }

    #[test]
    fn dpapi_round_trip() {
        let p = protect(b"index").unwrap();
        assert_ne!(p.as_slice(), b"index");
        assert_eq!(unprotect(&p).unwrap().as_slice(), b"index");
    }
}
