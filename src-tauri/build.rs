// The main app keeps Tauri's default manifest. Only the native bar (bar/) asks for
// uiAccess, the "above the taskbar" permission; see bar/build.rs and scripts/build-local.ps1.
fn main() {
    tauri_build::build()
}
