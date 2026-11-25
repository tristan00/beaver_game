use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/beaver_colony.ico");
    println!("cargo:rerun-if-changed=build.rs");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let ico = manifest_dir.join("assets").join("beaver_colony.ico");
    if !ico.exists() {
        return;
    }
    let Some(rc) = find_rc() else {
        return;
    };
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let rc_file = out_dir.join("app_icon.rc");
    let res_file = out_dir.join("app_icon.res");
    let ico_escaped = ico.to_string_lossy().replace('\\', "\\\\");
    if std::fs::write(&rc_file, format!("1 ICON \"{ico_escaped}\"\n")).is_err() {
        return;
    }
    let ok = Command::new(rc)
        .arg("/nologo")
        .arg("/fo")
        .arg(&res_file)
        .arg(&rc_file)
        .status()
        .is_ok_and(|s| s.success());
    if ok && res_file.exists() {
        println!("cargo:rustc-link-arg-bin=beaver_colony={}", res_file.display());
    }
}

fn find_rc() -> Option<PathBuf> {
    if Command::new("rc.exe").arg("/?").output().is_ok() {
        return Some(PathBuf::from("rc.exe"));
    }
    let kits = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut versions: Vec<PathBuf> = std::fs::read_dir(kits)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    versions.sort();
    for dir in versions.into_iter().rev() {
        let candidate = dir.join("x64").join("rc.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
