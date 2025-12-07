use duckdb::{AccessMode, Config, Connection, Error};
use std::path::PathBuf;
use crate::config::GnatConfig;

pub enum DbMode {
    ReadWrite,
    ReadOnly,
    Memory,
}

pub struct DbConfig {
    pub memory_gb: u32,
    pub threads: i64,
    pub temp_dir: PathBuf,
    pub max_temp_size: String,
}

impl Default for DbConfig {
    fn default() -> Self {
        let gnat_config = GnatConfig::default();
        Self {
            memory_gb: gnat_config.default_memory_gb,
            threads: gnat_config.duckdb_threads,
            temp_dir: gnat_config.temp_directory,
            max_temp_size: gnat_config.max_temp_size,
        }
    }
}

impl DbConfig {
    pub fn with_memory(mut self, gb: u32) -> Self {
        self.memory_gb = gb;
        self
    }

    pub fn with_threads(mut self, threads: i64) -> Self {
        self.threads = threads;
        self
    }
}

pub fn duckdb_connect(path: &str, mode: DbMode, config: DbConfig) -> Result<Connection, Error> {
    let mem_threshold = format!("{}GB", config.memory_gb);
    let sql_temp_config = format!(
        "SET max_temp_directory_size = '{}'; SET temp_directory = '{}';",
        config.max_temp_size,
        config.temp_dir.display()
    );

    let mut db_config = Config::default()
        .max_memory(&mem_threshold)?
        .threads(config.threads)?;

    if matches!(mode, DbMode::ReadOnly) {
        db_config = db_config.access_mode(AccessMode::ReadOnly)?;
    }

    let conn = match (path, &mode) {
        (":memory:", _) | (_, DbMode::Memory) => {
            Connection::open_in_memory_with_flags(db_config)?
        }
        (path, _) if path.starts_with("md:") => {
            Connection::open_with_flags(path, db_config)?
        }
        (path, _) => {
            Connection::open_with_flags(path, db_config)?
        }
    };

    conn.execute_batch(&sql_temp_config)?;
    Ok(conn)
}

// Legacy compatibility functions
pub fn duckdb_open(db_file: &str, mem_gig: u32) -> Result<Connection, duckdb::Error> {
    let config = DbConfig::default().with_memory(mem_gig);
    duckdb_connect(db_file, DbMode::ReadWrite, config)
}

pub fn duckdb_open_readonly(db_file: &str, mem_gig: u32) -> Result<Connection, duckdb::Error> {
    let config = DbConfig::default().with_memory(mem_gig);
    duckdb_connect(db_file, DbMode::ReadOnly, config)
}

pub fn duckdb_open_memory(mem_gig: u32) -> Result<Connection, duckdb::Error> {
    let config = DbConfig::default().with_memory(mem_gig);
    duckdb_connect(":memory:", DbMode::Memory, config)
}
