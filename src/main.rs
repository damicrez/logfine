pub mod cli;
pub mod config;
pub mod db;
pub mod export;
pub mod file_utils;
pub mod models;
pub mod schema;
pub mod sync;
pub mod task;
pub mod ui;

use std::fs;
use std::path::PathBuf;
use anyhow::Result;
use chrono::{DateTime, Local};
use clap::Parser;
use inquire::Confirm;

use crate::cli::{CliArgs, CliCommands, COLOR_INFO, COLOR_RESET, COLOR_SUCCESS, COLOR_WARN};
use crate::config::load_config;
use crate::export::export_database_to_json;
use crate::file_utils::{delete_completed_tasks, rewrite_todo_file};
use crate::sync::{cache_sync, SyncState, TaskAction};
use crate::ui::{launch_log, prompt_energy_state, prompt_mvo_items};

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
        println!("{COLOR_INFO}Exported the last {} days to:{COLOR_RESET} {:?}", resolved_days, output_path);
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
    fs::create_dir_all(&app_config.logbook_path)?;
    if let Some(parent) = app_config.todo_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut db_connection = db::init_db(&app_config.logbook_path)?;

    // Fetch or create log for today
    let log = db::get_or_create_log(&mut db_connection, &formatted_date)?;

    // Synchronize todo tasks
    let SyncState {
        actions: sync_actions,
        cache_inserts,
        cache_deletes,
        file_rewrites,
    } = cache_sync(&app_config, &mut db_connection)?;

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
                    println!("{COLOR_INFO}Auto-accepted typo for task:{COLOR_RESET} {}", new_raw);
                    true
                } else {
                    println!("{COLOR_INFO}A possible modification/typo was detected:{COLOR_RESET}");
                    println!("  {COLOR_WARN}Old:{COLOR_RESET} {}", old_raw);
                    println!("  {COLOR_SUCCESS}New:{COLOR_RESET} {}", new_raw);
                    Confirm::new("Was this a typo correction?")
                        .with_default(true)
                        .prompt()?
                };
                resolved_actions.push((TaskAction::Modified { old_raw, new_raw, new_task }, is_typo));
            }
        }
    }

    // Apply all updates in a single transaction
    db::apply_sync_updates(
        &mut db_connection,
        log.id,
        &cache_deletes,
        &cache_inserts,
        resolved_actions,
    )?;

    // Rewrite modified lines if auto-completion date or formatting was updated
    rewrite_todo_file(&app_config.todo_path, &file_rewrites)?;

    println!("─────────────────────────────────");

    if app_config.delete_tasks {
        delete_completed_tasks(&app_config.todo_path)?;
    }

    if !sync_only {
        let existing_energy = log.energy as u8;
        let energy_state = prompt_energy_state(existing_energy)?;
        db::update_log_energy(&mut db_connection, log.id, energy_state)?;

        let existing_mvos: Vec<String> = serde_json::from_str(&log.mvos).unwrap_or_default();
        let mvo_items = prompt_mvo_items(&app_config.mvos, &existing_mvos)?;
        db::update_log_mvos(&mut db_connection, log.id, &mvo_items)?;

        let existing_worked: Vec<String> = serde_json::from_str(&log.worked).unwrap_or_default();
        let existing_failed: Vec<String> = serde_json::from_str(&log.failed).unwrap_or_default();
        let existing_output: Vec<String> = serde_json::from_str(&log.output).unwrap_or_default();
        let (worked_items, failed_items, output_items) =
            launch_log(&existing_worked, &existing_failed, &existing_output)?;
        db::update_log_reflections(
            &mut db_connection,
            log.id,
            &worked_items,
            &failed_items,
            &output_items,
        )?;

        let (completed_today_count, remaining_count) =
            db::get_today_task_counts(&mut db_connection, log.id, &formatted_date)?;

        println!(
            "{COLOR_SUCCESS}>{COLOR_RESET} Today's completed tasks {COLOR_SUCCESS}{}{COLOR_RESET}, remaining tasks {COLOR_WARN}{}{COLOR_RESET}",
            completed_today_count, remaining_count
        );
    }

    Ok(())
}
