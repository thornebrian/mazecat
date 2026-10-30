// Embeds bar.manifest.xml (uiAccess, per-monitor DPI) when building with the MSVC linker.
// Windows only starts a uiAccess program that is signed by a trusted certificate and
// installed under Program Files; scripts/build-local.ps1 does both.
fn main() {
    println!("cargo:rerun-if-changed=bar.manifest.xml");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let path = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("bar.manifest.xml");
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", path.display());
        println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:NO");
    }
}
