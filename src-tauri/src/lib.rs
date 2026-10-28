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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![list_projects, read_project, write_project])
        .run(tauri::generate_context!())
        .expect("Maze Cat couldn't start");
}
