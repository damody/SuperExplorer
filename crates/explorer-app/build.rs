fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=../../third_party/everything-sdk/Everything64.dll");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // gpui-elements' editable text performs nested text measurement and IME layout on the
        // Windows UI thread. The MSVC default 1 MiB stack is insufficient in debug builds.
        println!("cargo:rustc-link-arg=/STACK:8388608");
        embed_resource::compile("app.rc", embed_resource::NONE)
            .manifest_required()
            .expect("compile Windows application resources");
        if let Ok(output) = std::env::var("OUT_DIR") {
            let output = std::path::PathBuf::from(output);
            if let Some(profile) = output.ancestors().nth(3) {
                let source =
                    std::path::Path::new("../../third_party/everything-sdk/Everything64.dll");
                let _ = std::fs::copy(source, profile.join("Everything64.dll"));
            }
        }
    }
}
