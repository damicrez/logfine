use std::fs;
use std::path::Path;
use anyhow::Result;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use serde::Serialize;

use crate::models::{LogDb, TaskDb};

#[derive(Serialize)]
pub struct ExportedTask {
    pub priority: Option<String>,
    pub completion_date: Option<String>,
    pub creation_date: Option<String>,
    pub project_tags: Vec<String>,
    pub context_tags: Vec<String>,
    pub key_value_tags: serde_json::Value,
    pub raw_line: String,
    pub is_completed: bool,
}

#[derive(Serialize)]
pub struct ExportedLog {
    pub date: String,
    pub energy: i32,
    pub mvos: Vec<String>,
    pub worked: Vec<String>,
    pub failed: Vec<String>,
    pub output: Vec<String>,
    pub tasks: Vec<ExportedTask>,
}

pub fn export_database_to_json(
    db_connection: &mut SqliteConnection,
    days: usize,
    output_path: &Path,
) -> Result<()> {
    use crate::schema::logs::dsl::*;

    // 1. Fetch log entries
    let log_entries = logs
        .order(log_date.desc())
        .limit(days as i64)
        .load::<LogDb>(db_connection)?;

    // 2. Fetch all tasks for these logs
    let task_entries = TaskDb::belonging_to(&log_entries)
        .load::<TaskDb>(db_connection)?
        .grouped_by(&log_entries);

    // 3. Construct ExportedLog list
    let mut exported_logs = Vec::new();
    for (log, log_tasks) in log_entries.into_iter().zip(task_entries) {
        let exported_tasks: Vec<ExportedTask> = log_tasks
            .into_iter()
            .map(|t| {
                let kv_tags: serde_json::Value = serde_json::from_str(&t.key_value_tags)
                    .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
                ExportedTask {
                    priority: t.priority,
                    completion_date: t.completion_date,
                    creation_date: t.creation_date,
                    project_tags: t.project_tag.as_deref()
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or_default(),
                    context_tags: t.context_tag.as_deref()
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or_default(),
                    key_value_tags: kv_tags,
                    raw_line: t.raw_line,
                    is_completed: t.is_completed,
                }
            })
            .collect();

        let mvos_parsed: Vec<String> = serde_json::from_str(&log.mvos).unwrap_or_default();
        let worked_parsed: Vec<String> = serde_json::from_str(&log.worked).unwrap_or_default();
        let failed_parsed: Vec<String> = serde_json::from_str(&log.failed).unwrap_or_default();
        let output_parsed: Vec<String> = serde_json::from_str(&log.output).unwrap_or_default();

        exported_logs.push(ExportedLog {
            date: log.log_date,
            energy: log.energy,
            mvos: mvos_parsed,
            worked: worked_parsed,
            failed: failed_parsed,
            output: output_parsed,
            tasks: exported_tasks,
        });
    }

    // Reverse to sort oldest to newest (since query retrieved newest first)
    exported_logs.reverse();

    // 4. Write to JSON file
    let json_data = serde_json::to_string_pretty(&exported_logs)?;
    fs::write(output_path, json_data)?;

    Ok(())
}
