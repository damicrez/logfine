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
