/*
 * Galileo Network Analytics (GNA) Toolkit
 *
 * Copyright 2024-2025 Fidelis Farm & Technologies, LLC
 * All Rights Reserved.
 * See license information in LICENSE.
 */

use std::io;
use std::fmt;

#[derive(Debug)]
pub enum GnatError {
    Database(duckdb::Error),
    Io(io::Error),
    Config(String),
    ModelNotFound(String),
    Parse(String),
    Network(String),
    Serialization(String),
    InvalidInput(String),
}

impl fmt::Display for GnatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GnatError::Database(e) => write!(f, "Database error: {}", e),
            GnatError::Io(e) => write!(f, "IO error: {}", e),
            GnatError::Config(msg) => write!(f, "Configuration error: {}", msg),
            GnatError::ModelNotFound(path) => write!(f, "Model not found: {}", path),
            GnatError::Parse(msg) => write!(f, "Parse error: {}", msg),
            GnatError::Network(msg) => write!(f, "Network error: {}", msg),
            GnatError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            GnatError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
        }
    }
}

impl std::error::Error for GnatError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            GnatError::Database(e) => Some(e),
            GnatError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<duckdb::Error> for GnatError {
    fn from(err: duckdb::Error) -> Self {
        GnatError::Database(err)
    }
}

impl From<io::Error> for GnatError {
    fn from(err: io::Error) -> Self {
        GnatError::Io(err)
    }
}

impl From<serde_json::Error> for GnatError {
    fn from(err: serde_json::Error) -> Self {
        GnatError::Serialization(format!("JSON error: {}", err))
    }
}

impl From<reqwest::Error> for GnatError {
    fn from(err: reqwest::Error) -> Self {
        GnatError::Network(format!("HTTP error: {}", err))
    }
}

// Helper to convert GnatError to io::Error for compatibility with existing code
impl From<GnatError> for io::Error {
    fn from(err: GnatError) -> Self {
        match err {
            GnatError::Io(e) => e,
            other => io::Error::new(io::ErrorKind::Other, other.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, GnatError>;
