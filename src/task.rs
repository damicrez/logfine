use std::collections::HashMap;
use std::sync::LazyLock;
use chrono::{DateTime, NaiveDate, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Represents a parsed todo.txt task
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub description: String,
    pub priority: Option<char>,
    pub completion_date: Option<DateTime<Utc>>,
    pub creation_date: Option<DateTime<Utc>>,
    pub project_tags: Vec<String>,
    pub context_tags: Vec<String>,
    pub key_value_tags: HashMap<String, String>,
}

impl Task {
    /// Serializes project tags to JSON for database storage, returns None if empty
    pub fn project_tags_json(&self) -> Option<String> {
        if self.project_tags.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&self.project_tags)
                .expect("Vec<String> serialization is infallible"))
        }
    }

    /// Serializes context tags to JSON for database storage, returns None if empty
    pub fn context_tags_json(&self) -> Option<String> {
        if self.context_tags.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&self.context_tags)
                .expect("Vec<String> serialization is infallible"))
        }
    }
}

static TODO_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(x )?(?:\(([A-Z])\) )?([0-9]{4}-[0-9]{2}-[0-9]{2} )?([0-9]{4}-[0-9]{2}-[0-9]{2} )?(.*)$").unwrap()
});

pub fn parse_task(line: &str) -> Option<Task> {
    if line.trim().is_empty() {
        return None;
    }
    let captures = TODO_REGEX.captures(line)?;
    
    let is_completed = captures.get(1).is_some();
    let priority = captures.get(2).and_then(|m| m.as_str().chars().next());
    
    let date1_str = captures.get(3).map(|m| m.as_str().trim());
    let date2_str = captures.get(4).map(|m| m.as_str().trim());
    
    let mut completion_date = None;
    let mut creation_date = None;

    if is_completed {
        if let Some(d1) = date1_str.and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()) {
            completion_date = d1.and_hms_opt(0, 0, 0).map(|d| d.and_utc());
            if let Some(d2) = date2_str.and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()) {
                creation_date = d2.and_hms_opt(0, 0, 0).map(|d| d.and_utc());
            }
        }
    } else if let Some(d1) = date1_str.and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()) {
        creation_date = d1.and_hms_opt(0, 0, 0).map(|d| d.and_utc());
    }

    let description = captures.get(5).map(|m| m.as_str()).unwrap_or("");
    
    let mut project_tags = Vec::new();
    let mut context_tags = Vec::new();
    let mut key_value_tags = HashMap::new();

    for item in description.split_whitespace() {
        if item.starts_with('+') && item.len() > 1 {
            project_tags.push(item[1..].to_string());
        } else if item.starts_with('@') && item.len() > 1 {
            context_tags.push(item[1..].to_string());
        } else if let Some((key, value)) = item.split_once(':').filter(|(k, v)| !k.is_empty() && !v.is_empty()) {
            key_value_tags.insert(key.to_string(), value.to_string());
        }
    }

    Some(Task {
        description: description.to_string(),
        priority,
        completion_date,
        creation_date,
        project_tags,
        context_tags,
        key_value_tags,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_task() {
        let task = parse_task("Buy milk").unwrap();
        assert_eq!(task.description, "Buy milk");
        assert_eq!(task.priority, None);
        assert_eq!(task.creation_date, None);
        assert_eq!(task.completion_date, None);
        assert!(task.project_tags.is_empty());
        assert!(task.context_tags.is_empty());
    }

    #[test]
    fn test_parse_task_with_priority_and_tags() {
        let task = parse_task("(A) Complete report +work @office due:2026-09-03").unwrap();
        assert_eq!(task.priority, Some('A'));
        assert_eq!(task.project_tags, vec!["work".to_string()]);
        assert_eq!(task.context_tags, vec!["office".to_string()]);
        assert_eq!(task.key_value_tags.get("due"), Some(&"2026-09-03".to_string()));
    }

    #[test]
    fn test_parse_completed_task_with_dates() {
        let task = parse_task("x 2026-09-02 2026-09-01 Read rust book").unwrap();
        assert!(task.completion_date.is_some());
        assert!(task.creation_date.is_some());
        assert_eq!(task.description, "Read rust book");
    }

    #[test]
    fn test_parse_empty_line() {
        assert!(parse_task("").is_none());
        assert!(parse_task("   ").is_none());
    }
}
