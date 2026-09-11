fn main() {
    println!("cargo:rerun-if-changed=setup.rc");
    println!("cargo:rerun-if-changed=setup.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("setup.rc", embed_resource::NONE)
            .manifest_required()
            .expect("compile SuperExplorer setup manifest");
    }
}
