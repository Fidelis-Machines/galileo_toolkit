/*
 * Galileo Network Analytics (GNA) Toolkit
 *
 * Copyright 2024-2025 Fidelis Farm & Technologies, LLC
 * All Rights Reserved.
 * See license information in LICENSE.
 */

use chrono::Utc;
use std::collections::HashMap;
use crate::error::{GnatError, Result};

/// Format a list of file paths into a DuckDB parquet list format
pub fn format_parquet_list(files: &[String]) -> String {
    let quoted = files
        .iter()
        .map(|f| format!("'{}'", f))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{}]", quoted)
}

/// Create a temporary filename with timestamp
pub fn create_temp_filename(output_dir: &str, command: &str) -> String {
    let current_utc = Utc::now();
    let safe_timestamp = current_utc.to_rfc3339().replace(":", "-");
    format!("{}/.gnat-{}-{}.parquet", output_dir, command, safe_timestamp)
}

/// Create a final filename with timestamp
pub fn create_final_filename(output_dir: &str, command: &str) -> String {
    let current_utc = Utc::now();
    let safe_timestamp = current_utc.to_rfc3339().replace(":", "-");
    format!("{}/gnat-{}-{}.parquet", output_dir, command, safe_timestamp)
}

/// Create both temporary and final filenames
pub fn create_output_filenames(output_dir: &str, command: &str) -> (String, String) {
    let current_utc = Utc::now();
    let safe_timestamp = current_utc.to_rfc3339().replace(":", "-");
    let tmp = format!("{}/.gnat-{}-{}.parquet", output_dir, command, safe_timestamp);
    let final_name = format!("{}/gnat-{}-{}.parquet", output_dir, command, safe_timestamp);
    (tmp, final_name)
}

/// Parse options string into HashMap
pub fn parse_options_safe(options_string: &str) -> Result<HashMap<String, String>> {
    let mut options = HashMap::new();

    if options_string.is_empty() {
        return Ok(options);
    }

    for pair in options_string.split(';') {
        let parts: Vec<&str> = pair.splitn(2, '=').collect();
        if parts.len() != 2 {
            return Err(GnatError::Parse(format!("Invalid option format: {}", pair)));
        }
        options.insert(parts[0].to_string(), parts[1].to_string());
    }

    Ok(options)
}

/// Options helper for easier access
pub struct ProcessorOptions {
    values: HashMap<String, String>,
}

impl ProcessorOptions {
    pub fn from_string(s: &str) -> Result<Self> {
        Ok(Self {
            values: parse_options_safe(s)?,
        })
    }

    pub fn get_or_default(&self, key: &str, default: &str) -> String {
        self.values
            .get(key)
            .cloned()
            .unwrap_or_else(|| default.to_string())
    }

    pub fn get_required(&self, key: &str) -> Result<String> {
        self.values
            .get(key)
            .cloned()
            .ok_or_else(|| GnatError::Config(format!("Required option missing: {}", key)))
    }

    pub fn get_parsed<T: std::str::FromStr>(&self, key: &str) -> Result<T>
    where
        T::Err: std::fmt::Display,
    {
        let value = self.get_required(key)?;
        value.parse::<T>().map_err(|e| {
            GnatError::Parse(format!("Failed to parse option '{}': {}", key, e))
        })
    }

    pub fn contains(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.values.iter()
    }
}
