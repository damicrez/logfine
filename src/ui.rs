use anyhow::Result;
use edit::{edit_with_builder, Builder};
use inquire::validator::Validation;
use inquire::{CustomType, MultiSelect};

use indexmap::IndexMap;

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

/// Parses a section from markdown content.
/// Supports both `[Section Name]` and `#+ Section Name` header formats.
pub fn parse_section(content: &str, section: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut in_section = false;

    for line in content.lines() {
        let trimmed = line.trim();
        let is_match = (trimmed.starts_with('[') && trimmed.ends_with(']') && &trimmed[1..trimmed.len() - 1] == section)
            || (trimmed.starts_with('#') && trimmed.trim_start_matches('#').trim() == section);

        if is_match {
            in_section = true;
        } else if (trimmed.starts_with('[') && trimmed.ends_with(']')) || trimmed.starts_with('#') {
            in_section = false;
        } else if in_section && !trimmed.is_empty() {
            result.push(trimmed.strip_prefix("- ").unwrap_or(trimmed).to_string());
        }
    }
    result
}

/// Launch default text editor for inputting log details using dynamic sections.
/// `existing_data` maps section names to their existing entries.
pub fn launch_log(
    sections: &[String],
    existing_data: &IndexMap<String, Vec<String>>,
) -> Result<IndexMap<String, Vec<String>>> {
    let mut template = String::new();

    for section in sections {
        template.push_str(&format!("[{}]\n", section));
        if let Some(items) = existing_data.get(section) {
            if items.is_empty() {
                template.push('\n');
            } else {
                for item in items {
                    template.push_str(&format!("- {}\n", item));
                }
                template.push('\n');
            }
        } else {
            template.push('\n');
        }
    }

    let mut builder = Builder::new();
    builder
        .prefix("log-")
        .suffix(".md");
    let content = edit_with_builder(&template, &builder)?;

    let mut result = IndexMap::new();
    for section in sections {
        result.insert(section.clone(), parse_section(&content, section));
    }

    // Capture any additional sections added by the user
    for line in content.lines() {
        let trimmed = line.trim();
        let found_section = if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() > 2 {
            Some(trimmed[1..trimmed.len() - 1].to_string())
        } else if trimmed.starts_with('#') {
            let s = trimmed.trim_start_matches('#').trim();
            if !s.is_empty() {
                Some(s.to_string())
            } else {
                None
            }
        } else {
            None
        };

        if let Some(sec) = found_section {
            if !result.contains_key(&sec) {
                result.insert(sec.clone(), parse_section(&content, &sec));
            }
        }
    }

    Ok(result)
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
    fn test_parse_section_bracket() {
        let content = "[What worked]\n- Task A\n- Task B\n\n[What failed]\n- Task C\n";
        assert_eq!(parse_section(content, "What worked"), vec!["Task A", "Task B"]);
        assert_eq!(parse_section(content, "What failed"), vec!["Task C"]);
        assert!(parse_section(content, "Output").is_empty());
    }

    #[test]
    fn test_parse_section_markdown_heading() {
        let content = "## What worked\n- Task A\n- Task B\n\n# What failed\n- Task C\n";
        assert_eq!(parse_section(content, "What worked"), vec!["Task A", "Task B"]);
        assert_eq!(parse_section(content, "What failed"), vec!["Task C"]);
        assert!(parse_section(content, "Output").is_empty());
    }

    #[test]
    fn test_parse_section_mixed() {
        let content = "[What worked]\n- Task A\n\n## What failed\n- Task B\n\n### Output\n- Task C\n";
        assert_eq!(parse_section(content, "What worked"), vec!["Task A"]);
        assert_eq!(parse_section(content, "What failed"), vec!["Task B"]);
        assert_eq!(parse_section(content, "Output"), vec!["Task C"]);
    }
}
