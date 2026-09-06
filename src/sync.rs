use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader};
use anyhow::Result;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use inquire::Confirm;
use strsim::jaro_winkler;

use crate::cli::{COLOR_INFO, COLOR_RESET, COLOR_SUCCESS, COLOR_WARN};
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
    skip_typos: bool,
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

    // 3. Calculate differences deterministically
    // Tasks that were in the cache but are no longer in the text file
    let missing_from_todo: Vec<String> = cache_lines.difference(&todo_set).cloned().collect();
    // Tasks that are in the text file but not in the cache, preserving file line order
    let mut seen_new = HashSet::new();
    let new_in_todo: Vec<String> = todo_lines
        .into_iter()
        .filter(|line| !cache_lines.contains(line) && seen_new.insert(line.clone()))
        .collect();

    let parsed_new: Vec<Option<Task>> = new_in_todo.iter().map(|l| parse_task(l)).collect();
    let parsed_missing: Vec<Option<Task>> = missing_from_todo.iter().map(|l| parse_task(l)).collect();

    // 4. Build candidate match pairs and sort by similarity score descending
    struct CandidateMatch {
        new_idx: usize,
        missing_idx: usize,
        score: f64,
    }

    let mut candidates = Vec::new();
    for (new_idx, new_line) in new_in_todo.iter().enumerate() {
        let Some(task) = &parsed_new[new_idx] else {
            continue;
        };
        for (missing_idx, missing_line) in missing_from_todo.iter().enumerate() {
            let score = if let Some(missing_task) = &parsed_missing[missing_idx] {
                jaro_winkler(&task.description, &missing_task.description)
            } else {
                jaro_winkler(new_line, missing_line)
            };
            if score > 0.79 {
                candidates.push(CandidateMatch {
                    new_idx,
                    missing_idx,
                    score,
                });
            }
        }
    }

    // Sort candidates descending by score so highest-similarity matches take precedence
    candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    let mut sync_actions = Vec::new();
    let mut file_rewrites = HashMap::new();
    let mut final_new_lines = new_in_todo.clone();
    let mut matched_new: HashSet<usize> = HashSet::new();
    let mut matched_missing: HashSet<usize> = HashSet::new();

    // 5. Process candidate matches in order of similarity
    for candidate in candidates {
        if matched_new.contains(&candidate.new_idx) || matched_missing.contains(&candidate.missing_idx) {
            continue;
        }

        let new_line = &new_in_todo[candidate.new_idx];
        let old_line = &missing_from_todo[candidate.missing_idx];
        let mut task = parsed_new[candidate.new_idx].as_ref().unwrap().clone();
        let mut current_new_line = new_line.clone();

        let old_completed = old_line.starts_with("x ");
        let new_completed = current_new_line.starts_with("x ");

        if old_completed && !new_completed {
            matched_new.insert(candidate.new_idx);
            matched_missing.insert(candidate.missing_idx);
            sync_actions.push(TaskAction::Reopened {
                old_raw: old_line.clone(),
                new_raw: current_new_line,
                new_task: task,
            });
        } else if !old_completed && new_completed {
            matched_new.insert(candidate.new_idx);
            matched_missing.insert(candidate.missing_idx);

            let old_task = parsed_missing[candidate.missing_idx].as_ref();
            let needs_auto_date = if !app_config.automatic_completion_date {
                false
            } else if let Some(ot) = old_task {
                if ot.creation_date.is_some() {
                    task.completion_date == ot.creation_date && task.creation_date.is_none()
                } else {
                    task.completion_date.is_none()
                }
            } else {
                task.completion_date.is_none()
            };

            if needs_auto_date {
                let date_str = chrono::Local::now().format("%Y-%m-%d").to_string();
                let mut modified_new_raw = current_new_line.clone();
                if let Some(p) = task.priority {
                    let prefix = format!("x ({}) ", p);
                    let new_prefix = format!("x ({}) {} ", p, date_str);
                    modified_new_raw = modified_new_raw.replacen(&prefix, &new_prefix, 1);
                } else {
                    let prefix = "x ";
                    let new_prefix = format!("x {} ", date_str);
                    modified_new_raw = modified_new_raw.replacen(prefix, &new_prefix, 1);
                }

                file_rewrites.insert(current_new_line.clone(), modified_new_raw.clone());
                current_new_line = modified_new_raw;
                task = parse_task(&current_new_line).expect("Failed to parse newly modified task line");
                final_new_lines[candidate.new_idx] = current_new_line.clone();
            }

            sync_actions.push(TaskAction::Completed {
                old_raw: old_line.clone(),
                new_raw: current_new_line,
                new_task: task,
            });
        } else {
            // Potential typo/modification - prompt user
            let is_typo = if skip_typos {
                println!("{COLOR_INFO}Auto-accepted typo for task:{COLOR_RESET} {}", current_new_line);
                true
            } else {
                println!("{COLOR_INFO}A possible modification/typo was detected:{COLOR_RESET}");
                println!("  {COLOR_WARN}Old:{COLOR_RESET} {}", old_line);
                println!("  {COLOR_SUCCESS}New:{COLOR_RESET} {}", current_new_line);
                Confirm::new("Was this a typo correction?")
                    .with_default(true)
                    .prompt()?
            };

            if is_typo {
                matched_new.insert(candidate.new_idx);
                matched_missing.insert(candidate.missing_idx);
                sync_actions.push(TaskAction::Modified {
                    old_raw: old_line.clone(),
                    new_raw: current_new_line,
                    new_task: task,
                });
            } else {
                println!("{COLOR_INFO}+ Treated as a new task.{COLOR_RESET}");
                matched_new.insert(candidate.new_idx);
                // missing_idx is NOT marked in matched_missing so other candidates can still match
                sync_actions.push(TaskAction::Added {
                    raw_line: current_new_line,
                    task,
                });
            }
        }
    }

    // 6. Unmatched new lines become Added actions
    for (new_idx, line_str) in final_new_lines.iter().enumerate() {
        if !matched_new.contains(&new_idx) {
            if let Some(task) = parsed_new[new_idx].clone() {
                sync_actions.push(TaskAction::Added {
                    raw_line: line_str.clone(),
                    task,
                });
            }
        }
    }

    let lines_to_cache: Vec<String> = final_new_lines
        .into_iter()
        .enumerate()
        .filter(|(idx, _)| parsed_new[*idx].is_some())
        .map(|(_, line)| line)
        .collect();

    Ok(SyncState {
        actions: sync_actions,
        cache_inserts: lines_to_cache,
        cache_deletes: missing_from_todo,
        file_rewrites,
    })
}
