use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct Todo {
    pub id: i64,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct Campfire {
    pub id: i64,
    pub bucket_id: i64,
    pub title: String,
    pub project_name: String,
}

#[derive(Debug, Clone)]
pub struct PendingTodo {
    pub id: i64,
    pub title: String,
    pub assignee: Option<String>,
    pub due_on: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ChatLine {
    pub author: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct LoggedEntry {
    pub project_id: Option<i64>,
    pub project_name: String,
    pub date: String,
    pub hours: f64,
    pub description: String,
    pub tag: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedEntry {
    pub project_id: i64,
    pub project_name: String,
    pub hours: String,
    pub description: String,
    pub date: String,
    pub todo_id: Option<i64>,
    pub todo_title: Option<String>,
    pub id: String,
}

impl StagedEntry {
    pub fn new(
        project_id: i64,
        project_name: String,
        hours: String,
        description: String,
        date: String,
        todo_id: Option<i64>,
        todo_title: Option<String>,
    ) -> Self {
        let id = uuid::Uuid::new_v4().to_string().replace('-', "");
        Self { project_id, project_name, hours, description, date, todo_id, todo_title, id }
    }

    pub fn hours_float(&self) -> f64 {
        parse_hours(&self.hours).unwrap_or(0.0)
    }
}

pub fn parse_hours(text: &str) -> Result<f64, String> {
    let text = text.trim().replace(',', ".");
    if text.is_empty() {
        return Err("introduce las horas".to_string());
    }
    let value = if let Some(colon_pos) = text.find(':') {
        let h_str = &text[..colon_pos];
        let m_str = &text[colon_pos + 1..];
        let h: i64 = if h_str.is_empty() { 0 } else { h_str.parse().map_err(|_| "horas inválidas")? };
        let m: i64 = if m_str.is_empty() { 0 } else { m_str.parse().map_err(|_| "minutos inválidos")? };
        if m < 0 || m >= 60 {
            return Err("los minutos deben ser 0-59".to_string());
        }
        h as f64 + m as f64 / 60.0
    } else {
        text.parse::<f64>().map_err(|_| "número inválido".to_string())?
    };
    if value <= 0.0 {
        return Err("debe ser mayor que 0".to_string());
    }
    Ok((value * 10000.0).round() / 10000.0)
}

pub fn round_up_5min(seconds: f64) -> u64 {
    if seconds <= 0.0 {
        return 0;
    }
    let minutes = (seconds / 60.0).ceil() as u64;
    ((minutes + 4) / 5) * 5
}

pub fn minutes_to_hms(minutes: u64) -> String {
    format!("{}:{:02}", minutes / 60, minutes % 60)
}
