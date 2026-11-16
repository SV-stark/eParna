fn main() {
    println!("cargo:rerun-if-changed=assets/eparna.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/eparna.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("failed to compile Windows icon resource");
    }
}
