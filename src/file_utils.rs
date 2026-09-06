use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;
use anyhow::Result;

/// Safely writes content to a target file atomically using a temporary file and rename
pub fn atomic_write(target_path: &Path, content: &str) -> Result<()> {
    let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
    let temp_name = format!(
        ".{}.tmp",
        target_path.file_name().and_then(|n| n.to_str()).unwrap_or("todo")
    );
    let temp_path = parent.join(temp_name);

    fs::write(&temp_path, content)?;
    if let Err(e) = fs::rename(&temp_path, target_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(e.into());
    }
    Ok(())
}

/// Permanently deletes all completed tasks (lines starting with "x ") from the todo file
pub fn delete_completed_tasks(todo_path: &Path) -> Result<()> {
    let file = File::open(todo_path)?;
    let reader = BufReader::new(file);
    let mut non_completed_lines = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if !line.starts_with("x ") {
            non_completed_lines.push(line);
        }
    }

    let mut content = non_completed_lines.join("\n");
    if !content.is_empty() {
        content.push('\n');
    }

    atomic_write(todo_path, &content)
}

/// Rewrites modified lines in the todo file atomically
pub fn rewrite_todo_file(
    todo_path: &Path,
    file_rewrites: &HashMap<String, String>,
) -> Result<()> {
    if file_rewrites.is_empty() {
        return Ok(());
    }
    let file = File::open(todo_path)?;
    let reader = BufReader::new(file);
    let mut lines = Vec::new();
    for line in reader.lines() {
        let mut l = line?;
        if let Some(new_raw) = file_rewrites.get(&l) {
            l = new_raw.clone();
        }
        lines.push(l);
    }
    let mut content = lines.join("\n");
    if !content.is_empty() {
        content.push('\n');
    }

    atomic_write(todo_path, &content)
}
