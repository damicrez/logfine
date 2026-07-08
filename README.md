# logfine

A CLI logger designed to generate structured data (JSON) perfect for AI analysis and personal trend tracking.

## Features

- Keep a record of tasks in your todo.txt file.
- Lineal flow, keep up a daily track of your energy state, MVO's, and "What worked", "What failed" and "Output".
- Stores everything into a local sqlite database. And you can export your data into a JSON file.

## Usage examples

I use this tool to ask GPT for patterns, low leverage tasks, misalignment between my effort and output...
You can use the JSON file for charts and see progress.

![Program Flow](https://github.com/damicrez/logfine/blob/main/flowexample.gif)

## Installation

Since *logfine* is built in Rust, you can easily compile it from source. This ensures the binary is perfectly optimized for your specific architecture and operating system.

### Prerequisites

You need to have the Rust toolchain installed on your system. If you don't have it yet, you can install it via [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf [https://sh.rustup.rs](https://sh.rustup.rs) | sh
```

### Option 1: Install via Cargo (Recommended)

This is the cleanest way to install the CLI. It compiles the binary in release mode and automatically places it in your Cargo binary directory (usually ~/.cargo/bin/), which should be in your system's $PATH.

```bash
# Clone the repository
git clone [https://github.com/damicrez/logfine.git](https://github.com/damicrez/logfine.git)
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
git clone [https://github.com/damicrez/logfine.git](https://github.com/damicrez/logfine.git)
cd logfine

# Compile in release mode (optimized)
cargo build --release
```

## Configuration

It's kinda self-explainatory, but...

**logbook_path**: Stores a sqlite database with the tasks and log data, you can check it by yourself.

**todo_path**: It should contain your todo.txt file, so logfine can register your completed and non-completed tasks.

**mvos**: Is a list of strings with the *Minimum Viable Output* of your day, so you can keep track of it everyday.

**delete_tasks**: A true|false configuration variable to delete or no delete the completed tasks in your todo.txt file.

### Example

```toml
logbook_path = "/home/damicrez/Documents/life/logbook/"
todo_path = "/home/damicrez/Nextcloud/README.md"
mvos = ["Code commit", "Zettelkasten note", "Social interaction"]
delete_tasks = false
```
