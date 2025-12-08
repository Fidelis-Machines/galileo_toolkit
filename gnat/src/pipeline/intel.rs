/*
 * Galileo Network Analytics (GNA) Toolkit
 *
 * Copyright 2024-2025 Fidelis Farm & Technologies, LLC
 * All Rights Reserved.
 * See license information in LICENSE.
 */

use crate::pipeline::check_parquet_stream;
use crate::pipeline::load_environment;
use crate::pipeline::parse_interval;
use crate::pipeline::parse_options;
use crate::pipeline::FileProcessor;
use crate::pipeline::Interval;
use crate::pipeline::StreamType;
use crate::sql::queries;
use crate::utils::common::format_parquet_list;
use crate::utils::duckdb::duckdb_open;
use chrono::prelude::*;
use chrono::{Duration, Utc};
use duckdb::params;
use duckdb::Appender;
use duckdb::Connection;
use serde::Deserialize;
use serde_with::{serde_as, DefaultOnNull};
use std::io::Error;

use reqwest::blocking::Client;
use reqwest::header::HeaderMap;
use std::collections::HashMap;

use urlencoding::encode;

#[derive(Debug)]
struct IpAddressRecord {
    ipAddress: String,
}

#[derive(Debug, Deserialize)]
struct AbuseResponse {
    data: AbuseData,
}

#[serde_as]
#[derive(Debug, Deserialize)]
struct AbuseData {
    ipAddress: String,
    #[serde_as(as = "DefaultOnNull")]
    isPublic: bool,
    #[serde_as(as = "DefaultOnNull")]
    ipVersion: u16,
    #[serde_as(as = "DefaultOnNull")]
    isWhitelisted: bool,
    #[serde_as(as = "DefaultOnNull")]
    abuseConfidenceScore: u16,
    #[serde_as(as = "DefaultOnNull")]
    countryCode: String,
    #[serde_as(as = "DefaultOnNull")]
    usageType: String,
    #[serde_as(as = "DefaultOnNull")]
    isp: String,
    #[serde_as(as = "DefaultOnNull")]
    domain: String,
    #[serde_as(as = "DefaultOnNull")]
    hostnames: Vec<String>,
    #[serde_as(as = "DefaultOnNull")]
    isTor: bool,
    #[serde_as(as = "DefaultOnNull")]
    totalReports: i32,
    #[serde_as(as = "DefaultOnNull")]
    numDistinctUsers: i32,
    #[serde_as(as = "DefaultOnNull")]
    lastReportedAt: String,
}

pub struct ThreatIntelProcessor {
    pub command: String,
    pub input_list: Vec<String>,
    pub output_list: Vec<String>,
    pub pass: String,
    pub interval: Interval,
    pub extension: String,
    pub threshold: u8,
    pub abuse_url: String,
    pub abuse_key: String,
    pub db_conn: Connection,
    pub active_date: DateTime<Utc>,
}

impl ThreatIntelProcessor {
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
        options
            .entry("abuse_url")
            .or_insert("https://api.abuseipdb.com/api/v2/check");
        options.entry("threshold").or_insert("4"); // default severity level severe (4)
        for (key, value) in &options {
            if !value.is_empty() {
                println!("{}: [{}={}]", command, key, value);
            }
        }

        let threshold = options
            .get("threshold")
            .expect("expected threshold")
            .parse::<u8>()
            .unwrap();

        if threshold > 5 || threshold < 1 {
            return Err(Error::new(
                std::io::ErrorKind::Other,
                format!("invalid threshold level {}", threshold),
            ));
        }

        let abuse_key = options.get("key").expect("expected key").to_string();
        let abuse_url = options
            .get("abuse_url")
            .expect("expected abuse_url")
            .to_string();
        let cache_file = options.get("cache").expect("expected db").to_string();

        let db_conn = duckdb_open(&cache_file, 1)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;
        db_conn
            .execute_batch(queries::CREATE_ABUSE_TABLE)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;
        println!("{}: cache [{}]", command, cache_file);

        let mut input_list = Vec::<String>::new();
        input_list.push(input.to_string());
        let mut output_list = Vec::<String>::new();
        output_list.push(output.to_string());

        Ok(Self {
            command: command.to_string(),
            input_list: input_list,
            output_list: output_list,
            pass: pass.to_string(),
            interval: interval,
            extension: extension_string.to_string(),
            threshold: threshold,
            abuse_url: abuse_url,
            abuse_key: abuse_key,
            db_conn: db_conn,
            active_date: Utc::now(),
        })
    }

    fn is_private_address(&self, ip: &str) -> bool {
        // Parse the string as an IP address. If parsing fails, treat as non-private.
        match ip.parse::<std::net::IpAddr>() {
            Ok(std::net::IpAddr::V4(v4)) => {
                let o = v4.octets();
                // RFC1918 ranges:
                // 10.0.0.0/8
                if o[0] == 10 {
                    return true;
                }
                // 172.16.0.0/12  -> 172.16.0.0 - 172.31.255.255
                if o[0] == 172 && (16..=31).contains(&o[1]) {
                    return true;
                }
                // 192.168.0.0/16
                if o[0] == 192 && o[1] == 168 {
                    return true;
                }
                false
            }
            Ok(std::net::IpAddr::V6(v6)) => {
                // If this is an IPv4-mapped IPv6 address (::ffff:a.b.c.d), check the mapped IPv4
                if let Some(mapped_v4) = v6.to_ipv4() {
                    let o = mapped_v4.octets();
                    if o[0] == 10
                        || (o[0] == 172 && (16..=31).contains(&o[1]))
                        || (o[0] == 192 && o[1] == 168)
                    {
                        return true;
                    }
                }
                // IPv6 Unique Local Addresses (ULA) fc00::/7 are the IPv6 "private" range
                let b = v6.octets();
                (b[0] & 0xfe) == 0xfc
            }
            Err(_) => false,
        }
    }
    fn abusedb_api_lookup(&self, appender: &mut Appender, ipAddress: &str) -> Result<u16, Error> {
        let client = Client::new();

        // Data for URL encoding
        let mut params = HashMap::new();
        params.insert("ipAddress", ipAddress);
        params.insert("maxAgeInDays", "30");
        // Manually encode parameters for the query string
        let query_string: String = params
            .iter()
            .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
            .collect::<Vec<String>>()
            .join("&");

        let full_url = format!("{}?{}", self.abuse_url, query_string);
        println!("{}: lookup [{}]", self.command, full_url);
        // Add headers
        let mut headers = HeaderMap::new();
        headers.insert("Accept", "application/json".parse().unwrap());
        headers.insert("Key", self.abuse_key.parse().unwrap());

        // Make the blocking GET request
        let response =
            client.get(&full_url).headers(headers).send().map_err(|e| {
                Error::new(std::io::ErrorKind::Other, format!("HTTP Get error: {}", e))
            })?;
        let status_code = response.status();
        let status_code_number = status_code.as_u16();
        // Check the response status and print the body
        if response.status().is_success() {
            let data = response.text().map_err(|e| {
                Error::new(
                    std::io::ErrorKind::Other,
                    format!("HTTP Response error: {}", e),
                )
            })?;

            let response: AbuseResponse = serde_json::from_str(&data)?;
            //println!("{}: parsed [{:?}]", self.command, response);
            //
            // insert into database
            //
            let mut hostnames = String::from("");
            if !response.data.hostnames.is_empty() {
                hostnames = response.data.hostnames.join(",");
            }

            let now: DateTime<Utc> = Utc::now();

            appender
                .append_row(params![
                    response.data.ipAddress,
                    response.data.isPublic,
                    response.data.ipVersion,
                    response.data.isWhitelisted,
                    response.data.abuseConfidenceScore,
                    response.data.countryCode,
                    response.data.usageType,
                    response.data.isp,
                    response.data.domain,
                    hostnames,
                    response.data.isTor,
                    response.data.totalReports,
                    response.data.numDistinctUsers,
                    response.data.lastReportedAt,
                    now.to_rfc3339()
                ])
                .map_err(|e| {
                    eprintln!("Error: {}", e);
                    Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e))
                })?;
        } else {
            if status_code_number == 429 {
                eprintln!("gnat_intel: Rate limit exceeded for AbuseIPDB API.");
                return Ok(status_code_number);
            } else {
                eprintln!("Error: {}", response.status());
                eprintln!(
                    "Error: {}",
                    response.text().map_err(|e| {
                        Error::new(
                            std::io::ErrorKind::Other,
                            format!("HTTP client error: {}", e),
                        )
                    })?
                );
            }
        }
        Ok(status_code_number)
    }

    fn abusedb_export(&mut self, parquet_list: &String) -> Result<(), Error> {
        //
        // look up ip addresses not in cache
        //

        let sql_command = format!(
            "SELECT daddr AS ipAddress FROM read_parquet({})
             WHERE (trigger > 0) AND (hbos_severity >= {}) AND (dasnorg != 'private')
             EXCEPT SELECT ipAddress FROM abuse;",
            parquet_list, self.threshold
        );

        let mut stmt = self
            .db_conn
            .prepare(&sql_command)
            .map_err(|e| {
                Error::new(
                    std::io::ErrorKind::Other,
                    format!("sql prepare error: {}", e),
                )
            })?;
        let record_iter = stmt
            .query_map(params![], |row| {
                Ok(IpAddressRecord {
                    ipAddress: row.get(0)?,
                })
            })
            .map_err(|e| {
                Error::new(std::io::ErrorKind::Other, format!("query map error: {}", e))
            })?;

        let mut appender: Appender = self
            .db_conn
            .appender("abuse")
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        for record in record_iter {
            let record = record.map_err(|e| {
                Error::new(std::io::ErrorKind::Other, format!("record error: {}", e))
            })?;

            if (record.ipAddress.is_empty()) || self.is_private_address(&record.ipAddress) {
                continue;
            }
            println!("{}: lookup ip address [{}]", self.command, record.ipAddress);
            let status_code = self.abusedb_api_lookup(&mut appender, &record.ipAddress)?;
            if status_code == 429 {
                let now: DateTime<Utc> = Utc::now();
                self.active_date = now + Duration::hours(1);
                eprintln!(
                    "{}: reached maximum requests for AbuseIPDB API, stopping lookups until {}",
                    self.command, self.active_date
                );

                break;
            }
        }
        let _ = appender.flush();
        drop(appender);
        //
        // export records joined with flow data to parquet
        //
        println!("{}: exporting", self.command);
        let mut stmt = self
            .db_conn
            .prepare(queries::EXPORT_ABUSE_DATA)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;
        stmt.execute(params![&self.output_list[0]])
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        //
        // expunge cache of records older than 24 hours
        //
        println!("{}: updating cache", self.command);
        self.db_conn
            .execute_batch(queries::DELETE_OLD_CACHE_ENTRIES)
            .map_err(|e| {
                Error::new(
                    std::io::ErrorKind::Other,
                    format!("sql prepare error: {}", e),
                )
            })?;

        Ok(())
    }
}
impl FileProcessor for ThreatIntelProcessor {
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
        return true;
    }

    fn process(&mut self, file_list: &Vec<String>) -> Result<(), Error> {
        let parquet_list = format_parquet_list(file_list);
        // Check if the parquet files are valid
        // If not, skip processing
        // This is a performance optimization to avoid processing invalid files
        // If the files are not valid, we will not be able to read them
        // and will end up with an empty table
        if let Ok(status) = check_parquet_stream(&parquet_list) {
            if status == false {
                eprintln!(
                    "{}: invalid stream of parquet files, skipping",
                    self.command
                );
                return Ok(());
            }
        };

        if self.active_date > Utc::now() {
            eprintln!(
                "{}: currently rate limited, skipping until {}.",
                self.command, self.active_date
            );
            return Ok(());
        }
        println!("{}: processing...", self.command);
        self.abusedb_export(&parquet_list)?;
        println!("{}: done.", self.command);
        Ok(())
    }
}
