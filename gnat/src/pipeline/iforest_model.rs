/*
 * Galileo Network Analytics (GNA) Toolkit
 *
 * Copyright 2024-2025 Fidelis Farm & Technologies, LLC
 * All Rights Reserved.
 * See license information in LICENSE.
 */

use crate::model::iforest::eif_model::{
    IForestModels, DEFAULT_EXTENSION_LEVEL, DEFAULT_NUM_TREES, DEFAULT_SUBSAMPLE_SIZE,
};
use crate::model::table::DistinctObservation;
use crate::pipeline::check_parquet_stream;
use crate::pipeline::load_environment;
use crate::pipeline::parse_interval;
use crate::pipeline::parse_options;
use crate::pipeline::FileProcessor;
use crate::pipeline::Interval;
use crate::pipeline::StreamType;
use crate::utils::common::format_parquet_list;
use crate::utils::duckdb::duckdb_open;
use chrono::Datelike;
use chrono::{DateTime, Utc};
use file_lock::{FileLock, FileOptions};
use std::fs;
use std::path::PathBuf;

use std::collections::HashMap;
use std::io::Error;
use std::path::Path;

/// Minimum number of days of data required to build a model
pub const MINIMUM_DAYS: u32 = 7;

struct DbFileRemover {
    path: PathBuf,
}

impl Drop for DbFileRemover {
    fn drop(&mut self) {
        if Path::new(&self.path).exists() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub struct IForestModelProcessor {
    pub command: String,
    pub input_list: Vec<String>,
    pub model_list: Vec<String>,
    pub pass: String,
    pub interval: Interval,
    pub extension: String,
    pub feature_list: Vec<String>,
    pub protocol_list: Vec<String>,
    pub num_trees: usize,
    pub subsample_size: usize,
    pub extension_level: usize,
}

impl IForestModelProcessor {
    pub fn new(
        command: &str,
        input: &str,
        output: &str,
        pass: &str,
        interval_string: &str,
        extension_string: &str,
        options_string: &str,
    ) -> Result<Self, Error> {
        let _ = load_environment();
        let interval = parse_interval(interval_string);
        let mut options = parse_options(options_string);

        // Default features for IForest - numerical features that work well for anomaly detection
        options
            .entry("features")
            .or_insert("dur,rtt,pcr,spkts,dpkts,sbytes,dbytes,sentropy,dentropy,siat,diat");
        options.entry("proto").or_insert("udp,tcp");
        options.entry("trees").or_insert("100");
        options.entry("subsample").or_insert("256");
        options.entry("extension").or_insert("1");

        for (key, value) in &options {
            if !value.is_empty() {
                println!("{}: [{}=>{}]", command, key, value);
            }
        }

        let features = options.get("features").expect("expected feature list");
        let feature_list: Vec<String> = features.split(",").map(str::to_string).collect();

        let protocols = options
            .get("proto")
            .expect("expected proto list")
            .to_string();
        let protocol_list: Vec<String> = protocols.split(",").map(str::to_string).collect();

        let num_trees: usize = options
            .get("trees")
            .unwrap_or(&"100")
            .parse()
            .unwrap_or(DEFAULT_NUM_TREES);

        let subsample_size: usize = options
            .get("subsample")
            .unwrap_or(&"256")
            .parse()
            .unwrap_or(DEFAULT_SUBSAMPLE_SIZE);

        let extension_level: usize = options
            .get("extension")
            .unwrap_or(&"1")
            .parse()
            .unwrap_or(DEFAULT_EXTENSION_LEVEL);

        let mut input_list = Vec::<String>::new();
        input_list.push(input.to_string());
        let mut model_list = Vec::<String>::new();
        model_list.push(output.to_string());

        Ok(Self {
            command: command.to_string(),
            input_list,
            model_list,
            pass: pass.to_string(),
            interval,
            extension: extension_string.to_string(),
            feature_list,
            protocol_list,
            num_trees,
            subsample_size,
            extension_level,
        })
    }
}

impl FileProcessor for IForestModelProcessor {
    fn get_command(&self) -> &String {
        &self.command
    }
    fn get_input(&self, input_list: &mut Vec<String>) -> Result<(), Error> {
        *input_list = self.input_list.clone();
        Ok(())
    }
    fn get_output(&self, _output_list: &mut Vec<String>) -> Result<(), Error> {
        Ok(())
    }
    fn get_pass(&self) -> &String {
        &self.pass
    }
    fn get_interval(&self) -> &Interval {
        &self.interval
    }
    fn get_stream_id(&self) -> u32 {
        StreamType::IPFIX as u32
    }
    fn get_file_extension(&self) -> &String {
        &self.extension
    }
    fn socket(&mut self) -> Result<(), Error> {
        Err(Error::other("socket function unsupported"))
    }
    fn delete_files(&self) -> bool {
        false
    }

    fn process(&mut self, file_list: &Vec<String>) -> Result<(), Error> {
        let lock_filename = format!("{}/.lock", self.input_list[0]);
        let options = FileOptions::new().write(true).create(true).append(true);
        let _file_lock = match FileLock::lock(&lock_filename, false, options) {
            Ok(lock) => lock,
            Err(_err) => {
                println!(
                    "{}: unable to acquire lock for {} -- skipping.",
                    self.command, lock_filename
                );
                return Ok(());
            }
        };

        let parquet_list = format_parquet_list(file_list);

        // Check if the parquet files are valid
        if let Ok(status) = check_parquet_stream(&parquet_list) {
            if !status {
                eprintln!(
                    "{}: invalid stream of parquet files, skipping",
                    self.command
                );
                return Ok(());
            }
        }

        // Check if the model file exists; if so, check its age
        if Path::new(&self.model_list[0]).exists() {
            if self.interval != Interval::ONCE {
                let meta = fs::metadata(&self.model_list[0]).map_err(|e| {
                    Error::new(
                        std::io::ErrorKind::Other,
                        format!("failed to get metadata: {}", e),
                    )
                })?;
                let ctime_local = meta.created().map_err(|e| {
                    Error::new(
                        std::io::ErrorKind::Other,
                        format!("failed to get created time: {}", e),
                    )
                })?;
                let ctime_utc: DateTime<Utc> = ctime_local.into();
                if Utc::now().day() == ctime_utc.day() {
                    // Model was created today, skip
                    return Ok(());
                }
                println!(
                    "{}: overwriting existing model file: {}",
                    self.command, self.model_list[0]
                );
            }
        }

        // Load the parquet files into a temporary duckdb database
        let current_utc: DateTime<Utc> = Utc::now();
        let rfc3339_name: String = current_utc.to_rfc3339();
        let tmp_input = format!(
            "{}.{}.tmp",
            self.model_list[0],
            rfc3339_name.replace(":", "-")
        );
        let _tmp_remove = DbFileRemover {
            path: tmp_input.clone().into(),
        };
        let db_input = duckdb_open(&tmp_input, 1)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        // Check the number of days in the dataset
        println!("{}: checking dataset duration...", self.command);
        let sql_days_command = format!(
            "SELECT date_diff('day',first,last) + 1 AS days
             FROM (SELECT MIN(stime) AS first, MAX(stime) AS last FROM read_parquet({}));",
            parquet_list
        );
        let mut stmt = db_input.prepare(&sql_days_command).map_err(|e| {
            Error::new(
                std::io::ErrorKind::Other,
                format!("DuckDB prepare error: {}", e),
            )
        })?;
        let days = stmt
            .query_row([], |row| Ok(row.get::<_, u32>(0).expect("missing days")))
            .expect("missing days");
        if days < MINIMUM_DAYS {
            println!(
                "{}: not enough data to baseline ({} days, need {}), skipping model build.",
                self.command, days, MINIMUM_DAYS
            );
            return Ok(());
        }

        println!(
            "{}: modeling with {} days of sampled data...",
            self.command, days
        );

        // Load observation list
        println!("{}: determining observation points...", self.command);
        let sql_distinct = format!(
            "SELECT DISTINCT observe, dvlan, proto FROM read_parquet({}) GROUP BY ALL ORDER BY ALL;",
            parquet_list
        );
        let mut stmt = db_input
            .prepare(&sql_distinct)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;
        let record_iter = stmt
            .query_map([], |row| {
                Ok(DistinctObservation {
                    observe: row.get(0)?,
                    vlan: row.get(1)?,
                    proto: row.get(2)?,
                })
            })
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        let mut distinct_observation_models: Vec<DistinctObservation> = Vec::new();
        for record in record_iter {
            distinct_observation_models.push(record.map_err(|e| {
                Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {:?}", e))
            })?);
        }

        let _ = db_input
            .close()
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {:?}", e)))?;

        if Path::new(&tmp_input).exists() {
            fs::remove_file(&tmp_input).map_err(|e| {
                Error::new(
                    std::io::ErrorKind::Other,
                    format!("failed to remove file: {}", e),
                )
            })?;
        }

        // Build IForest models
        let mut distinct_models = HashMap::new();
        for record in &distinct_observation_models {
            // Protocol list filtering
            if !self.protocol_list.is_empty() && !self.protocol_list.contains(&record.proto) {
                continue;
            }
            let distinct_key = format!("{}/{}/{}", record.observe, record.vlan, record.proto);
            println!(
                "{}: building IForest model [{}]",
                self.command, distinct_key
            );

            let mut model = IForestModels::new(&record.observe, record.vlan, &record.proto);

            match model.build(
                &parquet_list,
                &self.feature_list,
                self.num_trees,
                self.subsample_size,
                self.extension_level,
            ) {
                Ok(_) => {
                    println!(
                        "{}: built IForest model for [{}]",
                        self.command, distinct_key
                    );
                    distinct_models.insert(distinct_key, model);
                }
                Err(e) => {
                    eprintln!(
                        "{}: failed to build IForest model for [{}]: {}",
                        self.command, distinct_key, e
                    );
                }
            }
        }
        println!("{}: done building models", self.command);

        // Create output database
        let current_utc: DateTime<Utc> = Utc::now();
        let rfc3339_name: String = current_utc.to_rfc3339();
        let tmp_output = format!(
            "{}.{}.tmp",
            self.model_list[0],
            rfc3339_name.replace(":", "-")
        );
        let _tmp_remove = DbFileRemover {
            path: tmp_output.clone().into(),
        };

        let mut db_output = duckdb_open(&tmp_output, 1)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        // Serialize models to DuckDB
        for (distinct_key, model) in distinct_models.iter() {
            println!(
                "{}: serializing IForest model [{}]",
                self.command, distinct_key
            );
            let _ = model.serialize(&mut db_output).map_err(|e| {
                Error::new(std::io::ErrorKind::Other, format!("serialize error: {}", e))
            })?;
        }
        println!("{}: done serializing", self.command);

        // Summarize IForest models (calculate severity thresholds)
        for (distinct_key, model) in distinct_models.iter_mut() {
            println!(
                "{}: calculating IForest summary [{}]",
                self.command, distinct_key
            );
            let _ = model
                .summarize(&parquet_list, &mut db_output)
                .map_err(|e| {
                    Error::new(std::io::ErrorKind::Other, format!("summarize error: {}", e))
                })?;
        }

        let _ = db_output
            .close()
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {:?}", e)))?;
        println!("{}: done summarizing", self.command);

        // Replace old model file with new one
        if Path::new(&self.model_list[0]).exists() {
            // Backup the old model file
            let current_utc: DateTime<Utc> = Utc::now();
            let rfc3339_name: String = current_utc.to_rfc3339();
            let backup_file = format!("{}.{}", self.model_list[0], rfc3339_name.replace(":", "-"));
            fs::rename(&self.model_list[0], &backup_file).map_err(|e| {
                Error::new(
                    std::io::ErrorKind::Other,
                    format!("failed to rename backup model: {}", e),
                )
            })?;
        }

        if Path::new(&tmp_output).exists() {
            fs::rename(&tmp_output, &self.model_list[0]).map_err(|e| {
                Error::new(
                    std::io::ErrorKind::Other,
                    format!("failed to rename model: {}", e),
                )
            })?;
        }

        Ok(())
    }
}
