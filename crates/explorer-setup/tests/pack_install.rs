use std::{fs, process::Command};

#[test]
fn packs_and_installs_payload_without_service() {
    let payload = tempfile::tempdir().unwrap();
    fs::write(payload.path().join("hello.txt"), b"ok").unwrap();
    fs::write(payload.path().join("setup-version.txt"), "1.2.3.4").unwrap();
    let packed = payload.path().join("packed.exe");
    let stub = env!("CARGO_BIN_EXE_superexplorer-setup");
    let pack = Command::new(stub)
        .args([
            "--create-installer",
            "--payload-dir",
            &payload.path().display().to_string(),
            "--output",
            &packed.display().to_string(),
        ])
        .status()
        .unwrap();
    assert!(pack.success());
    let installed = tempfile::tempdir().unwrap();
    let install = Command::new(&packed)
        .args([
            "--install-directory",
            &installed.path().display().to_string(),
            "--skip-service",
            "--silent",
        ])
        .status()
        .unwrap();
    assert!(install.success());
    assert_eq!(fs::read(installed.path().join("hello.txt")).unwrap(), b"ok");
    assert!(installed.path().join("Uninstall.exe").is_file());
}
