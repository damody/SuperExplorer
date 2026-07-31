use std::{env, path::Path, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=EXPLORER_GIT_REVISION");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/index");

    let revision = env::var("EXPLORER_GIT_REVISION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(git_revision);
    println!("cargo:rustc-env=EXPLORER_GIT_REVISION={revision}");
}

fn git_revision() -> String {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .current_dir(&workspace_root)
        .output();
    let Ok(output) = output else {
        return "unknown".to_owned();
    };
    if !output.status.success() {
        return "unknown".to_owned();
    }

    let revision = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if revision.is_empty() {
        "unknown".to_owned()
    } else if is_dirty(&workspace_root) {
        format!("{revision}-dirty")
    } else {
        revision
    }
}

fn is_dirty(workspace_root: &Path) -> bool {
    Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no"])
        .current_dir(workspace_root)
        .output()
        .is_ok_and(|output| output.status.success() && !output.stdout.is_empty())
}
