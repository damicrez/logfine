use std::fs;
use std::path::Path;
use anyhow::Result;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

use crate::cli::{COLOR_INFO, COLOR_RESET, COLOR_SUCCESS, COLOR_WARN};
use crate::models::{LogDb, NewLogDb, NewTaskDb, TaskDb, TodoCacheDb};
use crate::sync::TaskAction;
use crate::task::Task;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

pub fn init_db(db_dir: &Path) -> Result<SqliteConnection> {
    fs::create_dir_all(db_dir)?;
    let db_path = db_dir.join("logfine.db");
    let db_url = db_path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid database path"))?;

    let mut db_connection = SqliteConnection::establish(db_url)?;
    db_connection.run_pending_migrations(MIGRATIONS).map_err(|e| anyhow::anyhow!("Migration error: {}", e))?;
    Ok(db_connection)
}

/// Helper function to load or create today's daily log entry in the database
pub fn get_or_create_log(db_connection: &mut SqliteConnection, date_str: &str) -> Result<LogDb> {
    use crate::schema::logs::dsl::*;

    let existing = logs
        .filter(log_date.eq(date_str))
        .first::<LogDb>(db_connection)
        .optional()?;

    if let Some(log_db) = existing {
        Ok(log_db)
    } else {
        let new_log = NewLogDb {
            log_date: date_str,
            energy: 3,
            mvos: "[]",
            worked: "[]",
            failed: "[]",
            output: "[]",
        };

        let log_db = diesel::insert_into(logs)
            .values(&new_log)
            .get_result::<LogDb>(db_connection)?;

        Ok(log_db)
    }
}

/// Helper function to insert a task into the database associated with a specific daily log
pub fn insert_db_task(
    db_connection: &mut SqliteConnection,
    target_log_id: i32,
    task: &Task,
    raw_line_str: &str,
    is_completed_flag: bool,
) -> Result<TaskDb> {
    use crate::schema::tasks::dsl::*;
    let new_db_task = NewTaskDb {
        log_id: target_log_id,
        priority: task.priority.map(|c| c.to_string()),
        completion_date: task.completion_date.map(|d| d.to_rfc3339()),
        creation_date: task.creation_date.map(|d| d.to_rfc3339()),
        project_tag: task.project_tags_json(),
        context_tag: task.context_tags_json(),
        key_value_tags: serde_json::to_string(&task.key_value_tags)?,
        raw_line: raw_line_str,
        is_completed: is_completed_flag,
    };
    Ok(diesel::insert_into(tasks)
        .values(&new_db_task)
        .get_result(db_connection)?)
}

/// Applies all cache and task synchronization changes in a single SQLite transaction
pub fn apply_sync_updates(
    db_connection: &mut SqliteConnection,
    target_log_id: i32,
    cache_deletes: &[String],
    cache_inserts: &[String],
    resolved_actions: Vec<TaskAction>,
) -> Result<()> {
    db_connection.transaction::<_, anyhow::Error, _>(|conn| {
        use crate::schema::todo_cache::dsl::*;
        if !cache_deletes.is_empty() {
            diesel::delete(todo_cache.filter(raw_line.eq_any(cache_deletes)))
                .execute(conn)?;
        }
        if !cache_inserts.is_empty() {
            let inserts: Vec<TodoCacheDb> = cache_inserts
                .iter()
                .map(|line| TodoCacheDb { raw_line: line.clone() })
                .collect();
            diesel::insert_into(todo_cache)
                .values(&inserts)
                .execute(conn)?;
        }

        for action in resolved_actions {
            match action {
                TaskAction::Added { raw_line: added_raw_line, task } => {
                    let is_completed_flag = added_raw_line.starts_with("x ");
                    insert_db_task(conn, target_log_id, &task, &added_raw_line, is_completed_flag)?;
                    println!("{COLOR_WARN}+ New task processed:{COLOR_RESET} {}", added_raw_line);
                }
                TaskAction::Completed { old_raw, new_raw, new_task } => {
                    use crate::schema::tasks::dsl::*;
                    diesel::update(tasks.filter(raw_line.eq(&old_raw).and(is_completed.eq(false))))
                        .set((
                            log_id.eq(target_log_id),
                            completion_date.eq(new_task.completion_date.map(|d| d.to_rfc3339())),
                            creation_date.eq(new_task.creation_date.map(|d| d.to_rfc3339())),
                            project_tag.eq(new_task.project_tags_json()),
                            context_tag.eq(new_task.context_tags_json()),
                            key_value_tags.eq(serde_json::to_string(&new_task.key_value_tags)?),
                            raw_line.eq(&new_raw),
                            is_completed.eq(true),
                        ))
                        .execute(conn)?;
                    println!("{COLOR_SUCCESS}✓ Task completed:{COLOR_RESET} {}", new_raw);
                }
                TaskAction::Reopened { old_raw, new_raw, new_task } => {
                    use crate::schema::tasks::dsl::*;
                    diesel::update(tasks.filter(raw_line.eq(&old_raw).and(is_completed.eq(true))))
                        .set((
                            log_id.eq(target_log_id),
                            priority.eq(new_task.priority.map(|c| c.to_string())),
                            completion_date.eq(None::<String>),
                            creation_date.eq(new_task.creation_date.map(|d| d.to_rfc3339())),
                            project_tag.eq(new_task.project_tags_json()),
                            context_tag.eq(new_task.context_tags_json()),
                            key_value_tags.eq(serde_json::to_string(&new_task.key_value_tags)?),
                            raw_line.eq(&new_raw),
                            is_completed.eq(false),
                        ))
                        .execute(conn)?;
                    println!("{COLOR_WARN}↺ Task reopened:{COLOR_RESET} {}", new_raw);
                }
                TaskAction::Modified { old_raw, new_raw, new_task } => {
                    use crate::schema::tasks::dsl::*;
                    let is_completed_flag = new_raw.starts_with("x ");
                    let old_completed = old_raw.starts_with("x ");
                    let preserve_priority = old_completed && is_completed_flag && new_task.priority.is_none();

                    let query = tasks.filter(raw_line.eq(&old_raw).and(is_completed.eq(old_completed)));

                    if preserve_priority {
                        diesel::update(query)
                            .set((
                                completion_date.eq(new_task.completion_date.map(|d| d.to_rfc3339())),
                                creation_date.eq(new_task.creation_date.map(|d| d.to_rfc3339())),
                                project_tag.eq(new_task.project_tags_json()),
                                context_tag.eq(new_task.context_tags_json()),
                                key_value_tags.eq(serde_json::to_string(&new_task.key_value_tags)?),
                                raw_line.eq(&new_raw),
                                is_completed.eq(is_completed_flag),
                            ))
                            .execute(conn)?;
                    } else {
                        diesel::update(query)
                            .set((
                                priority.eq(new_task.priority.map(|c| c.to_string())),
                                completion_date.eq(new_task.completion_date.map(|d| d.to_rfc3339())),
                                creation_date.eq(new_task.creation_date.map(|d| d.to_rfc3339())),
                                project_tag.eq(new_task.project_tags_json()),
                                context_tag.eq(new_task.context_tags_json()),
                                key_value_tags.eq(serde_json::to_string(&new_task.key_value_tags)?),
                                raw_line.eq(&new_raw),
                                is_completed.eq(is_completed_flag),
                            ))
                            .execute(conn)?;
                    }
                    println!("{COLOR_INFO}~ Log updated.{COLOR_RESET}");
                }
            }
        }
        Ok(())
    })
}

/// Updates the energy level of a daily log
pub fn update_log_energy(
    db_connection: &mut SqliteConnection,
    log_id: i32,
    energy_state: u8,
) -> Result<()> {
    use crate::schema::logs::dsl::*;
    diesel::update(logs.filter(id.eq(log_id)))
        .set(energy.eq(energy_state as i32))
        .execute(db_connection)?;
    Ok(())
}

/// Updates the MVO items of a daily log
pub fn update_log_mvos(
    db_connection: &mut SqliteConnection,
    log_id: i32,
    mvo_items: &[String],
) -> Result<()> {
    use crate::schema::logs::dsl::*;
    diesel::update(logs.filter(id.eq(log_id)))
        .set(mvos.eq(serde_json::to_string(mvo_items)?))
        .execute(db_connection)?;
    Ok(())
}

/// Updates the reflection items (worked, failed, output) of a daily log
pub fn update_log_reflections(
    db_connection: &mut SqliteConnection,
    log_id: i32,
    worked_items: &[String],
    failed_items: &[String],
    output_items: &[String],
) -> Result<()> {
    use crate::schema::logs::dsl::*;
    diesel::update(logs.filter(id.eq(log_id)))
        .set((
            worked.eq(serde_json::to_string(worked_items)?),
            failed.eq(serde_json::to_string(failed_items)?),
            output.eq(serde_json::to_string(output_items)?),
        ))
        .execute(db_connection)?;
    Ok(())
}

/// Computes completed task count for today's log
pub fn get_today_completed_tasks_count(
    db_connection: &mut SqliteConnection,
    target_log_id: i32,
) -> Result<i64> {
    use crate::schema::tasks::dsl::*;
    let count = tasks
        .filter(log_id.eq(target_log_id))
        .filter(is_completed.eq(true))
        .count()
        .get_result(db_connection)?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::parse_task;

    fn setup_test_db() -> (SqliteConnection, LogDb) {
        let mut conn = SqliteConnection::establish(":memory:").expect("Failed to connect to in-memory DB");
        conn.run_pending_migrations(MIGRATIONS).expect("Failed to run migrations");
        let log = get_or_create_log(&mut conn, "2026-09-05").unwrap();
        (conn, log)
    }

    #[test]
    fn test_completed_task_preserves_priority() {
        use crate::schema::tasks::dsl::*;

        let (mut conn, log) = setup_test_db();
        let initial_raw = "(A) Important meeting +work";
        let task_a = parse_task(initial_raw).unwrap();

        // 1. Add new task with priority A
        apply_sync_updates(
            &mut conn,
            log.id,
            &[],
            &[initial_raw.to_string()],
            vec![TaskAction::Added {
                raw_line: initial_raw.to_string(),
                task: task_a,
            }],
        ).unwrap();

        let db_task = tasks.filter(raw_line.eq(initial_raw)).first::<TaskDb>(&mut conn).unwrap();
        assert_eq!(db_task.priority, Some("A".to_string()));
        assert!(!db_task.is_completed);

        // 2. Complete task in todo.txt: standard completion line removes (A)
        let completed_raw = "x 2026-09-05 Important meeting +work";
        let task_completed = parse_task(completed_raw).unwrap();
        assert_eq!(task_completed.priority, None);

        apply_sync_updates(
            &mut conn,
            log.id,
            &[initial_raw.to_string()],
            &[completed_raw.to_string()],
            vec![TaskAction::Completed {
                old_raw: initial_raw.to_string(),
                new_raw: completed_raw.to_string(),
                new_task: task_completed,
            }],
        ).unwrap();

        let updated_task = tasks.filter(raw_line.eq(completed_raw)).first::<TaskDb>(&mut conn).unwrap();
        assert!(updated_task.is_completed);
        assert_eq!(
            updated_task.priority,
            Some("A".to_string()),
            "Priority should NOT be overwritten with NULL on completion"
        );

        // 3. Modify completed task typo
        let modified_raw = "x 2026-09-05 Important meeting with team +work";
        let task_modified = parse_task(modified_raw).unwrap();
        assert_eq!(task_modified.priority, None);

        apply_sync_updates(
            &mut conn,
            log.id,
            &[completed_raw.to_string()],
            &[modified_raw.to_string()],
            vec![TaskAction::Modified {
                old_raw: completed_raw.to_string(),
                new_raw: modified_raw.to_string(),
                new_task: task_modified,
            }],
        ).unwrap();

        let modified_task = tasks.filter(raw_line.eq(modified_raw)).first::<TaskDb>(&mut conn).unwrap();
        assert_eq!(
            modified_task.priority,
            Some("A".to_string()),
            "Priority should NOT be overwritten with NULL on completed task typo modification"
        );

        // 4. Reopen task without priority -> priority becomes None
        let reopened_raw = "Important meeting with team +work";
        let task_reopened = parse_task(reopened_raw).unwrap();
        assert_eq!(task_reopened.priority, None);

        apply_sync_updates(
            &mut conn,
            log.id,
            &[modified_raw.to_string()],
            &[reopened_raw.to_string()],
            vec![TaskAction::Reopened {
                old_raw: modified_raw.to_string(),
                new_raw: reopened_raw.to_string(),
                new_task: task_reopened,
            }],
        ).unwrap();

        let reopened_task = tasks.filter(raw_line.eq(reopened_raw)).first::<TaskDb>(&mut conn).unwrap();
        assert!(!reopened_task.is_completed);
        assert_eq!(
            reopened_task.priority,
            None,
            "Priority becomes None when reopened without priority"
        );
    }
}
