//! Jira Sync: read issues from a Jira Cloud site and change their state. Nothing else.
//! Maze Cat never creates, edits or deletes issues in this mode (Jira stays the source of truth).
//!
//! - The site and email live in MazeCat/jira.json. The API token lives in the operating system's
//!   credential store (Windows Credential Manager); it is never written to a file or sent to the windows.
//! - The last successful fetch is kept in MazeCat/jira-cache.json, so lists still show when offline.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

const KEYRING_SERVICE: &str = "MazeCat Jira";
const DEFAULT_JQL: &str = "assignee = currentUser() AND (statusCategory != Done OR resolved >= -7d)";
const ORDER: &str = " ORDER BY project ASC, Rank ASC";
const MAX_ISSUES: usize = 300;
/// The backlog can hold hundreds of items; show only the top ones by rank.
const BACKLOG_TOP: usize = 25;

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct JiraConfig {
    pub site: String,
    pub email: String,
    #[serde(default)]
    pub jql: String,
}

/// One issue, reduced to what Maze Cat shows.
#[derive(Serialize, Deserialize, Clone)]
pub struct Issue {
    pub key: String,
    pub summary: String,
    pub project_key: String,
    pub project_name: String,
    /// Jira's status category: "new" (to do), "indeterminate" (in progress) or "done".
    pub category: String,
    pub labels: Vec<String>,
    /// Original estimate in seconds, when set.
    pub estimate: Option<u64>,
    /// In an active sprint (TODO / WIP); otherwise it waits in the backlog (PLANNED).
    #[serde(default)]
    pub in_sprint: bool,
}

#[derive(Serialize, Clone)]
pub struct Snapshot {
    pub issues: Vec<Issue>,
    /// True when Jira couldn't be reached and these are the last saved results.
    pub stale: bool,
    pub message: String,
    /// True when the backlog holds more than the top items shown.
    #[serde(default)]
    pub backlog_more: bool,
}

#[derive(Default)]
pub struct JiraState(pub Mutex<Option<(Vec<Issue>, bool)>>);

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().local_data_dir().map_err(|e| e.to_string())?.join("MazeCat");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn read_config(app: &AppHandle) -> Option<JiraConfig> {
    let text = fs::read_to_string(data_dir(app).ok()?.join("jira.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// Only an https address on a host name, nothing else (no paths, no tricks).
fn checked_site(site: &str) -> Result<String, String> {
    let s = site.trim().trim_end_matches('/');
    let host = s.strip_prefix("https://").ok_or("The Jira address must start with https://")?;
    let ok = !host.is_empty() && host.len() <= 253 && host.contains('.')
        && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
    if ok { Ok(format!("https://{host}")) } else { Err("That doesn't look like a Jira address, e.g. https://yourteam.atlassian.net".into()) }
}

fn checked_key(key: &str) -> Result<&str, String> {
    let mut parts = key.splitn(2, '-');
    let (p, n) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let ok = !p.is_empty() && p.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && p.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && !n.is_empty() && n.chars().all(|c| c.is_ascii_digit());
    if ok { Ok(key) } else { Err(format!("\"{key}\" isn't a Jira issue key")) }
}

fn token_entry(cfg: &JiraConfig) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, &format!("{}|{}", cfg.site, cfg.email)).map_err(|e| e.to_string())
}

fn client(cfg: &JiraConfig) -> Result<(reqwest::Client, String), String> {
    let token = token_entry(cfg)?.get_password().map_err(|_| "No Jira token saved yet. Add it in Jira settings.".to_string())?;
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("MazeCat")
        .build()
        .map_err(|e| e.to_string())?;
    Ok((http, token))
}

fn plain_error(status: reqwest::StatusCode) -> String {
    match status.as_u16() {
        401 => "Jira didn't accept the email and token. Check them in Jira settings.".into(),
        403 => "Jira says this account can't do that.".into(),
        404 => "Jira couldn't find that. Check the address in Jira settings.".into(),
        429 => "Jira asked us to slow down. Try again in a minute.".into(),
        s => format!("Jira answered with an error ({s})."),
    }
}

#[tauri::command]
pub fn jira_config(app: AppHandle) -> Option<JiraConfig> {
    read_config(&app)
}

/// Saves the site, email and (optionally) a new token. The token goes to the credential store only.
#[tauri::command]
pub fn jira_save(app: AppHandle, site: String, email: String, jql: String, token: Option<String>) -> Result<(), String> {
    let cfg = JiraConfig { site: checked_site(&site)?, email: email.trim().to_string(), jql: jql.trim().to_string() };
    if cfg.email.is_empty() || !cfg.email.contains('@') { return Err("Add the email you sign in to Jira with.".into()); }
    if cfg.jql.len() > 2000 { return Err("That filter is too long.".into()); }
    if let Some(t) = token.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()) {
        token_entry(&cfg)?.set_password(&t).map_err(|e| format!("Couldn't save the token: {e}"))?;
    }
    let text = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
    fs::write(data_dir(&app)?.join("jira.json"), text).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct SearchPage { #[serde(default)] issues: Vec<RawIssue>, #[serde(rename = "nextPageToken")] next: Option<String> }
#[derive(Deserialize)]
struct RawIssue { key: String, fields: RawFields }
#[derive(Deserialize)]
struct RawFields {
    #[serde(default)] summary: String,
    status: Option<RawStatus>,
    project: Option<RawProject>,
    #[serde(default)] labels: Vec<String>,
    timeoriginalestimate: Option<u64>,
}
#[derive(Deserialize)]
struct RawStatus { #[serde(rename = "statusCategory")] category: Option<RawCategory> }
#[derive(Deserialize)]
struct RawCategory { key: String }
#[derive(Deserialize)]
struct RawProject { key: String, name: String }

/// The filter without its ORDER BY, so sprint conditions can be added to it.
fn base_jql(cfg: &JiraConfig) -> String {
    let j = if cfg.jql.is_empty() { DEFAULT_JQL } else { cfg.jql.as_str() };
    let cut = j.to_ascii_lowercase().find(" order by").unwrap_or(j.len());
    j[..cut].trim().to_string()
}

/// Sprint issues first, then the backlog. Projects without sprints (or a site without
/// Jira Software) fall back to one plain search, all treated as in the sprint.
async fn fetch(cfg: &JiraConfig) -> Result<(Vec<Issue>, bool), String> {
    let (http, token) = client(cfg)?;
    let base = base_jql(cfg);
    let sprint = format!("({base}) AND sprint in openSprints(){ORDER}");
    match search(&http, &token, cfg, &sprint, MAX_ISSUES).await {
        Ok(mut list) => {
            for i in list.iter_mut() { i.in_sprint = true; }
            let backlog = format!("({base}) AND (sprint is EMPTY OR sprint not in openSprints()) AND statusCategory != Done{ORDER}");
            let mut more = false;
            if let Ok(mut rest) = search(&http, &token, cfg, &backlog, BACKLOG_TOP + 1).await {
                more = rest.len() > BACKLOG_TOP;
                rest.truncate(BACKLOG_TOP);
                list.extend(rest);
            }
            Ok((list, more))
        }
        Err(_) => {
            let mut list = search(&http, &token, cfg, &format!("{base}{ORDER}"), MAX_ISSUES).await?;
            for i in list.iter_mut() { i.in_sprint = true; }
            Ok((list, false))
        }
    }
}

async fn search(http: &reqwest::Client, token: &str, cfg: &JiraConfig, jql: &str, limit: usize) -> Result<Vec<Issue>, String> {
    let mut out = Vec::new();
    let mut next: Option<String> = None;
    loop {
        let mut body = serde_json::json!({
            "jql": jql, "maxResults": limit.min(100),
            "fields": ["summary", "status", "project", "labels", "timeoriginalestimate"]
        });
        if let Some(t) = &next { body["nextPageToken"] = serde_json::Value::String(t.clone()); }
        let res = http.post(format!("{}/rest/api/3/search/jql", cfg.site))
            .basic_auth(&cfg.email, Some(token))
            .json(&body).send().await
            .map_err(|_| "Couldn't reach Jira. Showing the last saved lists.".to_string())?;
        if !res.status().is_success() { return Err(plain_error(res.status())); }
        let page: SearchPage = res.json().await.map_err(|_| "Jira sent something Maze Cat couldn't read.".to_string())?;
        for r in page.issues {
            let (pk, pn) = r.fields.project.map(|p| (p.key, p.name)).unwrap_or_default();
            out.push(Issue {
                key: r.key, summary: r.fields.summary, project_key: pk, project_name: pn,
                category: r.fields.status.and_then(|s| s.category).map(|c| c.key).unwrap_or_else(|| "new".into()),
                labels: r.fields.labels, estimate: r.fields.timeoriginalestimate, in_sprint: false,
            });
        }
        next = page.next;
        if next.is_none() || out.len() >= limit { break; }
    }
    Ok(out)
}

fn cache_path(app: &AppHandle) -> Result<PathBuf, String> { Ok(data_dir(app)?.join("jira-cache.json")) }

/// The issues to show. `refresh` asks Jira; otherwise the copy in memory (or on disk) is used.
#[tauri::command]
pub async fn jira_issues(app: AppHandle, state: State<'_, JiraState>, refresh: bool) -> Result<Snapshot, String> {
    let cfg = read_config(&app).ok_or("Jira Sync isn't set up yet. Add your Jira address, email and token in Jira settings.")?;
    if !refresh {
        if let Some((list, more)) = state.0.lock().map_err(|e| e.to_string())?.clone() {
            return Ok(Snapshot { issues: list, stale: false, message: String::new(), backlog_more: more });
        }
    }
    match fetch(&cfg).await {
        Ok((list, more)) => {
            if let Ok(text) = serde_json::to_string(&list) { let _ = fs::write(cache_path(&app)?, text); }
            *state.0.lock().map_err(|e| e.to_string())? = Some((list.clone(), more));
            Ok(Snapshot { issues: list, stale: false, message: String::new(), backlog_more: more })
        }
        Err(message) => {
            let saved: Vec<Issue> = fs::read_to_string(cache_path(&app)?).ok()
                .and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
            Ok(Snapshot { issues: saved, stale: true, message, backlog_more: false })
        }
    }
}

#[derive(Deserialize)]
struct Transitions { transitions: Vec<Transition> }
#[derive(Deserialize)]
struct Transition { id: String, to: TransitionTo }
#[derive(Deserialize)]
struct TransitionTo { #[serde(rename = "statusCategory")] category: Option<RawCategory> }

/// Moves an issue to a status in the given category ("new", "indeterminate" or "done"),
/// using the first transition the workflow offers. When it moves to done and the timer ran
/// for at least a minute, that time is logged on the issue as work. These are the only
/// writes Maze Cat makes in Jira.
#[tauri::command]
pub async fn jira_move(app: AppHandle, state: State<'_, JiraState>, key: String, category: String, seconds: Option<u64>) -> Result<String, String> {
    let key = checked_key(&key)?.to_string();
    if !["new", "indeterminate", "done"].contains(&category.as_str()) { return Err("Unknown state".into()); }
    let cfg = read_config(&app).ok_or("Jira Sync isn't set up yet.")?;
    let (http, token) = client(&cfg)?;
    let url = format!("{}/rest/api/3/issue/{}/transitions", cfg.site, key);
    let res = http.get(&url).basic_auth(&cfg.email, Some(&token)).send().await.map_err(|_| "Couldn't reach Jira.".to_string())?;
    if !res.status().is_success() { return Err(plain_error(res.status())); }
    let list: Transitions = res.json().await.map_err(|_| "Jira sent something Maze Cat couldn't read.".to_string())?;
    let pick = list.transitions.iter()
        .find(|t| t.to.category.as_ref().is_some_and(|c| c.key == category))
        .ok_or_else(|| format!("Jira's workflow has no direct step to that state for {key}. Change it in Jira."))?;
    let res = http.post(&url).basic_auth(&cfg.email, Some(&token))
        .json(&serde_json::json!({ "transition": { "id": pick.id } })).send().await
        .map_err(|_| "Couldn't reach Jira.".to_string())?;
    if !res.status().is_success() { return Err(plain_error(res.status())); }
    if let Some((list, _)) = state.0.lock().map_err(|e| e.to_string())?.as_mut() {
        if let Some(i) = list.iter_mut().find(|i| i.key == key) { i.category = category.clone(); }
    }
    // Time spent: Jira keeps work logs in whole minutes, so under a minute isn't logged.
    let secs = seconds.unwrap_or(0).min(24 * 3600);
    if category == "done" && secs >= 60 {
        let res = http.post(format!("{}/rest/api/3/issue/{}/worklog", cfg.site, key))
            .basic_auth(&cfg.email, Some(&token))
            .json(&serde_json::json!({ "timeSpentSeconds": secs / 60 * 60 })).send().await;
        return Ok(match res {
            Ok(r) if r.status().is_success() => format!("Done in Jira, {} min logged.", secs / 60),
            _ => "Done in Jira, but the time couldn't be logged.".into(),
        });
    }
    Ok(if category == "done" { "Done in Jira.".into() } else { String::new() })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sites_and_keys() {
        assert_eq!(checked_site("https://govw.atlassian.net/").unwrap(), "https://govw.atlassian.net");
        assert!(checked_site("http://govw.atlassian.net").is_err());
        assert!(checked_site("https://evil.com/path").is_err());
        assert!(checked_key("MIQR-12").is_ok());
        assert!(checked_key("miqr-12").is_err());
        assert!(checked_key("MIQR-12/../x").is_err());
    }
}
