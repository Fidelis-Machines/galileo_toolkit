//! Tests for AggregationProcessor in pipeline::intel

use super::*;
use crate::pipeline::{Interval, FileType};

#[test]
fn test_intel_processor_new_and_process() {
    let command = "intelcmd";
    let input = "input";
    let output = "output";
    let pass = "pass";
    let interval_string = "1h";
    let extension_string = "parquet";
    let options_string = "";

    // Should construct without error
    let mut intel = AggregationProcessor::new(
        command,
        input,
        output,
        pass,
        interval_string,
        extension_string,
        options_string,
    ).expect("AggregationProcessor::new should succeed");

    // Test process with empty file list and dummy FileType
    let file_list = vec![];
    let schema_type = FileType::V1;
    let result = intel.process(&file_list, schema_type);
    assert!(result.is_ok(), "process should succeed on empty file list");
}
