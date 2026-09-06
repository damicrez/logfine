use std::path::PathBuf;
use clap::{Parser, Subcommand};
use anstyle::{AnsiColor, Color, Reset, Style};

use chrono::NaiveDate;

pub const COLOR_SUCCESS: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green)));
pub const COLOR_WARN: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Yellow)));
pub const COLOR_INFO: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Blue)));
pub const COLOR_RESET: Reset = Reset;

/// Built in Rust, logfine is a CLI tool to keep track of your days.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Option<CliCommands>,
}

#[derive(Subcommand, Debug)]
pub enum CliCommands {
    /// Synchronize tasks with the database cache without prompts
    Sync {
        /// Skip interactive prompts for typos and automatically accept updates
        #[arg(long)]
        skip_typos: bool,
    },
    /// Export logs to a JSON file
    Export {
        /// Number of days to export
        #[arg(conflicts_with_all = ["start", "end"])]
        days: Option<usize>,
        /// Filter logs starting from this date (YYYY-MM-DD)
        #[arg(short = 's', long)]
        start: Option<NaiveDate>,
        /// Filter logs up to this date (YYYY-MM-DD)
        #[arg(short = 'e', long)]
        end: Option<NaiveDate>,
        /// Optional path to the output JSON file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}
