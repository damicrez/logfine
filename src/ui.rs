use anyhow::Result;
use edit::{edit_with_builder, Builder};
use inquire::validator::Validation;
use inquire::{CustomType, MultiSelect};

/// Prompt the user to enter their energy state (1-3)
pub fn prompt_energy_state(default_val: u8) -> Result<u8> {
    let validator = |val: &u8| -> Result<
        Validation,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        if (1..=3).contains(val) {
            Ok(Validation::Valid)
        } else {
            Ok(Validation::Invalid("Value must be between 1 and 3".into()))
        }
    };
    let energy = CustomType::<u8>::new("Today's energy state")
        .with_validator(validator)
        .with_error_message("Please type a valid number")
        .with_help_message("1-3, Low-High")
        .with_default(default_val)
        .prompt()?;
    Ok(energy)
}

/// Prompt the user to select completed Minimum Viable Output (MVO) items
pub fn prompt_mvo_items(items: &[String], existing_mvos: &[String]) -> Result<Vec<String>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let default_indices: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| existing_mvos.contains(item))
        .map(|(idx, _)| idx)
        .collect();
    let checked = MultiSelect::new("Today's minimum viable output", items.to_vec())
        .with_default(&default_indices)
        .with_vim_mode(true)
        .without_help_message()
        .prompt()?;
    Ok(checked)
}

/// Parses a bracketed section e.g. `[What worked]` from markdown content
pub fn parse_section(content: &str, section: &str) -> Vec<String> {
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

/// Launch default text editor for inputting log details (what worked, failed, output)
pub fn launch_log(
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
        parse_section(&content, "Output"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_mvo_items_empty() {
        let items: Vec<String> = Vec::new();
        let existing: Vec<String> = Vec::new();
        let result = prompt_mvo_items(&items, &existing).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_section_basic() {
        let content = "[What worked]\n- Task A\n- Task B\n\n[What failed]\n- Task C\n";
        assert_eq!(parse_section(content, "What worked"), vec!["Task A", "Task B"]);
        assert_eq!(parse_section(content, "What failed"), vec!["Task C"]);
        assert!(parse_section(content, "Output").is_empty());
    }
}
