use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use std::path::Path;
use std::fs;
use anyhow::Result;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

pub fn init_db(db_dir: &Path) -> Result<SqliteConnection> {
    fs::create_dir_all(db_dir)?;
    let db_path = db_dir.join("logfine.db");
    let db_url = db_path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid database path"))?;
    
    let mut db_connection = SqliteConnection::establish(db_url)?;
    db_connection.run_pending_migrations(MIGRATIONS).map_err(|e| anyhow::anyhow!("Migration error: {}", e))?;
    Ok(db_connection)
}
