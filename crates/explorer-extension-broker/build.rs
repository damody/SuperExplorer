fn main() {
    println!("cargo:rerun-if-changed=broker.rc");
    println!("cargo:rerun-if-changed=broker.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("broker.rc", embed_resource::NONE)
            .manifest_required()
            .expect("compile Windows broker resources");
    }
}
