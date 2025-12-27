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
use crate::utils::common::{create_output_filenames, format_parquet_list};
use crate::utils::duckdb::duckdb_open_memory;

use maxminddb::{geoip2, Reader};
use std::fs;
use std::io::Error;
use std::net::IpAddr;
use std::path::Path;
use std::str::FromStr;

pub struct GeoipProcessor {
    pub command: String,
    pub input_list: Vec<String>,
    pub output_list: Vec<String>,
    pub pass: String,
    pub interval: Interval,
    pub extension: String,
    pub asn_db_path: String,
    pub city_db_path: String,
    pub country_db_path: String,
    asn_reader: Option<Reader<Vec<u8>>>,
    city_reader: Option<Reader<Vec<u8>>>,
    country_reader: Option<Reader<Vec<u8>>>,
}

impl GeoipProcessor {
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
        let options = parse_options(options_string);
        for (key, value) in &options {
            if !value.is_empty() {
                println!("{}: [{}=>{}]", command, key, value);
            }
        }

        let asn_db_path = options.get("asn").unwrap_or(&"").to_string();
        let city_db_path = options.get("city").unwrap_or(&"").to_string();
        let country_db_path = options.get("country").unwrap_or(&"").to_string();

        if asn_db_path.is_empty() && city_db_path.is_empty() && country_db_path.is_empty() {
            return Err(Error::new(
                std::io::ErrorKind::InvalidInput,
                "At least one database path required: --options asn=path;city=path;country=path",
            ));
        }

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
            asn_db_path,
            city_db_path,
            country_db_path,
            asn_reader: None,
            city_reader: None,
            country_reader: None,
        })
    }

    fn load_databases(&mut self) -> Result<(), Error> {
        if !self.asn_db_path.is_empty() && self.asn_reader.is_none() {
            if !Path::new(&self.asn_db_path).exists() {
                return Err(Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("ASN database not found: {}", self.asn_db_path),
                ));
            }
            println!("{}: loading ASN database: {}", self.command, self.asn_db_path);
            self.asn_reader = Some(
                Reader::open_readfile(&self.asn_db_path)
                    .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("Failed to open ASN db: {}", e)))?,
            );
        }

        if !self.city_db_path.is_empty() && self.city_reader.is_none() {
            if !Path::new(&self.city_db_path).exists() {
                return Err(Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("City database not found: {}", self.city_db_path),
                ));
            }
            println!("{}: loading city database: {}", self.command, self.city_db_path);
            self.city_reader = Some(
                Reader::open_readfile(&self.city_db_path)
                    .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("Failed to open city db: {}", e)))?,
            );
        }

        if !self.country_db_path.is_empty() && self.country_reader.is_none() {
            if !Path::new(&self.country_db_path).exists() {
                return Err(Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Country database not found: {}", self.country_db_path),
                ));
            }
            println!("{}: loading country database: {}", self.command, self.country_db_path);
            self.country_reader = Some(
                Reader::open_readfile(&self.country_db_path)
                    .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("Failed to open country db: {}", e)))?,
            );
        }

        Ok(())
    }

    fn is_private_address(ip_str: &str) -> bool {
        if let Ok(ip) = IpAddr::from_str(ip_str) {
            match ip {
                IpAddr::V4(ipv4) => {
                    ipv4.is_private()
                        || ipv4.is_loopback()
                        || ipv4.is_link_local()
                        || ipv4.is_broadcast()
                        || ipv4.is_multicast()
                        || ipv4.is_unspecified()
                }
                IpAddr::V6(ipv6) => {
                    ipv6.is_loopback()
                        || ipv6.is_multicast()
                        || ipv6.is_unspecified()
                }
            }
        } else {
            true // Treat invalid IPs as private (don't lookup)
        }
    }

    fn lookup_asn(&self, ip_str: &str) -> (u32, String) {
        if Self::is_private_address(ip_str) {
            return (0, String::new());
        }

        if let Some(ref reader) = self.asn_reader {
            if let Ok(ip) = IpAddr::from_str(ip_str) {
                if let Ok(asn) = reader.lookup::<geoip2::Asn>(ip) {
                    let asn_number = asn.autonomous_system_number.unwrap_or(0);
                    let asn_org = asn
                        .autonomous_system_organization
                        .unwrap_or("")
                        .to_lowercase();
                    return (asn_number, asn_org);
                }
            }
        }
        (0, String::new())
    }

    fn lookup_country(&self, ip_str: &str) -> String {
        if Self::is_private_address(ip_str) {
            return String::new();
        }

        if let Some(ref reader) = self.country_reader {
            if let Ok(ip) = IpAddr::from_str(ip_str) {
                if let Ok(country) = reader.lookup::<geoip2::Country>(ip) {
                    if let Some(c) = country.country {
                        if let Some(iso_code) = c.iso_code {
                            return iso_code.to_lowercase();
                        }
                    }
                }
            }
        }
        String::new()
    }

    fn lookup_city(&self, ip_str: &str) -> String {
        if Self::is_private_address(ip_str) {
            return String::new();
        }

        if let Some(ref reader) = self.city_reader {
            if let Ok(ip) = IpAddr::from_str(ip_str) {
                if let Ok(city) = reader.lookup::<geoip2::City>(ip) {
                    if let Some(c) = city.city {
                        if let Some(names) = c.names {
                            if let Some(name) = names.get("en") {
                                return name.to_lowercase();
                            }
                        }
                    }
                }
            }
        }
        String::new()
    }
}

impl FileProcessor for GeoipProcessor {
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

        // Load databases if not already loaded
        self.load_databases()?;

        let (tmp_parquet_filename, parquet_filename) =
            create_output_filenames(&self.output_list[0], &self.command);

        let db_in = duckdb_open_memory(2)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        // Load the parquet files into an in-memory table
        let sql_command = format!(
            "CREATE TABLE flow AS SELECT * FROM read_parquet({});",
            parquet_list
        );
        db_in
            .execute_batch(&sql_command)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        // Get all distinct IP addresses that need lookup
        let mut stmt = db_in
            .prepare("SELECT DISTINCT saddr FROM flow UNION SELECT DISTINCT daddr FROM flow;")
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        let ip_iter = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        // Build lookup cache
        let mut asn_cache: std::collections::HashMap<String, (u32, String)> =
            std::collections::HashMap::new();
        let mut country_cache: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut city_cache: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        for ip_result in ip_iter {
            if let Ok(ip) = ip_result {
                if !Self::is_private_address(&ip) {
                    if self.asn_reader.is_some() && !asn_cache.contains_key(&ip) {
                        asn_cache.insert(ip.clone(), self.lookup_asn(&ip));
                    }
                    if self.country_reader.is_some() && !country_cache.contains_key(&ip) {
                        country_cache.insert(ip.clone(), self.lookup_country(&ip));
                    }
                    if self.city_reader.is_some() && !city_cache.contains_key(&ip) {
                        city_cache.insert(ip.clone(), self.lookup_city(&ip));
                    }
                }
            }
        }

        println!(
            "{}: looked up {} unique IP addresses",
            self.command,
            asn_cache.len().max(country_cache.len()).max(city_cache.len())
        );

        // Update source IP fields
        for (ip, (asn, asnorg)) in &asn_cache {
            if *asn > 0 {
                let sql = format!(
                    "UPDATE flow SET sasn = {}, sasnorg = '{}' WHERE saddr = '{}';",
                    asn,
                    asnorg.replace("'", "''"),
                    ip
                );
                let _ = db_in.execute_batch(&sql);

                let sql = format!(
                    "UPDATE flow SET dasn = {}, dasnorg = '{}' WHERE daddr = '{}';",
                    asn,
                    asnorg.replace("'", "''"),
                    ip
                );
                let _ = db_in.execute_batch(&sql);
            }
        }

        for (ip, country) in &country_cache {
            if !country.is_empty() {
                let sql = format!(
                    "UPDATE flow SET scountry = '{}' WHERE saddr = '{}';",
                    country.replace("'", "''"),
                    ip
                );
                let _ = db_in.execute_batch(&sql);

                let sql = format!(
                    "UPDATE flow SET dcountry = '{}' WHERE daddr = '{}';",
                    country.replace("'", "''"),
                    ip
                );
                let _ = db_in.execute_batch(&sql);
            }
        }

        for (ip, city) in &city_cache {
            if !city.is_empty() {
                let sql = format!(
                    "UPDATE flow SET scity = '{}' WHERE saddr = '{}';",
                    city.replace("'", "''"),
                    ip
                );
                let _ = db_in.execute_batch(&sql);

                let sql = format!(
                    "UPDATE flow SET dcity = '{}' WHERE daddr = '{}';",
                    city.replace("'", "''"),
                    ip
                );
                let _ = db_in.execute_batch(&sql);
            }
        }

        // Export the updated table
        let sql_command = format!(
            "COPY flow TO '{}' (FORMAT 'parquet');",
            tmp_parquet_filename
        );
        db_in
            .execute_batch(&sql_command)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        fs::rename(&tmp_parquet_filename, &parquet_filename).map_err(|e| {
            Error::new(
                std::io::ErrorKind::Other,
                format!("renaming temporary file error: {}", e),
            )
        })?;

        Ok(())
    }
}
