use std::process::Command;
use anyhow::Result;
use chrono::{Duration, Local};

use crate::config::{today_iso};
use crate::models::{Campfire, ChatLine, LoggedEntry, PendingTodo, Project, Todo};
use crate::storage::{load_recording_cache, save_recording_cache};

const CLI: &str = "basecamp";
const TIMEOUT_SECS: u64 = 60;

#[derive(Debug)]
pub struct BasecampError {
    pub message: String,
    pub auth: bool,
}

impl std::fmt::Display for BasecampError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

fn is_auth_error(msg: &str) -> bool {
    let m = msg.to_lowercase();
    ["401", "403", "unauthor", "forbidden", "oauth", "token", "expired", "scope"]
        .iter()
        .any(|t| m.contains(t))
}

fn run_cli(args: &[&str]) -> Result<serde_json::Value, BasecampError> {
    let output = Command::new(CLI)
        .args(args)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                BasecampError {
                    message: "`basecamp` CLI not found. Install: https://basecamp.com/install-cli".to_string(),
                    auth: false,
                }
            } else {
                BasecampError { message: e.to_string(), auth: false }
            }
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let data: Option<serde_json::Value> = if !stdout.trim().is_empty() {
        serde_json::from_str(stdout.trim()).ok()
    } else {
        None
    };

    if let Some(ref v) = data {
        if v.get("ok").and_then(|o| o.as_bool()) == Some(false) {
            let msg = v["error"].as_str().unwrap_or("request failed").to_string();
            return Err(BasecampError { auth: is_auth_error(&msg), message: msg });
        }
    }

    if data.is_none() && !output.status.success() {
        let err = stderr.trim().to_string();
        let msg = if err.is_empty() {
            format!("exit code {}", output.status.code().unwrap_or(-1))
        } else {
            // Try to parse JSON error from stderr
            serde_json::from_str::<serde_json::Value>(&err)
                .ok()
                .and_then(|v| v["error"].as_str().map(|s| s.to_string()))
                .unwrap_or(err)
        };
        return Err(BasecampError { auth: is_auth_error(&msg), message: msg });
    }

    Ok(data.unwrap_or(serde_json::Value::Null))
}

fn api_get(path: &str) -> Result<serde_json::Value, BasecampError> {
    run_cli(&["api", "get", path, "-q"])
}

fn api_post(path: &str, body: &str) -> Result<serde_json::Value, BasecampError> {
    run_cli(&["api", "post", path, "-q", "-d", body])
}

fn get_all(path: &str) -> Result<Vec<serde_json::Value>, BasecampError> {
    let mut results = Vec::new();
    for page in 1..=50 {
        let sep = if path.contains('?') { "&" } else { "?" };
        let paged = format!("{}{sep}page={page}", path);
        let data = api_get(&paged)?;
        match data.as_array() {
            Some(arr) if !arr.is_empty() => results.extend(arr.iter().cloned()),
            _ => break,
        }
    }
    Ok(results)
}

fn current_person_id() -> Option<i64> {
    api_get("/my/profile.json")
        .ok()
        .and_then(|v| v["id"].as_i64())
}

pub fn my_name() -> Option<String> {
    api_get("/my/profile.json")
        .ok()
        .and_then(|v| v["name"].as_str().map(|s| s.to_string()))
}

pub fn list_projects() -> Result<Vec<Project>, BasecampError> {
    let raw = get_all("/projects.json")?;
    let mut projects: Vec<Project> = raw
        .iter()
        .filter(|p| p["status"].as_str().unwrap_or("active") == "active")
        .map(|p| Project {
            id: p["id"].as_i64().unwrap_or(0),
            name: p["name"].as_str().unwrap_or("").to_string(),
            updated_at: p["updated_at"].as_str().unwrap_or("").to_string(),
        })
        .collect();
    projects.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(projects)
}

pub fn my_window_entries(days: u32) -> Result<Vec<LoggedEntry>, BasecampError> {
    let start = (Local::now() - Duration::days(days as i64)).format("%Y-%m-%d").to_string();
    let end = today_iso();
    let me = current_person_id();
    let path = format!("/reports/timesheet.json?start_date={start}&end_date={end}");
    let data = api_get(&path)?;
    let arr = match data.as_array() {
        Some(a) => a,
        None => return Ok(vec![]),
    };
    let mut out = Vec::new();
    for e in arr {
        let person_id = e["person"]["id"].as_i64();
        if me.is_some() && person_id != me {
            continue;
        }
        let parent = &e["parent"];
        let parent_type = parent["type"].as_str().unwrap_or("");
        let tag = if parent_type != "Timesheet" && !parent_type.is_empty() {
            parent["title"].as_str().map(|s| s.to_string())
        } else {
            None
        };
        let hours = e["hours"].as_str()
            .and_then(|h| h.parse::<f64>().ok())
            .or_else(|| e["hours"].as_f64())
            .unwrap_or(0.0);
        out.push(LoggedEntry {
            project_id: e["bucket"]["id"].as_i64(),
            project_name: e["bucket"]["name"].as_str().unwrap_or("").to_string(),
            date: e["date"].as_str().unwrap_or("").to_string(),
            hours,
            description: e["description"].as_str().unwrap_or("").to_string(),
            tag,
            created_at: e["created_at"].as_str().unwrap_or("").to_string(),
        });
    }
    Ok(out)
}

pub fn project_timesheet_recording_id(project_id: i64) -> Option<i64> {
    let mut cache = load_recording_cache();
    let key = project_id.to_string();
    if let Some(&rid) = cache.get(&key) {
        return Some(rid);
    }
    let path = format!("/projects/{project_id}/timesheet.json");
    let data = api_get(&path).ok()?;
    let arr = data.as_array()?;
    for entry in arr {
        let parent = &entry["parent"];
        if parent["type"].as_str() == Some("Timesheet") {
            if let Some(rid) = parent["id"].as_i64() {
                cache.insert(key, rid);
                save_recording_cache(&cache);
                return Some(rid);
            }
        }
    }
    None
}

pub fn list_todos(project_id: i64) -> Result<Vec<Todo>, BasecampError> {
    let path = format!("/projects/recordings.json?type=Todo&bucket={project_id}");
    let raw = get_all(&path)?;
    let todos = raw
        .iter()
        .filter(|t| t["completed"].as_bool() != Some(true))
        .map(|t| {
            let title = t["title"].as_str()
                .or_else(|| t["content"].as_str())
                .unwrap_or("Todo")
                .to_string();
            Todo { id: t["id"].as_i64().unwrap_or(0), title }
        })
        .collect();
    Ok(todos)
}

pub fn list_pending_todos(project_id: i64) -> Result<Vec<PendingTodo>, BasecampError> {
    let path = format!("/projects/recordings.json?type=Todo&bucket={project_id}");
    let raw = get_all(&path)?;
    let todos = raw
        .iter()
        .filter(|t| t["completed"].as_bool() != Some(true) && t["status"].as_str() != Some("trashed"))
        .map(|t| {
            let title = t["title"].as_str()
                .or_else(|| t["content"].as_str())
                .unwrap_or("Todo")
                .to_string();
            let assignee = t["assignee"]["name"].as_str().map(|s| s.to_string());
            let due_on = t["due_on"].as_str().map(|s| s.to_string());
            PendingTodo { id: t["id"].as_i64().unwrap_or(0), title, assignee, due_on }
        })
        .collect();
    Ok(todos)
}

pub fn get_first_todolist(project_id: i64) -> Result<Option<i64>, BasecampError> {
    let path = format!("/projects/{project_id}/todolists.json");
    let data = api_get(&path)?;
    let list_id = data.as_array()
        .and_then(|arr| arr.iter().find(|l| l["status"].as_str() != Some("trashed")))
        .and_then(|l| l["id"].as_i64());
    Ok(list_id)
}

pub fn create_todo(project_id: i64, todolist_id: i64, content: &str) -> Result<(), BasecampError> {
    let path = format!("/buckets/{project_id}/todolists/{todolist_id}/todos.json");
    let body = serde_json::json!({ "content": content });
    api_post(&path, &body.to_string())?;
    Ok(())
}

pub fn create_entry(
    recording_id: i64,
    date: &str,
    hours: &str,
    description: &str,
) -> Result<(), BasecampError> {
    let mut body = serde_json::json!({
        "date": date,
        "hours": hours,
    });
    if !description.is_empty() {
        body["description"] = serde_json::Value::String(description.to_string());
    }
    let path = format!("/recordings/{recording_id}/timesheet/entries.json");
    api_post(&path, &body.to_string())?;
    Ok(())
}

pub fn list_campfires() -> Result<Vec<Campfire>, BasecampError> {
    let data = api_get("/chats.json")?;
    let arr = data.as_array().cloned().unwrap_or_default();
    let mut rooms: Vec<Campfire> = arr
        .iter()
        .map(|c| Campfire {
            id: c["id"].as_i64().unwrap_or(0),
            bucket_id: c["bucket"]["id"].as_i64().unwrap_or(0),
            title: c["title"].as_str().unwrap_or("Campfire").to_string(),
            project_name: c["bucket"]["name"].as_str().unwrap_or("").to_string(),
        })
        .collect();
    rooms.sort_by(|a, b| a.project_name.to_lowercase().cmp(&b.project_name.to_lowercase()));
    Ok(rooms)
}

pub fn chat_lines(bucket_id: i64, chat_id: i64) -> Result<Vec<ChatLine>, BasecampError> {
    let path = format!("/buckets/{bucket_id}/chats/{chat_id}/lines.json");
    let data = api_get(&path)?;
    let arr = data.as_array().cloned().unwrap_or_default();
    let lines: Vec<ChatLine> = arr
        .iter()
        .rev()
        .map(|line| ChatLine {
            author: line["creator"]["name"].as_str().unwrap_or("?").to_string(),
            text: strip_html(line["content"].as_str().unwrap_or("")),
        })
        .collect();
    Ok(lines)
}

pub fn post_chat_line(bucket_id: i64, chat_id: i64, text: &str) -> Result<(), BasecampError> {
    let escaped = text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let body = serde_json::json!({ "content": format!("<div>{escaped}</div>") });
    let path = format!("/buckets/{bucket_id}/chats/{chat_id}/lines.json");
    api_post(&path, &body.to_string())?;
    Ok(())
}

fn strip_html(content: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in content.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    // Unescape common HTML entities
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .trim()
        .to_string()
}
