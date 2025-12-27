/*
 * Galileo Network Analytics (GNA) Toolkit
 *
 * Copyright 2024-2025 Fidelis Farm & Technologies, LLC
 * All Rights Reserved.
 * See license information in LICENSE.
 */

use crate::model::iforest::eif_model::{IForestModels, IForestSummaryRecord};
use crate::model::table::DistinctObservation;

use crate::pipeline::check_parquet_stream;
use crate::pipeline::load_environment;
use crate::pipeline::StreamType;
use crate::sql::queries;
use crate::utils::common::{create_output_filenames, format_parquet_list};
use crate::utils::duckdb::{duckdb_open_memory, duckdb_open_readonly};
use duckdb::params;
use std::collections::HashMap;
use std::fs;
use std::io::Error;
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::pipeline::parse_interval;
use crate::pipeline::parse_options;
use crate::pipeline::FileProcessor;
use crate::pipeline::Interval;

/// SQL queries specific to IForest
pub const IFOREST_DISTINCT_OBSERVATIONS: &str =
    "SELECT DISTINCT observe, vlan, proto FROM iforest_summary WHERE 1=1 GROUP BY ALL ORDER BY ALL;";

pub const SELECT_IFOREST_SUMMARY: &str =
    "SELECT * FROM iforest_summary WHERE observe=? AND vlan=? AND proto=?;";

/// Score table for IForest (same structure as HBOS for compatibility)
pub const CREATE_IFOREST_SCORE_TABLE: &str =
    "CREATE TABLE score_table (id UUID, hbos_score DOUBLE, hbos_severity UTINYINT);";

pub const CREATE_IFOREST_MAP_TABLE: &str =
    "CREATE TABLE hbos_map_table (id UUID, risk_list VARCHAR[], hbos_map MAP(VARCHAR, DOUBLE));";

pub struct IForestProcessor {
    pub command: String,
    pub input_list: Vec<String>,
    pub output_list: Vec<String>,
    pub pass: String,
    pub interval: Interval,
    pub extension: String,
    pub model_spec: String,
    pub model_mtime: u64,
    pub protocol_list: Vec<String>,
    pub iforest_summary_map: HashMap<String, IForestSummaryRecord>,
    pub iforest_map: HashMap<String, IForestModels>,
}

impl IForestProcessor {
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
        options.entry("proto").or_insert("udp,tcp");
        for (key, value) in &options {
            if !value.is_empty() {
                println!("{}: [{}=>{}]", command, key, value);
            }
        }

        let model_file = options
            .get("model")
            .expect("expected --option model=file")
            .to_string();
        let protocols = options
            .get("proto")
            .expect("expected proto list")
            .to_string();
        let protocol_list: Vec<String> = protocols.split(",").map(str::to_string).collect();

        let mtime = IForestProcessor::file_modified_time_in_seconds(&model_file);

        let mut input_list = Vec::<String>::new();
        input_list.push(input.to_string());
        let mut output_list = Vec::<String>::new();
        output_list.push(output.to_string());
        Ok(Self {
            command: command.to_string(),
            input_list,
            output_list,
            pass: pass.to_string(),
            interval,
            extension: extension_string.to_string(),
            model_spec: model_file,
            model_mtime: mtime,
            protocol_list,
            iforest_summary_map: HashMap::new(),
            iforest_map: HashMap::new(),
        })
    }

    pub fn file_modified_time_in_seconds(path: &str) -> u64 {
        if !Path::new(path).exists() {
            return 0;
        }
        match fs::metadata(path)
            .and_then(|meta| meta.modified())
            .and_then(|mtime| Ok(mtime.duration_since(UNIX_EPOCH)))
        {
            Ok(duration) => duration.expect("as secs").as_secs(),
            Err(_) => 0,
        }
    }

    fn load_iforest_summary(&mut self) -> Result<(), Error> {
        if !Path::new(&self.model_spec).exists() {
            let error_msg = format!(
                "{}: model file {} does not exist",
                self.command, self.model_spec
            );
            return Err(Error::other(error_msg));
        }

        println!("{}: loading iforest summary information", self.command);
        let model_conn = duckdb_open_readonly(&self.model_spec, 1)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        let mut stmt = model_conn
            .prepare(IFOREST_DISTINCT_OBSERVATIONS)
            .map_err(|e| {
                Error::new(
                    std::io::ErrorKind::Other,
                    format!("DuckDB prepare error: {}", e),
                )
            })?;

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
                Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e))
            })?);
        }

        let mut iforest_summary_map: HashMap<String, IForestSummaryRecord> = HashMap::new();
        for record in distinct_observation_models.clone().into_iter() {
            let distinct_key = format!("{}/{}/{}", record.observe, record.vlan, record.proto);

            let mut stmt = model_conn
                .prepare(SELECT_IFOREST_SUMMARY)
                .map_err(|e| {
                    Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e))
                })?;

            let iforest_summary = stmt
                .query_row(
                    params![&record.observe, &record.vlan, &record.proto],
                    |row| {
                        Ok(IForestSummaryRecord {
                            observe: row.get(0)?,
                            vlan: row.get(1)?,
                            proto: row.get(2)?,
                            num_trees: row.get(3)?,
                            subsample_size: row.get(4)?,
                            extension_level: row.get(5)?,
                            num_features: row.get(6)?,
                            feature_names: row.get(7)?,
                            min: row.get(8)?,
                            max: row.get(9)?,
                            skewness: row.get(10)?,
                            avg: row.get(11)?,
                            std: row.get(12)?,
                            mad: row.get(13)?,
                            median: row.get(14)?,
                            quantile: row.get(15)?,
                            low: row.get(16)?,
                            medium: row.get(17)?,
                            high: row.get(18)?,
                            severe: row.get(19)?,
                            critical: row.get(20)?,
                        })
                    },
                )
                .map_err(|e| {
                    Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e))
                })?;

            iforest_summary_map.insert(distinct_key, iforest_summary);
        }

        let _ = model_conn.close();

        self.iforest_summary_map = iforest_summary_map;

        Ok(())
    }

    fn load_iforest_model(&mut self) -> Result<(), Error> {
        if !Path::new(&self.model_spec).exists() {
            let error_msg = format!(
                "{}: model file {} does not exist",
                self.command, self.model_spec
            );
            return Err(Error::other(error_msg));
        }
        println!("{}: loading iforest model information", self.command);
        let model_conn = duckdb_open_readonly(&self.model_spec, 1)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        let mut stmt = model_conn
            .prepare(IFOREST_DISTINCT_OBSERVATIONS)
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

        let mut distinct_observation: Vec<DistinctObservation> = Vec::new();
        for record in record_iter {
            distinct_observation.push(record.map_err(|e| {
                Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e))
            })?);
        }

        let mut distinct_models = HashMap::new();
        for record in distinct_observation.clone().into_iter() {
            let distinct_key = format!("{}/{}/{}", record.observe, record.vlan, record.proto);
            println!("iforest: loading model [{}]", distinct_key);

            let mut model = IForestModels::new(&record.observe, record.vlan, &record.proto);

            let _ = model.deserialize(&model_conn);
            println!(
                "{}: low={}, medium={}, high={}, severe={}",
                self.command, model.low, model.medium, model.high, model.severe
            );
            distinct_models.insert(distinct_key, model);
        }
        let _ = model_conn.close();
        self.iforest_map = distinct_models;

        Ok(())
    }

    fn load_model(&mut self) -> Result<(), Error> {
        if self.iforest_summary_map.is_empty() {
            match self.load_iforest_summary() {
                Ok(_) => println!("{}: loaded iforest summary", self.command),
                Err(e) => return Err(e),
            }
        }
        if self.iforest_map.is_empty() {
            match self.load_iforest_model() {
                Ok(_) => println!("{}: loaded model", self.command),
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

impl FileProcessor for IForestProcessor {
    fn get_command(&self) -> &String {
        &self.command
    }
    fn get_input(&self, input_list: &mut Vec<String>) -> Result<(), Error> {
        *input_list = self.input_list.clone();
        Ok(())
    }
    fn get_output(&self, output_list: &mut Vec<String>) -> Result<(), Error> {
        *output_list = self.output_list.clone();
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
        true
    }
    fn process(&mut self, file_list: &Vec<String>) -> Result<(), Error> {
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

        if let Err(_e) = self.load_model() {
            // If loading the model fails, just forward the data
            println!("{}: model does not exists; skipping...", self.command);
            let _ = self.forward(&parquet_list, &self.output_list.clone());
            return Ok(());
        }

        let (tmp_filename, final_filename) =
            create_output_filenames(&self.output_list[0], &self.command);

        let mut db_conn = duckdb_open_memory(1)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        // Create score tables (using same names as HBOS for compatibility)
        db_conn
            .execute_batch(CREATE_IFOREST_SCORE_TABLE)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;
        db_conn
            .execute_batch(CREATE_IFOREST_MAP_TABLE)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        let sql_command = format!(
            "CREATE TABLE flow AS SELECT * FROM read_parquet({});",
            parquet_list
        );
        db_conn
            .execute_batch(&sql_command)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        println!("{}: determining observation points...", self.command);
        let mut stmt = db_conn
            .prepare(queries::PARQUET_DISTINCT_OBSERVATIONS)
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
                Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e))
            })?);
        }

        println!("{}: scoring...", self.command);
        for record in &distinct_observation_models {
            // protocol_list filtering
            if !self.protocol_list.is_empty() && !self.protocol_list.contains(&record.proto) {
                continue;
            }
            let distinct_key = format!("{}/{}/{}", record.observe, record.vlan, record.proto);
            println!("{}:\t{}", self.command, distinct_key);
            let iforest_model = match self.iforest_map.get(&distinct_key) {
                None => {
                    eprintln!("Warning: missing iforest model {}", distinct_key);
                    continue;
                }
                Some(model) => model,
            };

            let _ = iforest_model.score(&mut db_conn).map_err(|e| {
                Error::new(
                    std::io::ErrorKind::Other,
                    format!("iforest scoring error: {}", e),
                )
            })?;
        }

        // Update the flow table with scores (using HBOS column names for compatibility)
        println!("{}: updating records...", self.command);
        db_conn
            .execute_batch(queries::UPDATE_FLOW_WITH_SCORES)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        // Export the flow table to parquet
        let sql_export_command = format!(
            "COPY (SELECT * FROM flow) TO '{}' (FORMAT parquet);",
            tmp_filename
        );
        db_conn
            .execute_batch(&sql_export_command)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;
        let _ = db_conn.close();

        if Path::new(&tmp_filename).exists() {
            fs::rename(&tmp_filename, &final_filename)?;
        }

        if Path::new(&self.model_spec).exists() {
            let mtime = IForestProcessor::file_modified_time_in_seconds(&self.model_spec);
            if mtime != self.model_mtime {
                println!("{}: (re)loading new model...", self.command);
                self.model_mtime = mtime;
                self.iforest_summary_map.clear();
                self.iforest_map.clear();
            }
        }

        Ok(())
    }
}
