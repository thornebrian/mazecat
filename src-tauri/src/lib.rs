//! Maze Cat: task lists kept as plain YAML files, one project per file.
//! Files live in the local data folder, under MazeCat/Notes
//! (on Windows: %LOCALAPPDATA%\MazeCat\Notes, the same folder the first prototype used).

use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};

const WELCOME: &str = "Welcome:\n  WIP:\n  - Try Maze Cat by clicking this line, then Start (easy)\n  TODO:\n  - Write your first list with New list (easy)\n  - Click the cat on the bar to show or hide your lists (easy)\n  DONE:\n  - Install Maze Cat (easy)\n";

fn notes_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .local_data_dir()
        .map_err(|e| e.to_string())?
        .join("MazeCat")
        .join("Notes");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// A list name is a file name without its extension: no folders, no tricks.
fn checked(name: &str) -> Result<&str, String> {
    let ok = !name.trim().is_empty()
        && name.len() <= 80
        && !name.contains("..")
        && name.chars().all(|c| c.is_alphanumeric() || " -_.()&'".contains(c));
    if ok { Ok(name) } else { Err(format!("\"{name}\" can't be used as a list name")) }
}

fn file_for(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let name = checked(name)?;
    let dir = notes_dir(app)?;
    let yaml = dir.join(format!("{name}.yaml"));
    let yml = dir.join(format!("{name}.yml"));
    Ok(if !yaml.exists() && yml.exists() { yml } else { yaml })
}

#[tauri::command]
fn list_projects(app: AppHandle) -> Result<Vec<String>, String> {
    let dir = notes_dir(&app)?;
    let mut names: Vec<String> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| matches!(p.extension().and_then(|x| x.to_str()), Some("yaml") | Some("yml")))
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(String::from))
        .collect();
    if names.is_empty() {
        fs::write(dir.join("Welcome.yaml"), WELCOME).map_err(|e| e.to_string())?;
        names.push("Welcome".into());
    }
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    Ok(names)
}

#[tauri::command]
fn read_project(app: AppHandle, name: String) -> Result<String, String> {
    fs::read_to_string(file_for(&app, &name)?).map_err(|e| e.to_string())
}

/// Saves through a temporary file, so a crash mid-write never leaves half a list.
#[tauri::command]
fn write_project(app: AppHandle, name: String, text: String) -> Result<(), String> {
    let path = file_for(&app, &name)?;
    let tmp = path.with_extension("yaml.tmp");
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}


/// The bar that sits on the taskbar is a small native program (mazecat-bar.exe, next to
/// this one). Windows only keeps a window above the taskbar for programs with the uiAccess
/// permission, and the web engine these windows use stops drawing under it, so the bar is
/// drawn natively and the logic stays here. The two talk over a local connection guarded by
/// a one-time token: one JSON message per line. Without the native bar (dev runs, other
/// systems, or if it can't start) the web bar shows instead, as before.
mod native_bar {
    use serde_json::Value;
    use std::io::Write;
    use std::net::TcpStream;
    use std::sync::Mutex;
    use tauri::State;
    #[cfg(windows)]
    use {serde_json::json, tauri::{AppHandle, Emitter}};

    #[derive(Default)]
    pub struct Link {
        out: Mutex<Option<TcpStream>>,
        mode: Mutex<&'static str>,
    }

    #[cfg(windows)]
    fn set_mode(app: &AppHandle, link: &Link, mode: &'static str) {
        *link.mode.lock().unwrap() = mode;
        let _ = app.emit_to("bar", "nbar", json!({ "t": mode }));
    }

    /// "web" (no native bar), "starting", "ready" or "gone".
    #[tauri::command]
    pub fn nbar_mode(link: State<'_, Link>) -> String {
        let m = *link.mode.lock().unwrap();
        if m.is_empty() { "web".into() } else { m.into() }
    }

    #[tauri::command]
    pub fn nbar_send(link: State<'_, Link>, msg: Value) -> Result<(), String> {
        let mut out = link.out.lock().unwrap();
        match out.as_mut() {
            Some(s) => writeln!(s, "{msg}").map_err(|e| e.to_string()),
            None => Ok(()),
        }
    }

    #[cfg(windows)]
    fn token() -> String {
        use std::hash::{BuildHasher, Hasher};
        let mut out = String::new();
        for i in 0..2u64 {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(i);
            h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
            h.write_u32(std::process::id());
            out.push_str(&format!("{:016x}", h.finish()));
        }
        out
    }

    /// A uiAccess program can't be started directly by a normal one (CreateProcess refuses
    /// with "elevation required"); going through the shell lets Windows start it properly.
    #[cfg(windows)]
    fn launch(exe: &std::path::Path, args: &str) -> bool {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNOACTIVATE;
        let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
        let (op, file, params) = (wide("open"), wide(&exe.to_string_lossy()), wide(args));
        let r = unsafe { ShellExecuteW(std::ptr::null_mut(), op.as_ptr(), file.as_ptr(), params.as_ptr(), std::ptr::null(), SW_SHOWNOACTIVATE) };
        r as isize > 32
    }

    #[cfg(windows)]
    pub fn start(app: &AppHandle, link: &Link) {
        use std::io::{BufRead, BufReader};
        use std::net::TcpListener;
        use tauri::Manager;
        let Some(exe) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("mazecat-bar.exe"))) else { return };
        if !exe.exists() {
            return;
        }
        let Ok(listener) = TcpListener::bind("127.0.0.1:0") else { return };
        let Ok(port) = listener.local_addr().map(|a| a.port()) else { return };
        let token = token();
        if !launch(&exe, &format!("--port {port} --token {token}")) {
            return;
        }
        *link.mode.lock().unwrap() = "starting";
        let app = app.clone();
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                let Ok(s) = conn else { continue };
                let Ok(r) = s.try_clone() else { continue };
                // Wait up to 5 s for the hello, on the handle we read from (a clone can
                // keep its own timeout on Windows, so set and clear it on this one).
                let _ = r.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                let mut reader = BufReader::new(r);
                let mut first = String::new();
                let _ = reader.read_line(&mut first);
                let hello: Value = serde_json::from_str(first.trim()).unwrap_or_default();
                if hello["t"] != "hello" || hello["token"].as_str() != Some(token.as_str()) {
                    continue; // not our bar: drop it and keep waiting
                }
                let _ = reader.get_ref().set_read_timeout(None);
                let _ = s.set_nodelay(true);
                let link = app.state::<Link>();
                *link.out.lock().unwrap() = Some(s);
                set_mode(&app, &link, "ready");
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) => break, // the bar closed
                        Ok(_) => {}
                        // A quiet bar is fine: only a real error or the end means it's gone.
                        Err(e) if matches!(e.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted) => continue,
                        Err(_) => break,
                    }
                    let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { continue };
                    if v["t"] == "quit" {
                        app.exit(0);
                        return;
                    }
                    let _ = app.emit_to("bar", "nbar", v);
                }
                *link.out.lock().unwrap() = None;
                set_mode(&app, &link, "gone");
                return;
            }
        });
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // One copy at a time: opening Maze Cat again shows the bar (web) or your lists (native bar).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = app.emit_to("bar", "nbar", serde_json::json!({ "t": "again" }));
        }))
        .manage(native_bar::Link::default())
        .setup(|_app| {
            #[cfg(windows)]
            native_bar::start(_app.handle(), &_app.state::<native_bar::Link>());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_projects,
            read_project,
            write_project,
            native_bar::nbar_mode,
            native_bar::nbar_send
        ])
        .run(tauri::generate_context!())
        .expect("Maze Cat couldn't start");
}
