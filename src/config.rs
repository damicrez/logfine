use std::fs;
use std::io;
use std::path::PathBuf;
use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::cli::{COLOR_INFO, COLOR_RESET, COLOR_WARN};

/// Configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub logbook_path: PathBuf,
    pub todo_path: PathBuf,
    #[serde(default)]
    pub mvos: Vec<String>,
    #[serde(default)]
    pub delete_tasks: bool,
    #[serde(default)]
    pub automatic_completion_date: bool,
    #[serde(default)]
    pub template_path: Option<PathBuf>,
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
            automatic_completion_date: false,
            template_path: None,
        }
    }
}

/// Loads configuration from the local user directory or initializes default config
pub fn load_config() -> Result<Config> {
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
                let default_config = Config {
                    mvos: vec![
                        "Read one chapter of a book".to_string(),
                        "Do some exercise".to_string(),
                        "One push commit to GitHub".to_string(),
                    ],
                    ..Default::default()
                };

                // Warn if an existing database is found at the default path
                let db_path = default_config.logbook_path.join("logfine.db");
                if db_path.exists() {
                    eprintln!("{COLOR_WARN}Warning: An existing database was found at:{COLOR_RESET} {:?}", db_path);
                    eprintln!("{COLOR_WARN}but no config file exists. A default config will be written.{COLOR_RESET}");
                    eprintln!("{COLOR_WARN}If you migrated from another system, restore your config to:{COLOR_RESET} {:?}", path);
                }

                println!("{COLOR_INFO}Config file not found. Writing default config at:{COLOR_RESET} {:?}", path);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                
                let default_content = toml::to_string(&default_config)?;
                fs::write(&path, default_content)?;
                
                println!("{COLOR_INFO}Database will be stored at:{COLOR_RESET} {:?}", default_config.logbook_path.join("logfine.db"));
                Ok(default_config)
            } else {
                Err(err.into())
            }
        }
    }
}

/// Default template section used when no custom template is configured
pub const DEFAULT_TEMPLATE_SECTIONS: &[&str] = &["What worked", "What failed", "Output"];

/// Loads template sections from a markdown file, or returns the default sections.
pub fn load_template_sections(template_path: &Option<PathBuf>) -> Vec<String> {
    let Some(path) = template_path else {
        return DEFAULT_TEMPLATE_SECTIONS.iter().map(|s| s.to_string()).collect();
    };

    let resolved_path = if let Some(path_str) = path.to_str() {
        if let Some(stripped) = path_str.strip_prefix("~/") {
            dirs::home_dir().map(|h| h.join(stripped)).unwrap_or_else(|| path.clone())
        } else {
            path.clone()
        }
    } else {
        path.clone()
    };

    match fs::read_to_string(&resolved_path) {
        Ok(content) => {
            let sections: Vec<String> = content
                .lines()
                .filter_map(|line| {
                    let trimmed = line.trim();
                    if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() > 2 {
                        Some(trimmed[1..trimmed.len() - 1].to_string())
                    } else if trimmed.starts_with('#') {
                        let header = trimmed.trim_start_matches('#').trim();
                        if !header.is_empty() {
                            Some(header.to_string())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect();

            if sections.is_empty() {
                eprintln!(
                    "{COLOR_WARN}Warning: Template file {:?} contains no sections. Using default template.{COLOR_RESET}",
                    resolved_path
                );
                DEFAULT_TEMPLATE_SECTIONS.iter().map(|s| s.to_string()).collect()
            } else {
                sections
            }
        }
        Err(_) => {
            eprintln!(
                "{COLOR_WARN}Warning: Template file {:?} not found. Using default template.{COLOR_RESET}",
                resolved_path
            );
            DEFAULT_TEMPLATE_SECTIONS.iter().map(|s| s.to_string()).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_load_template_sections_none() {
        let sections = load_template_sections(&None);
        assert_eq!(sections, vec!["What worked", "What failed", "Output"]);
    }

    #[test]
    fn test_load_template_sections_missing_file() {
        let missing = Some(PathBuf::from("/nonexistent/path/to/template.md"));
        let sections = load_template_sections(&missing);
        assert_eq!(sections, vec!["What worked", "What failed", "Output"]);
    }

    #[test]
    fn test_load_template_sections_bracket_format() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_template_bracket.md");
        let mut f = fs::File::create(&file_path).unwrap();
        writeln!(f, "[Logros]\nDetalles...\n\n[Errores]\nDetalles...\n\n[Ideas]").unwrap();

        let sections = load_template_sections(&Some(file_path.clone()));
        assert_eq!(sections, vec!["Logros", "Errores", "Ideas"]);
        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn test_load_template_sections_markdown_heading_format() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_template_heading.md");
        let mut f = fs::File::create(&file_path).unwrap();
        writeln!(f, "## Logros\nDetalles...\n\n## Errores\nDetalles...\n\n## Aprendizajes").unwrap();

        let sections = load_template_sections(&Some(file_path.clone()));
        assert_eq!(sections, vec!["Logros", "Errores", "Aprendizajes"]);
        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn test_load_template_sections_mixed_format() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_template_mixed.md");
        let mut f = fs::File::create(&file_path).unwrap();
        writeln!(f, "[Logros]\n- Algo\n\n## Errores\n- Otro\n\n[Siguientes pasos]").unwrap();

        let sections = load_template_sections(&Some(file_path.clone()));
        assert_eq!(sections, vec!["Logros", "Errores", "Siguientes pasos"]);
        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn test_load_template_sections_empty_file_falls_back() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_template_empty.md");
        let mut f = fs::File::create(&file_path).unwrap();
        writeln!(f, "Just some plain text without sections").unwrap();

        let sections = load_template_sections(&Some(file_path.clone()));
        assert_eq!(sections, vec!["What worked", "What failed", "Output"]);
        let _ = fs::remove_file(file_path);
    }
}
