/*
 * Galileo Network Analytics (GNA) Toolkit
 *
 * Copyright 2024-2025 Fidelis Farm & Technologies, LLC
 * All Rights Reserved.
 * See license information in LICENSE.
 */

use clap::Parser;
use gnat::pipeline::iforest::IForestProcessor;
use gnat::pipeline::FileProcessor;
use std::error::Error;

/// Extended Isolation Forest anomaly detection for network flows
///
/// This tool uses Extended Isolation Forest (EIF) to detect anomalies in network flow data.
/// Unlike standard Isolation Forest which uses axis-parallel splits, EIF uses random
/// hyperplanes, making it more effective for detecting anomalies in datasets with
/// complex patterns.
///
/// Example usage:
///   gnat_iforest --input /data/flows --output /data/scored --options "model=/models/iforest.db"
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// Input directory containing parquet flow files
    #[arg(long)]
    input: String,

    /// Output directory for scored parquet files
    #[arg(long)]
    output: String,

    /// Optional pass-through directory for processed files
    #[arg(long)]
    pass: Option<String>,

    /// Options in format "key1=value1;key2=value2"
    /// Required: model=<path to model file>
    /// Optional: proto=tcp,udp (default: udp,tcp)
    #[arg(long)]
    options: Option<String>,

    /// Processing interval: once, second, minute, hour, day, week
    #[arg(long)]
    interval: Option<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let mut iforest_processor = IForestProcessor::new(
        "iforest",
        &args.input,
        &args.output,
        &args.pass.clone().unwrap_or(String::new()),
        &args.interval.clone().unwrap_or(String::from("second")),
        ".parquet",
        &args.options.clone().unwrap_or(String::new()),
    )?;

    iforest_processor.run()?;

    Ok(())
}
