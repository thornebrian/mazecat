//! Maze Cat: task lists kept as plain YAML files, one project per file.
//! Files live in the local data folder, under MazeCat/Notes
//! (on Windows: %LOCALAPPDATA%\MazeCat\Notes, the same folder the first prototype used).

use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

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


/// Keeps the bar above the taskbar: whenever another window comes to the front
/// (the taskbar, the Start menu, any app), put the bar back on top at once,
/// without taking focus. The 3-second check in the bar stays as a backup.
#[cfg(windows)]
mod on_top {
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, EVENT_SYSTEM_FOREGROUND, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
    };

    static BAR: AtomicIsize = AtomicIsize::new(0);

    fn raise() {
        let h = BAR.load(Ordering::Relaxed);
        if h != 0 {
            unsafe { SetWindowPos(h as HWND, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
        }
    }

    static BURST: AtomicBool = AtomicBool::new(false);

    unsafe extern "system" fn on_foreground(_: HWINEVENTHOOK, _: u32, _: HWND, _: i32, _: i32, _: u32, _: u32) {
        raise();
        // The taskbar raises itself again a moment after it takes focus, so answer
        // with a short burst of re-raises instead of a single one.
        if !BURST.swap(true, Ordering::AcqRel) {
            std::thread::spawn(|| {
                for ms in [30u64, 90, 200, 400, 800, 1500] {
                    std::thread::sleep(std::time::Duration::from_millis(ms));
                    raise();
                }
                BURST.store(false, Ordering::Release);
            });
        }
    }

    pub fn watch(bar: isize) {
        BAR.store(bar, Ordering::Relaxed);
        unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                std::ptr::null_mut(),
                Some(on_foreground),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );
        }
        raise();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // One copy at a time: opening Maze Cat again brings the running bar to the front.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(bar) = app.get_webview_window("bar") {
                let _ = bar.show();
                let _ = bar.unminimize();
                let _ = bar.set_always_on_top(true);
                let _ = bar.set_focus();
            }
        }))
        .setup(|_app| {
            #[cfg(windows)]
            if let Some(bar) = _app.get_webview_window("bar") {
                if let Ok(h) = bar.hwnd() {
                    on_top::watch(h.0 as isize);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_projects, read_project, write_project])
        .run(tauri::generate_context!())
        .expect("Maze Cat couldn't start");
}
