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
use chrono::Local;
use clap::Parser;

use crate::cli::{CliArgs, CliCommands, COLOR_INFO, COLOR_RESET, COLOR_SUCCESS, COLOR_WARN};
use crate::config::load_config;
use crate::export::{export_database_to_json, ExportFilter};
use crate::file_utils::{count_remaining_tasks, delete_completed_tasks, rewrite_todo_file};
use crate::sync::{cache_sync, SyncState};
use crate::ui::{launch_log, prompt_energy_state, prompt_mvo_items};

fn main() -> Result<()> {
    let cli_args = CliArgs::parse();

    if let Some(CliCommands::Export { days, start, end, output }) = cli_args.command {
        let app_config = load_config()?;
        let mut db_connection = db::init_db(&app_config.logbook_path)?;

        let today = Local::now().date_naive();

        let (filter, default_filename) = match (days, start, end) {
            (Some(d), _, _) => (
                ExportFilter::Days(d),
                format!("logfine_export_{}_days.json", d),
            ),
            (None, Some(s), Some(e)) => {
                if s > e {
                    anyhow::bail!("Start date ({}) cannot be after end date ({})", s, e);
                }
                (
                    ExportFilter::Range {
                        start: Some(s),
                        end: e,
                    },
                    format!("logfine_export_{}_to_{}.json", s, e),
                )
            }
            (None, Some(s), None) => {
                if s > today {
                    anyhow::bail!(
                        "Start date ({}) cannot be after today ({})",
                        s,
                        today
                    );
                }
                (
                    ExportFilter::Range {
                        start: Some(s),
                        end: today,
                    },
                    format!("logfine_export_{}_to_{}.json", s, today),
                )
            }
            (None, None, Some(e)) => (
                ExportFilter::Range {
                    start: None,
                    end: e,
                },
                format!("logfine_export_until_{}.json", e),
            ),
            (None, None, None) => {
                let default_days = 7;
                (
                    ExportFilter::Days(default_days),
                    format!("logfine_export_{}_days.json", default_days),
                )
            }
        };

        let output_path = output.unwrap_or_else(|| PathBuf::from(default_filename));

        let count = export_database_to_json(&mut db_connection, filter, &output_path)?;
        if count == 0 {
            println!("{COLOR_WARN}No logs found for the specified date range. No file was created.{COLOR_RESET}");
        } else {
            println!("{COLOR_INFO}Exported {} log(s) to:{COLOR_RESET} {:?}", count, output_path);
        }
        return Ok(());
    }

    let mut skip_typos = false;
    let mut sync_only = false;

    if let Some(CliCommands::Sync { skip_typos: skip }) = cli_args.command {
        sync_only = true;
        skip_typos = skip;
    }

    let formatted_date = Local::now().format("%Y-%m-%d").to_string();
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
    } = cache_sync(&app_config, &mut db_connection, skip_typos)?;

    // Rewrite modified lines if auto-completion date or formatting was updated
    rewrite_todo_file(&app_config.todo_path, &file_rewrites)?;

    // Apply all updates in a single transaction
    db::apply_sync_updates(
        &mut db_connection,
        log.id,
        &cache_deletes,
        &cache_inserts,
        sync_actions,
    )?;

    println!("─────────────────────────────────");

    if app_config.delete_tasks {
        delete_completed_tasks(&app_config.todo_path)?;
    }

    // Count remaining tasks from the file (after archiving)
    let remaining_count = count_remaining_tasks(&app_config.todo_path)?;

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
    }

    // Query DB for total tasks completed today
    let completed_count = db::get_today_completed_tasks_count(&mut db_connection, log.id)?;

    println!(
        "{COLOR_SUCCESS}>{COLOR_RESET} Today's completed tasks {COLOR_SUCCESS}{}{COLOR_RESET}, remaining tasks {COLOR_WARN}{}{COLOR_RESET}",
        completed_count, remaining_count
    );

    Ok(())
}
