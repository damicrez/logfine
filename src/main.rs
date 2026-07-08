pub mod schema;
pub mod models;
pub mod db;

use std::fs::{self, File, OpenOptions};
use std::io::BufReader;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc, Local, NaiveDate};
use edit::{Builder, edit_with_builder};
use inquire::{CustomType, MultiSelect, Confirm};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use strsim::jaro_winkler;
use diesel::prelude::*;
use clap::{Parser, Subcommand};
use anyhow::Result;
use std::sync::LazyLock;
use regex::Regex;

use crate::models::{LogDb, NewLogDb, NewTaskDb, TodoCacheDb, TaskDb};

/// Command-line interface arguments parsed via Clap
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct CliArgs {
    #[command(subcommand)]
    command: Option<CliCommands>,
}

#[derive(Subcommand)]
enum CliCommands {
    /// Synchronize completed tasks with the database cache
    Sync {
        /// Skip interactive prompts for typos and automatically accept updates
        #[arg(long)]
        skip_typos: bool,
    },
    /// Export the last N days of daily logs to a JSON file
    Export {
        /// Number of days to export
        days: Option<usize>,
        /// Optional path to the output JSON file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

/// Represents a parsed todo.txt task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub priority: Option<char>,
    pub completion_date: Option<DateTime<Utc>>,
    pub creation_date: Option<DateTime<Utc>>,
    pub project_tag: Option<String>,
    pub context_tag: Option<String>,
    pub key_value_tags: HashMap<String, String>,
}

/// Configuration settings
#[derive(Debug, Serialize, Deserialize)]
struct Config {
    logbook_path: PathBuf,
    todo_path: PathBuf,
    #[serde(default)]
    mvos: Vec<String>,
    #[serde(default)]
    delete_tasks: bool,
}

impl Default for Config {
    fn default() -> Self {
        let base_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("logfine");
        let todo_path = base_dir.clone().join("todo.txt");

        Config {
            logbook_path: base_dir,
            todo_path,
            mvos: Vec::new(),
            delete_tasks: false,
        }
    }
}

/// Loads configuration from the local user directory or initializes default config
fn load_config() -> Result<Config> {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("logfine");
    path.push("logfine.toml");

    match fs::read_to_string(&path) {
        Ok(content) => {
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        }
        Err(err) => {
            if err.kind() == io::ErrorKind::NotFound {
                println!("Config file not found. Writing default config at: {:?}", path);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }

                let mut default_config = Config::default();
                default_config.mvos = vec![
                    "Read one chapter of a book".to_string(),
                    "Do some exercise".to_string(),
                    "One push commit to GitHub".to_string(),
                ];
                
                let default_content = toml::to_string(&default_config)?;
                fs::write(&path, default_content)?;
                
                println!("Check your database at {:?} using sqlite or execute \"logfine export\" to get your data in JSON", default_config.logbook_path);
                Ok(default_config)
            } else {
                Err(err.into())
            }
        }
    }
}

/// Prompt the user to enter their energy state (1-3)
fn prompt_energy_state(default_val: u8) -> Result<u8> {
    let validator = |val: &u8| -> Result<
        inquire::validator::Validation,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        if (1..=3).contains(val) {
            Ok(inquire::validator::Validation::Valid)
        } else {
            Err("Value must be between 1 and 3".into())
        }
    };
    let energy = CustomType::<u8>::new("Today's energy state (1-3, Low-High)")
        .with_validator(validator)
        .with_default(default_val)
        .prompt()?;
    Ok(energy)
}

/// Prompt the user to select completed Minimum Viable Output (MVO) items
fn prompt_mvo_items(items: &[String], existing_mvos: &[String]) -> Result<Vec<String>> {
    let default_indices: Vec<usize> = items.iter().enumerate().filter(|(_,item)| existing_mvos.contains(item)).map(|(idx, _)| idx).collect();
    let checked = MultiSelect::new("Today's minimum viable output", items.to_vec())
        .with_default(&default_indices)
        .prompt()?;
    Ok(checked)
}


fn parse_section(content: &str, section: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut in_section = false;
    let section_marker = format!("[{}]", section);

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == section_marker {
            in_section = true;
        } else if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_section = false;
        } else if in_section && !trimmed.is_empty() {
            result.push(trimmed.strip_prefix("- ").unwrap_or(trimmed).to_string());
        }
    }
    result
}

// Launch default text editor for inputting log details (what worked, failed, output)

fn launch_log(
    existing_worked: &[String],
    existing_failed: &[String],
    existing_output: &[String],
) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let mut template = String::new();

    template.push_str("[What worked]\n");
    if existing_worked.is_empty() {
        template.push('\n');
    } else {
        for item in existing_worked {
            template.push_str(&format!("- {}\n", item));
        }
        template.push('\n');
    }

    template.push_str("[What failed]\n");
    if existing_failed.is_empty() {
        template.push('\n');
    } else {
        for item in existing_failed {
            template.push_str(&format!("- {}\n", item));
        }
        template.push('\n');
    }

    template.push_str("[Output]\n");
    if existing_output.is_empty() {
        template.push('\n');
    } else {
        for item in existing_output {
            template.push_str(&format!("- {}\n", item));
        }
        template.push('\n');
    }

    let mut builder = Builder::new();
    builder
        .prefix("log-")
        .suffix(".md");
    let content = edit_with_builder(&template, &builder)?;
    Ok((
            parse_section(&content, "What worked"),
            parse_section(&content, "What failed"),
            parse_section(&content, "Output")))
}

static TODO_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(x )?(?:\(([A-Z])\) )?([0-9]{4}-[0-9]{2}-[0-9]{2} )?([0-9]{4}-[0-9]{2}-[0-9]{2} )?(.*)$").unwrap()
});

fn parse_task(line: &str) -> Option<Task> {
    if line.trim().is_empty() {
        return None;
    }
    let captures = TODO_REGEX.captures(line)?;
    
    let is_completed = captures.get(1).is_some();
    let priority = captures.get(2).map(|m| m.as_str().chars().next().unwrap());
    
    let date1_str = captures.get(3).map(|m| m.as_str().trim());
    let date2_str = captures.get(4).map(|m| m.as_str().trim());
    
    let mut completion_date = None;
    let mut creation_date = None;

    if is_completed {
        if let Some(d1) = date1_str.and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()) {
            completion_date = Some(d1.and_hms_opt(0, 0, 0).unwrap().and_utc());
            if let Some(d2) = date2_str.and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()) {
                creation_date = Some(d2.and_hms_opt(0, 0, 0).unwrap().and_utc());
            }
        }
    } else {
        if let Some(d1) = date1_str.and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()) {
            creation_date = Some(d1.and_hms_opt(0, 0, 0).unwrap().and_utc());
        }
    }

    let description = captures.get(5).map(|m| m.as_str()).unwrap_or("");
    
    let mut project_tag = None;
    let mut context_tag = None;
    let mut key_value_tags = HashMap::new();

    for item in description.split_whitespace() {
        if item.starts_with('+') && item.len() > 1 && project_tag.is_none() {
            project_tag = Some(item[1..].to_string());
        } else if item.starts_with('@') && item.len() > 1 && context_tag.is_none() {
            context_tag = Some(item[1..].to_string());
        } else if let Some((key, value)) = item.split_once(':').filter(|(k, v)| !k.is_empty() && !v.is_empty()) {
            key_value_tags.insert(key.to_string(), value.to_string());
        }
    }

    Some(Task {
        priority,
        completion_date,
        creation_date,
        project_tag,
        context_tag,
        key_value_tags,
    })
}

/// Actions representing task status changes after a sync
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

/// Synchronizes the todo.txt file with the state stored in the SQLite database cache
pub fn cache_sync(
    todo_path: &Path,
    db_connection: &mut SqliteConnection,
) -> Result<Vec<TaskAction>> {
    use crate::schema::todo_cache::dsl::*;

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
    let mut matching_removed = missing_from_todo.clone();

    // 4. Analyze new lines (appearances)
    for new_line in new_in_todo {
        let Some(task) = parse_task(&new_line) else {
            continue;
        };

        // Track line to be inserted into the cache database
        lines_to_cache.push(new_line.clone());

        // 5. Fuzzy matching against missing tasks
        let mut best_match = None;
        let mut highest_score = 0.0;
        let mut best_match_idx = None;

        for (idx, missing_line) in matching_removed.iter().enumerate() {
            let score = jaro_winkler(&new_line, missing_line);
            if score > 0.85 && score > highest_score {
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

    // 6. Save the updated state to the database todo_cache table inside a transaction
    db_connection.transaction::<_, anyhow::Error, _>(|conn| {
        if !missing_from_todo.is_empty() {
            diesel::delete(todo_cache.filter(raw_line.eq_any(&missing_from_todo)))
                .execute(conn)?;
        }

        if !lines_to_cache.is_empty() {
            let inserts: Vec<TodoCacheDb> = lines_to_cache
                .into_iter()
                .map(|line| TodoCacheDb { raw_line: line })
                .collect();
            diesel::insert_into(todo_cache)
                .values(&inserts)
                .execute(conn)?;
        }
        Ok(())
    })?;

    Ok(sync_actions)
}

/// Helper function to load or create today's daily log entry in the database
fn get_or_create_log(db_connection: &mut SqliteConnection, date_str: &str) -> Result<LogDb> {
    use crate::schema::logs::dsl::*;
    
    let existing = logs.filter(log_date.eq(date_str))
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
fn insert_db_task(
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
        project_tag: task.project_tag.clone(),
        context_tag: task.context_tag.clone(),
        key_value_tags: serde_json::to_string(&task.key_value_tags)?,
        raw_line: raw_line_str,
        is_completed: is_completed_flag,
    };
    Ok(diesel::insert_into(tasks)
        .values(&new_db_task)
        .get_result(db_connection)?)
}

/// Permanently deletes all completed tasks (lines starting with "x ") from the todo file
fn delete_completed_tasks(todo_path: &Path) -> Result<()> {
    let file = File::open(todo_path)?;
    let reader = BufReader::new(file);
    let mut non_completed_lines = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if !line.starts_with("x ") {
            non_completed_lines.push(line);
        }
    }

    // Atomic write to avoid data loss
    let parent = todo_path.parent().unwrap_or_else(|| Path::new("."));
    let temp_path = parent.join(format!(
        ".{}.tmp",
        todo_path.file_name().and_then(|n| n.to_str()).unwrap_or("todo")
    ));

    let mut content = non_completed_lines.join("\n");
    if !content.is_empty() {
        content.push('\n');
    }

    fs::write(&temp_path, content)?;
    if let Err(e) = fs::rename(&temp_path, todo_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(e.into());
    }
    Ok(())
}

#[derive(Serialize)]
struct ExportedTask {
    priority: Option<String>,
    completion_date: Option<String>,
    creation_date: Option<String>,
    project_tag: Option<String>,
    context_tag: Option<String>,
    key_value_tags: serde_json::Value,
    raw_line: String,
    is_completed: bool,
}

#[derive(Serialize)]
struct ExportedLog {
    date: String,
    energy: i32,
    mvos: Vec<String>,
    worked: Vec<String>,
    failed: Vec<String>,
    output: Vec<String>,
    tasks: Vec<ExportedTask>,
}

fn export_database_to_json(
    db_connection: &mut SqliteConnection,
    days: usize,
    output_path: &Path,
) -> Result<()> {
    use crate::schema::logs::dsl::*;
    use crate::schema::tasks::dsl::{tasks, log_id};

    // 1. Fetch log entries
    let log_entries = logs
        .order(log_date.desc())
        .limit(days as i64)
        .load::<LogDb>(db_connection)?;

    // 2. Fetch all tasks for these logs
    let log_ids: Vec<i32> = log_entries.iter().map(|l| l.id).collect();
    let task_entries = tasks
        .filter(log_id.eq_any(&log_ids))
        .load::<TaskDb>(db_connection)?;

    // Group tasks by log_id
    let mut tasks_by_log_id: HashMap<i32, Vec<TaskDb>> = HashMap::new();
    for task in task_entries {
        tasks_by_log_id.entry(task.log_id).or_default().push(task);
    }

    // 3. Construct ExportedLog list
    let mut exported_logs = Vec::new();
    for log in log_entries {
        let log_tasks = tasks_by_log_id.remove(&log.id).unwrap_or_default();
        let exported_tasks: Vec<ExportedTask> = log_tasks
            .into_iter()
            .map(|t| {
                let kv_tags: serde_json::Value = serde_json::from_str(&t.key_value_tags)
                    .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
                ExportedTask {
                    priority: t.priority,
                    completion_date: t.completion_date,
                    creation_date: t.creation_date,
                    project_tag: t.project_tag,
                    context_tag: t.context_tag,
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

fn main() -> Result<()> {
    let cli_args = CliArgs::parse();

    if let Some(CliCommands::Export { days, output }) = cli_args.command {
        let app_config = load_config()?;
        let mut db_connection = db::init_db(&app_config.logbook_path)?;

        let resolved_days = days.unwrap_or(7);

        let output_path = output.unwrap_or_else(|| {
            PathBuf::from(format!("logfine_export_{}_days.json", resolved_days))
        });

        export_database_to_json(&mut db_connection, resolved_days, &output_path)?;
        println!("Exported the last {} days to {:?}", resolved_days, output_path);
        return Ok(());
    }
    
    let mut skip_typos = false;
    let mut sync_only = false;
    
    if let Some(CliCommands::Sync { skip_typos: skip }) = cli_args.command {
        sync_only = true;
        skip_typos = skip;
    }

    let current_time: DateTime<Local> = Local::now();
    let formatted_date = current_time.format("%Y-%m-%d").to_string();
    let app_config = load_config()?;

    // Establish DB connection & run migrations
    if let Some(parent) = app_config.logbook_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut db_connection = db::init_db(&app_config.logbook_path)?;

    // Fetch or create log for today
    let log = get_or_create_log(&mut db_connection, &formatted_date)?;

    // Synchronize todo tasks
    let sync_actions = cache_sync(&app_config.todo_path, &mut db_connection)?;

    // Collect user decisions for modified tasks first to avoid holding a transaction lock during prompts
    let mut resolved_actions = Vec::new();
    for action in sync_actions {
        match action {
            TaskAction::Added { raw_line, task } => {
                resolved_actions.push((TaskAction::Added { raw_line, task }, false));
            }
            TaskAction::Completed { old_raw, new_raw, new_task } => {
                resolved_actions.push((TaskAction::Completed { old_raw, new_raw, new_task }, false));
            }
            TaskAction::Reopened { old_raw, new_raw, new_task } => {
                resolved_actions.push((TaskAction::Reopened { old_raw, new_raw, new_task }, false));
            }
            TaskAction::Modified { old_raw, new_raw, new_task } => {
                let is_typo = if skip_typos {
                    println!("Auto-accepted typo for task: {}", new_raw);
                    true
                } else {
                    println!("A possible modification/typo was detected:");
                    println!("  Old: {}", old_raw);
                    println!("  New: {}", new_raw);
                    Confirm::new("Was this a typo correction?")
                        .with_default(true)
                        .prompt()?
                };
                resolved_actions.push((TaskAction::Modified { old_raw, new_raw, new_task }, is_typo));
            }
        }
    }

    // Apply all updates in a single transaction
    db_connection.transaction::<_, anyhow::Error, _>(|conn| {
        for (action, is_typo) in resolved_actions {
            match action {
                TaskAction::Added { raw_line: added_raw_line, task } => {
                    let is_completed_flag = added_raw_line.starts_with("x ");
                    insert_db_task(conn, log.id, &task, &added_raw_line, is_completed_flag)?;
                    if is_completed_flag {
                        println!("+ New completed task processed.");
                    } else {
                        println!("+ New uncompleted task processed.");
                    }
                }
                TaskAction::Completed { old_raw, new_raw, new_task } => {
                    use crate::schema::tasks::dsl::*;
                    diesel::update(tasks.filter(raw_line.eq(&old_raw).and(is_completed.eq(false))))
                        .set((
                            priority.eq(new_task.priority.map(|c| c.to_string())),
                            completion_date.eq(new_task.completion_date.map(|d| d.to_rfc3339())),
                            creation_date.eq(new_task.creation_date.map(|d| d.to_rfc3339())),
                            project_tag.eq(new_task.project_tag.clone()),
                            context_tag.eq(new_task.context_tag.clone()),
                            key_value_tags.eq(serde_json::to_string(&new_task.key_value_tags)?),
                            raw_line.eq(&new_raw),
                            is_completed.eq(true),
                        ))
                        .execute(conn)?;
                    println!("✓ Task completed: {}", new_raw);
                }
                TaskAction::Reopened { old_raw, new_raw, new_task } => {
                    use crate::schema::tasks::dsl::*;
                    diesel::update(tasks.filter(raw_line.eq(&old_raw).and(is_completed.eq(true))))
                        .set((
                            priority.eq(new_task.priority.map(|c| c.to_string())),
                            completion_date.eq(None::<String>),
                            creation_date.eq(new_task.creation_date.map(|d| d.to_rfc3339())),
                            project_tag.eq(new_task.project_tag.clone()),
                            context_tag.eq(new_task.context_tag.clone()),
                            key_value_tags.eq(serde_json::to_string(&new_task.key_value_tags)?),
                            raw_line.eq(&new_raw),
                            is_completed.eq(false),
                        ))
                        .execute(conn)?;
                    println!("↺ Task reopened: {}", new_raw);
                }
                TaskAction::Modified { old_raw, new_raw, new_task } => {
                    let is_completed_flag = new_raw.starts_with("x ");
                    if is_typo {
                        use crate::schema::tasks::dsl::*;
                        let old_completed = old_raw.starts_with("x ");
                        diesel::update(tasks.filter(raw_line.eq(&old_raw).and(is_completed.eq(old_completed))))
                            .set((
                                priority.eq(new_task.priority.map(|c| c.to_string())),
                                completion_date.eq(new_task.completion_date.map(|d| d.to_rfc3339())),
                                creation_date.eq(new_task.creation_date.map(|d| d.to_rfc3339())),
                                project_tag.eq(new_task.project_tag.clone()),
                                context_tag.eq(new_task.context_tag.clone()),
                                key_value_tags.eq(serde_json::to_string(&new_task.key_value_tags)?),
                                raw_line.eq(&new_raw),
                                is_completed.eq(is_completed_flag),
                            ))
                            .execute(conn)?;
                        println!("~ Log updated.");
                    } else {
                        insert_db_task(conn, log.id, &new_task, &new_raw, is_completed_flag)?;
                        println!("+ Treated as a new task.");
                    }
                }
            }
        }
        Ok(())
    })?;

    if app_config.delete_tasks {
        delete_completed_tasks(&app_config.todo_path)?;
        println!("Completed tasks removed from todo file.");
    }

    if !sync_only {
        let existing_energy = log.energy as u8;
        let energy_state = prompt_energy_state(existing_energy)?;
        use crate::schema::logs::dsl::*;
        diesel::update(logs.filter(id.eq(log.id)))
            .set(energy.eq(energy_state as i32))
            .execute(&mut db_connection)?;

        let existing_mvos: Vec<String> = serde_json::from_str(&log.mvos).unwrap_or_default();
        let mvo_items = prompt_mvo_items(&app_config.mvos, &existing_mvos)?;
        diesel::update(logs.filter(id.eq(log.id)))
            .set(mvos.eq(serde_json::to_string(&mvo_items)?))
            .execute(&mut db_connection)?;

        let existing_worked: Vec<String> = serde_json::from_str(&log.worked).unwrap_or_default();
        let existing_failed: Vec<String> = serde_json::from_str(&log.failed).unwrap_or_default();
        let existing_output: Vec<String> = serde_json::from_str(&log.output).unwrap_or_default();
        let (worked_items, failed_items, output_items) = launch_log(&existing_worked, &existing_failed, &existing_output)?;
        diesel::update(logs.filter(id.eq(log.id)))
            .set((
                worked.eq(serde_json::to_string(&worked_items)?),
                failed.eq(serde_json::to_string(&failed_items)?),
                output.eq(serde_json::to_string(&output_items)?),
            ))
            .execute(&mut db_connection)?;
    }

    println!("Finished.");
    Ok(())
}
