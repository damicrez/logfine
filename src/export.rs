use std::fs;
use std::path::Path;
use anyhow::Result;
use chrono::NaiveDate;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use serde::{Deserialize, Serialize};

use crate::models::{LogDb, TaskDb};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportFilter {
    Days(usize),
    Range {
        start: Option<NaiveDate>,
        end: NaiveDate,
    },
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
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
    filter: ExportFilter,
    output_path: &Path,
) -> Result<usize> {
    use crate::schema::logs::dsl::*;

    // 1. Fetch log entries according to filter
    let log_entries = match filter {
        ExportFilter::Days(days) => {
            let mut entries = logs
                .order(log_date.desc())
                .limit(days as i64)
                .load::<LogDb>(db_connection)?;
            entries.reverse();
            entries
        }
        ExportFilter::Range { start, end } => {
            let mut query = logs.into_boxed();
            if let Some(start_date) = start {
                query = query.filter(log_date.ge(start_date.format("%Y-%m-%d").to_string()));
            }
            query = query.filter(log_date.le(end.format("%Y-%m-%d").to_string()));
            query.order(log_date.asc()).load::<LogDb>(db_connection)?
        }
    };

    if log_entries.is_empty() {
        return Ok(0);
    }

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

    let count = exported_logs.len();

    // 4. Write to JSON file
    let json_data = serde_json::to_string_pretty(&exported_logs)?;
    fs::write(output_path, json_data)?;

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::MIGRATIONS;
    use crate::models::{NewLogDb, NewTaskDb};
    use diesel_migrations::MigrationHarness;
    use std::fs;

    fn setup_test_db() -> SqliteConnection {
        let mut conn = SqliteConnection::establish(":memory:").expect("Failed to connect to in-memory DB");
        conn.run_pending_migrations(MIGRATIONS).expect("Failed to run migrations");
        conn
    }

    #[test]
    fn test_export_by_range_and_inclusivity() {
        use crate::schema::logs::dsl::*;
        use crate::schema::tasks::dsl::*;

        let mut conn = setup_test_db();

        // Insert logs for 2026-08-01, 2026-08-15, 2026-09-01
        let log_dates = vec!["2026-08-01", "2026-08-15", "2026-09-01"];
        for d in &log_dates {
            diesel::insert_into(logs)
                .values(&NewLogDb {
                    log_date: d,
                    energy: 4,
                    mvos: "[\"Exercise\"]",
                    worked: "[]",
                    failed: "[]",
                    output: "[]",
                })
                .execute(&mut conn)
                .unwrap();
        }

        let inserted_logs = logs.order(log_date.asc()).load::<LogDb>(&mut conn).unwrap();
        // Insert task for log 2026-08-15
        diesel::insert_into(tasks)
            .values(&NewTaskDb {
                log_id: inserted_logs[1].id,
                priority: Some("A".to_string()),
                completion_date: None,
                creation_date: Some("2026-08-15".to_string()),
                project_tag: None,
                context_tag: None,
                key_value_tags: "{}".to_string(),
                raw_line: "(A) Mid-month task",
                is_completed: false,
            })
            .execute(&mut conn)
            .unwrap();

        let temp_file = std::env::temp_dir().join("test_export_range.json");

        // Test inclusive range: 2026-08-01 to 2026-08-15
        let filter = ExportFilter::Range {
            start: Some(NaiveDate::from_ymd_opt(2026, 8, 1).unwrap()),
            end: NaiveDate::from_ymd_opt(2026, 8, 15).unwrap(),
        };

        let count = export_database_to_json(&mut conn, filter, &temp_file).unwrap();
        assert_eq!(count, 2);
        assert!(temp_file.exists());

        let content = fs::read_to_string(&temp_file).unwrap();
        let parsed: Vec<ExportedLog> = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].date, "2026-08-01");
        assert_eq!(parsed[1].date, "2026-08-15");
        assert_eq!(parsed[1].tasks.len(), 1);
        assert_eq!(parsed[1].tasks[0].raw_line, "(A) Mid-month task");

        let _ = fs::remove_file(&temp_file);
    }

    #[test]
    fn test_export_empty_range_does_not_create_file() {
        let mut conn = setup_test_db();
        let temp_file = std::env::temp_dir().join("test_export_empty.json");
        if temp_file.exists() {
            let _ = fs::remove_file(&temp_file);
        }

        let filter = ExportFilter::Range {
            start: Some(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
            end: NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
        };

        let count = export_database_to_json(&mut conn, filter, &temp_file).unwrap();
        assert_eq!(count, 0);
        assert!(!temp_file.exists(), "File should not be created if no logs exist");
    }

    #[test]
    fn test_export_until_end_only() {
        use crate::schema::logs::dsl::*;
        let mut conn = setup_test_db();

        for d in &["2026-07-01", "2026-08-01", "2026-09-01"] {
            diesel::insert_into(logs)
                .values(&NewLogDb {
                    log_date: d,
                    energy: 3,
                    mvos: "[]",
                    worked: "[]",
                    failed: "[]",
                    output: "[]",
                })
                .execute(&mut conn)
                .unwrap();
        }

        let temp_file = std::env::temp_dir().join("test_export_until.json");
        let filter = ExportFilter::Range {
            start: None,
            end: NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
        };

        let count = export_database_to_json(&mut conn, filter, &temp_file).unwrap();
        assert_eq!(count, 2);
        let content = fs::read_to_string(&temp_file).unwrap();
        let parsed: Vec<ExportedLog> = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].date, "2026-07-01");
        assert_eq!(parsed[1].date, "2026-08-01");

        let _ = fs::remove_file(&temp_file);
    }
}
