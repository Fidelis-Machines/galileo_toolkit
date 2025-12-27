/*
 * Galileo Network Analytics (GNA) Toolkit
 *
 * Copyright 2024-2025 Fidelis Farm & Technologies, LLC
 * All Rights Reserved.
 * See license information in LICENSE.
 */

use std::env;
use std::path::PathBuf;

/// Environment variable keys
pub mod env_keys {
    pub const MOTHERDUCK_TOKEN: &str = "motherduck_token";
    pub const ABUSE_API_KEY: &str = "abuse_api_key";
    pub const GNAT_MAX_BATCH: &str = "GNAT_MAX_BATCH";
    pub const GNAT_TEMP_DIR: &str = "GNAT_TEMP_DIR";
    pub const GNAT_MAX_TEMP_SIZE: &str = "GNAT_MAX_TEMP_SIZE";
    pub const GNAT_DUCKDB_THREADS: &str = "GNAT_DUCKDB_THREADS";
}

/// Global configuration for GNAT
#[derive(Debug, Clone)]
pub struct GnatConfig {
    pub max_batch_size: usize,
    pub temp_directory: PathBuf,
    pub max_temp_size: String,
    pub duckdb_threads: i64,
    pub default_memory_gb: u32,
}

impl Default for GnatConfig {
    fn default() -> Self {
        Self {
            max_batch_size: env::var(env_keys::GNAT_MAX_BATCH)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1024),
            temp_directory: env::var(env_keys::GNAT_TEMP_DIR)
                .ok()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/tmp")),
            max_temp_size: env::var(env_keys::GNAT_MAX_TEMP_SIZE)
                .unwrap_or_else(|_| "64GB".to_string()),
            duckdb_threads: env::var(env_keys::GNAT_DUCKDB_THREADS)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(4),
            default_memory_gb: 1,
        }
    }
}

impl GnatConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_max_batch(mut self, size: usize) -> Self {
        self.max_batch_size = size;
        self
    }

    pub fn with_temp_dir(mut self, path: PathBuf) -> Self {
        self.temp_directory = path;
        self
    }
}
