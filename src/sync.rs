use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader};
use anyhow::Result;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use strsim::jaro_winkler;

use crate::config::Config;
use crate::task::{parse_task, Task};

/// Actions representing task status changes after a sync
#[derive(Debug)]
pub enum TaskAction {
    Added {
        raw_line: String,
        task: Task,
    },
    Completed {
        old_raw: String,
        new_raw: String,
        new_task: Task,
    },
    Reopened {
        old_raw: String,
        new_raw: String,
        new_task: Task,
    },
    Modified {
        old_raw: String,
        new_raw: String,
        new_task: Task,
    },
}

pub struct SyncState {
    pub actions: Vec<TaskAction>,
    pub cache_inserts: Vec<String>,
    pub cache_deletes: Vec<String>,
    pub file_rewrites: HashMap<String, String>,
}

/// Synchronizes the todo.txt file with the state stored in the SQLite database cache
pub fn cache_sync(
    app_config: &Config,
    db_connection: &mut SqliteConnection,
) -> Result<SyncState> {
    use crate::schema::todo_cache::dsl::*;
    let todo_path = &app_config.todo_path;

    // 1. Load cache lines from SQLite database
    let cache_lines: HashSet<String> = todo_cache
        .select(raw_line)
        .load::<String>(db_connection)?
        .into_iter()
        .collect();

    // 2. Read todo.txt lines
    let todo_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(todo_path)?;
    let reader = BufReader::new(todo_file);
    let todo_lines: Vec<String> = reader.lines()
        .collect::<Result<Vec<String>, _>>()?
        .into_iter()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let todo_set: HashSet<String> = todo_lines.iter().cloned().collect();

    // 3. Calculate differences mathematically
    // Tasks that were in the cache but are no longer in the text file
    let missing_from_todo: Vec<String> = cache_lines.difference(&todo_set).cloned().collect();
    // Tasks that are in the text file but not in the cache
    let new_in_todo: Vec<String> = todo_set.difference(&cache_lines).cloned().collect();

    let mut sync_actions = Vec::new();
    let mut lines_to_cache = Vec::new();
    let mut file_rewrites = HashMap::new();
    let mut matching_removed = missing_from_todo.clone();

    // 4. Analyze new lines (appearances)
    for mut new_line in new_in_todo {
        let Some(mut task) = parse_task(&new_line) else {
            continue;
        };
        // Track line to be inserted into the cache database
        lines_to_cache.push(new_line.clone());

        // 5. Fuzzy matching against missing tasks
        let mut best_match = None;
        let mut highest_score = 0.0;
        let mut best_match_idx = None;

        for (idx, missing_line) in matching_removed.iter().enumerate() {
            let score = if let Some(missing_task) = parse_task(missing_line) {
                jaro_winkler(&task.description, &missing_task.description)
            } else {
                jaro_winkler(&new_line, missing_line)
            };
            if score > 0.79 && score > highest_score {
                highest_score = score;
                best_match = Some(missing_line.clone());
                best_match_idx = Some(idx);
            }
        }

        if let Some(old_line) = best_match {
            // Remove matched line to prevent multiple matches
            if let Some(idx) = best_match_idx {
                matching_removed.remove(idx);
            }

            let old_completed = old_line.starts_with("x ");
            let new_completed = new_line.starts_with("x ");

            if old_completed && !new_completed {
                sync_actions.push(TaskAction::Reopened {
                    old_raw: old_line,
                    new_raw: new_line.clone(),
                    new_task: task,
                });
            } else if !old_completed && new_completed {
                let Some(old_task) = parse_task(&old_line) else {
                    continue;
                };
                let needs_auto_date = if !app_config.automatic_completion_date {
                    false
                } else if old_task.creation_date.is_some() {
                    task.completion_date == old_task.creation_date && task.creation_date.is_none()
                } else {
                    task.completion_date.is_none()
                };

                if needs_auto_date {
                    let date_str = chrono::Local::now().format("%Y-%m-%d").to_string();
                    let mut modified_new_raw = new_line.clone();
                    if let Some(p) = task.priority {
                        let prefix = format!("x ({}) ", p);
                        let new_prefix = format!("x ({}) {} ", p, date_str);
                        modified_new_raw = modified_new_raw.replacen(&prefix, &new_prefix, 1);
                    } else {
                        let prefix = "x ";
                        let new_prefix = format!("x {} ", date_str);
                        modified_new_raw = modified_new_raw.replacen(prefix, &new_prefix, 1);
                    }
                    
                    file_rewrites.insert(new_line.clone(), modified_new_raw.clone());
                    new_line = modified_new_raw;
                    task = parse_task(&new_line).expect("Failed to parse newly modified task line");
                    if let Some(last) = lines_to_cache.last_mut() {
                        *last = new_line.clone();
                    }
                }

                sync_actions.push(TaskAction::Completed {
                    old_raw: old_line,
                    new_raw: new_line.clone(),
                    new_task: task,
                });
            } else {
                sync_actions.push(TaskAction::Modified {
                    old_raw: old_line,
                    new_raw: new_line.clone(),
                    new_task: task,
                });
            }
        } else {
            sync_actions.push(TaskAction::Added {
                raw_line: new_line.clone(),
                task,
            });
        }
    }

    Ok(SyncState {
        actions: sync_actions,
        cache_inserts: lines_to_cache,
        cache_deletes: missing_from_todo,
        file_rewrites,
    })
}
