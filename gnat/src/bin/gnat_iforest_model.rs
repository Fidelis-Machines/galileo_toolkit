/*
 * Galileo Network Analytics (GNA) Toolkit
 *
 * Copyright 2024-2025 Fidelis Farm & Technologies, LLC
 * All Rights Reserved.
 * See license information in LICENSE.
 */

use clap::Parser;
use gnat::pipeline::iforest_model::IForestModelProcessor;
use gnat::pipeline::FileProcessor;
use std::error::Error;

/// Extended Isolation Forest model training for network flow anomaly detection
///
/// This tool trains Extended Isolation Forest (EIF) models from network flow data.
/// The trained models can then be used with gnat_iforest for scoring.
///
/// Example usage:
///   gnat_iforest_model --input /data/samples --output /models/iforest.db
///   gnat_iforest_model --input /data/samples --output /models/iforest.db \
///     --options "features=dur,rtt,pcr,spkts,dpkts;trees=100;subsample=256;extension=1"
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// Input directory containing parquet flow files for training
    #[arg(long)]
    input: String,

    /// Output path for the trained model file (DuckDB database)
    #[arg(long)]
    output: String,

    /// Optional pass-through directory for processed files
    #[arg(long)]
    pass: Option<String>,

    /// Options in format "key1=value1;key2=value2"
    ///
    /// Available options:
    ///   features=<comma-separated list> - Features to use for training
    ///     Default: dur,rtt,pcr,spkts,dpkts,sbytes,dbytes,sentropy,dentropy,siat,diat
    ///   proto=<comma-separated list> - Protocols to model (default: udp,tcp)
    ///   trees=<number> - Number of trees in the forest (default: 100)
    ///   subsample=<number> - Subsample size for each tree (default: 256)
    ///   extension=<number> - Extension level (0=standard IF, 1+=EIF, default: 1)
    #[arg(long)]
    options: Option<String>,

    /// Processing interval: once, second, minute, hour, day, week
    #[arg(long)]
    interval: Option<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let mut iforest_model_processor = IForestModelProcessor::new(
        "iforest_model",
        &args.input,
        &args.output,
        &args.pass.clone().unwrap_or(String::new()),
        &args.interval.clone().unwrap_or(String::from("hour")),
        ".parquet",
        &args.options.clone().unwrap_or(String::new()),
    )?;

    iforest_model_processor.run()?;

    Ok(())
}
