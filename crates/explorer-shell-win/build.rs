fn main() {
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .file("src/seh_shell_execute.cpp")
        // /EHa lets the structured handler see C++ exceptions (0xE06D7363) that
        // Visual Studio's .sln association can throw back into this process.
        .flag("/EHa");
    if let Some(sdk) = latest_windows_sdk_include() {
        for sub in ["ucrt", "shared", "um"] {
            build.include(sdk.join(sub));
        }
    }
    if let Some(msvc) = latest_msvc_include() {
        build.include(msvc);
    }
    build.compile("seh_shell_execute");
    println!("cargo:rustc-link-lib=shell32");
    println!("cargo:rerun-if-changed=src/seh_shell_execute.cpp");
}

fn latest_windows_sdk_include() -> Option<std::path::PathBuf> {
    latest_child(r"C:\Program Files (x86)\Windows Kits\10\Include")
}

fn latest_msvc_include() -> Option<std::path::PathBuf> {
    let tools =
        latest_child(r"C:\Program Files\Microsoft Visual Studio\18\Community\VC\Tools\MSVC")?;
    Some(tools.join("include"))
}

fn latest_child(root: &str) -> Option<std::path::PathBuf> {
    let mut children = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    children.sort();
    children.pop()
}
