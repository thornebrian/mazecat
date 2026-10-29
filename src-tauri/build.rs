// MAZECAT_UIACCESS=1 builds the "above the taskbar" version (scripts/build-local.ps1).
// It must be signed and installed under Program Files, or Windows refuses to start it,
// so dev runs and CI builds keep Tauri's default manifest.
fn main() {
    println!("cargo:rerun-if-env-changed=MAZECAT_UIACCESS");
    println!("cargo:rerun-if-changed=windows/uiaccess.manifest.xml");
    let mut attrs = tauri_build::Attributes::new();
    if std::env::var("MAZECAT_UIACCESS").as_deref() == Ok("1") {
        attrs = attrs.windows_attributes(
            tauri_build::WindowsAttributes::new()
                .app_manifest(include_str!("windows/uiaccess.manifest.xml")),
        );
    }
    tauri_build::try_build(attrs).expect("failed to run tauri-build");
}
