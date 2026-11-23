use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn find_rc() -> Option<PathBuf> {
    if let Ok(status) = Command::new("rc.exe").arg("/?").output() {
        if status.status.success() || !status.stdout.is_empty() || !status.stderr.is_empty() {
            return Some(PathBuf::from("rc.exe"));
        }
    }
    let kits = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut versions: Vec<PathBuf> = fs::read_dir(kits)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path().join(r"x64\rc.exe")))
        .filter(|p| p.is_file())
        .collect();
    versions.sort();
    versions.pop()
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let manifest_xml = manifest_dir.join("app.manifest");
    let icon_ico = manifest_dir.join("..").join("src-tauri").join("icons").join("icon.ico");

    println!("cargo:rerun-if-changed={}", manifest_xml.display());
    println!("cargo:rerun-if-changed={}", icon_ico.display());

    if let Some(rc) = find_rc() {
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let rc_path = out_dir.join("app.rc");
        let res_path = out_dir.join("app.res");
        let icon_escaped = icon_ico.to_string_lossy().replace('\\', "\\\\");
        let xml_escaped = manifest_xml.to_string_lossy().replace('\\', "\\\\");
        let rc_body = format!("1 ICON \"{icon_escaped}\"\n1 24 \"{xml_escaped}\"\n");
        fs::write(&rc_path, rc_body).expect("write app.rc");
        let status = Command::new(rc)
            .args(["/nologo", "/fo"])
            .arg(&res_path)
            .arg(&rc_path)
            .status()
            .expect("run rc.exe");
        assert!(status.success(), "rc.exe failed");
        println!("cargo:rustc-link-arg-bins={}", res_path.display());
    } else {
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", manifest_xml.display());
    }
}
