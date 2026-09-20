# logfine

A local-first CLI logger designed to generate structured JSON data for AI analysis and personal trend tracking.

## Features

- Task tracking via todo.txt integration & synchronization.
- Energy, MVOs (Minimum Viable Outputs) and custom logbook tracking.
- Vim-like keybinding-driven workflow.
- Structured querying & filtering via an embedded SQLite database.
- Local-first architecture ensuring offline and private usage.
- High performance & memory safety powered by Rust and Diesel ORM.

## Why?

The purpose of this software is to **quantify** daily habits, energy levels, tasks, and events.
It makes data easier to analyze, supporting decision-making and externalizing memory.

![Program Flow](./flowexample.gif)

Personally, I use this tool **to ask an AI agent** for patterns, busywork, misalignment between my effort and output, which habits correlate with my highest energy, etc. [Article: Agentic Maintained Personal Informatics (Spanish)](https://api.damiandlcp.com/api/assets/136712c5-7b8b-4efa-b5ee-aedadaa53668/AgentMaintainedPersonalInformaticsSystemForPeriodReports.pdf) 

![AI analysis](./AgenticUsage.gif)

That's why the data can be exported to JSON: It is **both human-readable and machine-readable**.

![Database showcase](./sqlexample.gif)

## Installation

Since *logfine* is built in Rust, you can easily compile it from source. This ensures the binary is perfectly optimized for your specific architecture and operating system.

### Prerequisites

You need to have the Rust toolchain installed on your system. If you don't have it yet, you can install it via [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Option 1: Install via Cargo (Recommended)

This is the cleanest way to install the CLI. It compiles the binary in release mode and automatically places it in your Cargo binary directory (usually ~/.cargo/bin/), which should be in your system's $PATH.

```bash
# Clone the repository
git clone https://github.com/damicrez/logfine.git
cd logfine

# Build and install the binary locally
cargo install --path .

# Once installed, you can run the tool from anywhere in your terminal:
logfine
```

### Option 2: Build the Binary Manually

If you just want to compile the executable file without installing it globally into your system, use the release build command:

```bash
# Clone the repository
git clone https://github.com/damicrez/logfine.git
cd logfine

# Compile in release mode (optimized)
cargo build --release
```

## Configuration

The configuration file is expected to be located at your default configuration directory (~/.config/logfine/logfine.toml on Unix-like systems.)

Here's an explanation of every configuration item:

**logbook_path:** Directory where a SQLite database will be located.

**todo_path:** Path to your todo.txt file.

**mvos:** List of strings defining the *Minimum Viable Outputs* for the day.

**delete_tasks:** Boolean (`true`/`false`). If set to `true`, completed tasks are removed from your todo.txt.

**automatic_completion_date:** Boolean (`true`/`false`) variable to append completion dates when a task is marked as completed, helping maintain todo.txt compliance.

**template_path:** Path to a Markdown file to be used as the logbook template. If no file is provided, the default "Worked, Failed & Output" template is used. Supports both bracketed (`[Section]`) and Markdown header (`# Section`) syntax.

### Configuration example

```toml
logbook_path = "/home/username/Documents/life/"
todo_path = "/home/username/Nextcloud/todo.txt"
mvos = ["Code commit", "Zettelkasten note", "Social exposure"]
delete_tasks = true
automatic_completion_date = false
template_path = "/home/username/.config/logfine/template.md"
```

### Template example

```markdown
[Worked with brackets]

# Failed with headers

```

## Subcommands

**export**: Export the last N days of daily logs to a JSON file.

```bash
# Export the last 7 days (default) to JSON
logfine export

# Export the last 30 days to a custom output file
logfine export 30 -o monthly_report.json

# Export days from 2026-09-01 to 2026-09-14
logfine export -s 2026-09-01 -e 2026-09-14
```

**sync**: Synchronize tasks with the database cache without prompts.

```bash
# Sync todo.txt with the database
logfine sync

# Automatically accept detected typos during sync
logfine sync --skip-typos
```

