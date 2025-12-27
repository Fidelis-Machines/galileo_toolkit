/*
 * Galileo Network Analytics (GNA) Toolkit
 *
 * Copyright 2024-2025 Fidelis Farm & Technologies, LLC
 * All Rights Reserved.
 * See license information in LICENSE.
 */

//! Extended Isolation Forest Implementation
//!
//! This module implements the Extended Isolation Forest (EIF) algorithm for anomaly detection.
//! Unlike the standard Isolation Forest which uses axis-parallel splits, EIF uses random
//! hyperplanes for splitting, making it more effective for detecting anomalies in datasets
//! with non-axis-aligned patterns.
//!
//! Reference: Hariri, S., Kind, M. C., & Brunner, R. J. (2019). Extended Isolation Forest.
//! IEEE Transactions on Knowledge and Data Engineering.

use crate::model::table::MemFlowRecord;
use crate::utils::duckdb::duckdb_open;
use chrono::{DateTime, Utc};
use duckdb::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Error;
use std::path::Path;
use std::path::PathBuf;

/// Default number of trees in the forest
pub const DEFAULT_NUM_TREES: usize = 100;

/// Default subsample size for building each tree
pub const DEFAULT_SUBSAMPLE_SIZE: usize = 256;

/// Extension level for EIF (0 = standard IF, higher = more extension)
pub const DEFAULT_EXTENSION_LEVEL: usize = 1;

/// Euler-Mascheroni constant used in path length calculation
const EULER_MASCHERONI: f64 = 0.5772156649;

/// SQL to create iforest_summary table
pub static IFOREST_SUMMARY: &str = "CREATE TABLE IF NOT EXISTS iforest_summary
(
    observe VARCHAR,
    vlan INTEGER,
    proto VARCHAR,
    num_trees INTEGER,
    subsample_size INTEGER,
    extension_level INTEGER,
    num_features INTEGER,
    feature_names VARCHAR,
    min FLOAT,
    max FLOAT,
    skewness FLOAT,
    avg FLOAT,
    stdev FLOAT,
    mad FLOAT,
    median FLOAT,
    quantile FLOAT,
    low FLOAT,
    medium FLOAT,
    high FLOAT,
    severe FLOAT,
    critical FLOAT
);";

/// SQL to create iforest_trees table for storing serialized trees
pub static IFOREST_TREES: &str = "CREATE TABLE IF NOT EXISTS iforest_trees
(
    observe VARCHAR,
    vlan INTEGER,
    proto VARCHAR,
    tree_index INTEGER,
    tree_data BLOB
);";

/// SQL to create iforest_score table for scoring
pub static IFOREST_SCORE: &str = "CREATE TABLE IF NOT EXISTS iforest_score
(
    score FLOAT
)";

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

#[allow(dead_code)]
enum Severity {
    None = 0,
    Low = 1,
    Medium = 2,
    High = 3,
    Severe = 4,
    Critical = 5,
    Emergency = 6,
}

/// A node in an Extended Isolation Tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EITreeNode {
    /// Internal node with a random hyperplane split
    Internal {
        /// Normal vector defining the hyperplane (random direction)
        normal: Vec<f64>,
        /// Intercept point on the hyperplane
        intercept: f64,
        /// Left child (points on negative side of hyperplane)
        left: Box<EITreeNode>,
        /// Right child (points on positive side of hyperplane)
        right: Box<EITreeNode>,
    },
    /// External (leaf) node
    External {
        /// Number of samples that reached this leaf
        size: usize,
    },
}

impl EITreeNode {
    /// Create a new external (leaf) node
    fn external(size: usize) -> Self {
        EITreeNode::External { size }
    }

    /// Create a new internal node
    fn internal(normal: Vec<f64>, intercept: f64, left: EITreeNode, right: EITreeNode) -> Self {
        EITreeNode::Internal {
            normal,
            intercept,
            left: Box::new(left),
            right: Box::new(right),
        }
    }
}

/// Extended Isolation Tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedIsolationTree {
    /// Root node of the tree
    root: EITreeNode,
    /// Maximum depth limit
    height_limit: usize,
    /// Extension level (0 = standard IF, 1+ = EIF)
    extension_level: usize,
}

impl ExtendedIsolationTree {
    /// Build a new Extended Isolation Tree from data
    pub fn new(
        data: &[Vec<f64>],
        height_limit: usize,
        extension_level: usize,
        rng: &mut SimpleRng,
    ) -> Self {
        let root = Self::build_tree(data, 0, height_limit, extension_level, rng);
        ExtendedIsolationTree {
            root,
            height_limit,
            extension_level,
        }
    }

    /// Recursively build the tree
    fn build_tree(
        data: &[Vec<f64>],
        current_depth: usize,
        height_limit: usize,
        extension_level: usize,
        rng: &mut SimpleRng,
    ) -> EITreeNode {
        let n = data.len();

        // Base case: create external node
        if current_depth >= height_limit || n <= 1 {
            return EITreeNode::external(n);
        }

        let num_features = if data.is_empty() { 0 } else { data[0].len() };
        if num_features == 0 {
            return EITreeNode::external(n);
        }

        // Generate random normal vector for the hyperplane
        let normal = Self::generate_random_normal(num_features, extension_level, rng);

        // Find min and max projections onto the normal
        let mut min_proj = f64::INFINITY;
        let mut max_proj = f64::NEG_INFINITY;

        for point in data {
            let proj = Self::dot_product(&normal, point);
            if proj < min_proj {
                min_proj = proj;
            }
            if proj > max_proj {
                max_proj = proj;
            }
        }

        // If all points project to the same value, create external node
        if (max_proj - min_proj).abs() < f64::EPSILON {
            return EITreeNode::external(n);
        }

        // Random intercept between min and max projections
        let intercept = min_proj + rng.next_f64() * (max_proj - min_proj);

        // Split data based on the hyperplane
        let mut left_data = Vec::new();
        let mut right_data = Vec::new();

        for point in data {
            let proj = Self::dot_product(&normal, point);
            if proj < intercept {
                left_data.push(point.clone());
            } else {
                right_data.push(point.clone());
            }
        }

        // Recursively build children
        let left = Self::build_tree(&left_data, current_depth + 1, height_limit, extension_level, rng);
        let right = Self::build_tree(&right_data, current_depth + 1, height_limit, extension_level, rng);

        EITreeNode::internal(normal, intercept, left, right)
    }

    /// Generate a random normal vector
    /// For extension_level = 0: axis-aligned (standard IF)
    /// For extension_level > 0: random direction in the subspace
    fn generate_random_normal(
        num_features: usize,
        extension_level: usize,
        rng: &mut SimpleRng,
    ) -> Vec<f64> {
        let mut normal = vec![0.0; num_features];

        if extension_level == 0 {
            // Standard Isolation Forest: pick a single random axis
            let idx = rng.next_usize() % num_features;
            normal[idx] = 1.0;
        } else {
            // Extended Isolation Forest: random direction
            // The number of non-zero components is num_features - extension_level + 1
            // but capped to be at least 2
            let active_dims = std::cmp::max(2, num_features.saturating_sub(extension_level) + 1);
            let active_dims = std::cmp::min(active_dims, num_features);

            // Randomly select which dimensions to use
            let mut indices: Vec<usize> = (0..num_features).collect();
            // Fisher-Yates shuffle for first active_dims elements
            for i in 0..active_dims {
                let j = i + (rng.next_usize() % (num_features - i));
                indices.swap(i, j);
            }

            // Generate random values for active dimensions
            let mut sum_sq = 0.0;
            for i in 0..active_dims {
                // Use Box-Muller transform for Gaussian random numbers
                let u1 = rng.next_f64().max(f64::MIN_POSITIVE);
                let u2 = rng.next_f64();
                let gaussian = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
                normal[indices[i]] = gaussian;
                sum_sq += gaussian * gaussian;
            }

            // Normalize the vector
            let norm = sum_sq.sqrt();
            if norm > f64::EPSILON {
                for i in 0..active_dims {
                    normal[indices[i]] /= norm;
                }
            }
        }

        normal
    }

    /// Compute dot product of two vectors
    fn dot_product(a: &[f64], b: &[f64]) -> f64 {
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }

    /// Compute the path length for a single point
    pub fn path_length(&self, point: &[f64]) -> f64 {
        Self::path_length_recursive(&self.root, point, 0)
    }

    /// Recursively compute path length
    fn path_length_recursive(node: &EITreeNode, point: &[f64], current_depth: usize) -> f64 {
        match node {
            EITreeNode::External { size } => {
                current_depth as f64 + c_factor(*size)
            }
            EITreeNode::Internal {
                normal,
                intercept,
                left,
                right,
            } => {
                let proj = Self::dot_product(normal, point);
                if proj < *intercept {
                    Self::path_length_recursive(left, point, current_depth + 1)
                } else {
                    Self::path_length_recursive(right, point, current_depth + 1)
                }
            }
        }
    }
}

/// Extended Isolation Forest ensemble
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedIsolationForest {
    /// Collection of trees
    trees: Vec<ExtendedIsolationTree>,
    /// Number of samples used for training
    subsample_size: usize,
    /// Feature names for reference
    feature_names: Vec<String>,
    /// Extension level
    extension_level: usize,
}

impl ExtendedIsolationForest {
    /// Create a new Extended Isolation Forest
    pub fn new(
        data: &[Vec<f64>],
        num_trees: usize,
        subsample_size: usize,
        extension_level: usize,
        feature_names: Vec<String>,
        seed: u64,
    ) -> Self {
        let mut rng = SimpleRng::new(seed);
        let height_limit = (subsample_size as f64).log2().ceil() as usize;
        let sample_size = std::cmp::min(subsample_size, data.len());

        let mut trees = Vec::with_capacity(num_trees);

        for _ in 0..num_trees {
            // Subsample the data
            let sample = Self::subsample(data, sample_size, &mut rng);
            // Build tree
            let tree = ExtendedIsolationTree::new(&sample, height_limit, extension_level, &mut rng);
            trees.push(tree);
        }

        ExtendedIsolationForest {
            trees,
            subsample_size: sample_size,
            feature_names,
            extension_level,
        }
    }

    /// Randomly subsample data
    fn subsample(data: &[Vec<f64>], size: usize, rng: &mut SimpleRng) -> Vec<Vec<f64>> {
        let n = data.len();
        if size >= n {
            return data.to_vec();
        }

        // Reservoir sampling
        let mut sample: Vec<Vec<f64>> = data.iter().take(size).cloned().collect();

        for i in size..n {
            let j = rng.next_usize() % (i + 1);
            if j < size {
                sample[j] = data[i].clone();
            }
        }

        sample
    }

    /// Compute anomaly score for a single point
    /// Returns a score between 0 and 1, where values closer to 1 indicate anomalies
    pub fn score(&self, point: &[f64]) -> f64 {
        if self.trees.is_empty() {
            return 0.5;
        }

        // Average path length across all trees
        let avg_path_length: f64 = self.trees.iter()
            .map(|tree| tree.path_length(point))
            .sum::<f64>() / self.trees.len() as f64;

        // Normalize using the expected path length for the subsample size
        let c = c_factor(self.subsample_size);

        // Anomaly score: s = 2^(-E[h(x)]/c(n))
        // Higher score = more anomalous
        2.0_f64.powf(-avg_path_length / c)
    }

    /// Compute anomaly scores for multiple points
    pub fn score_batch(&self, points: &[Vec<f64>]) -> Vec<f64> {
        points.iter().map(|p| self.score(p)).collect()
    }

    /// Get feature names
    pub fn feature_names(&self) -> &[String] {
        &self.feature_names
    }

    /// Get number of trees
    pub fn num_trees(&self) -> usize {
        self.trees.len()
    }

    /// Serialize the forest to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        serde_json::to_vec(self).map_err(|e| {
            Error::new(std::io::ErrorKind::Other, format!("Serialization error: {}", e))
        })
    }

    /// Deserialize the forest from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        serde_json::from_slice(bytes).map_err(|e| {
            Error::new(std::io::ErrorKind::Other, format!("Deserialization error: {}", e))
        })
    }
}

/// Calculate the average path length of unsuccessful search in BST
/// This is the normalization factor c(n) used in anomaly score calculation
fn c_factor(n: usize) -> f64 {
    if n <= 1 {
        return 0.0;
    }
    if n == 2 {
        return 1.0;
    }
    let n_f = n as f64;
    2.0 * (n_f.ln() + EULER_MASCHERONI) - (2.0 * (n_f - 1.0) / n_f)
}

/// Simple PRNG for reproducibility
#[derive(Debug, Clone)]
pub struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    pub fn new(seed: u64) -> Self {
        SimpleRng { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        // xorshift64
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    pub fn next_usize(&mut self) -> usize {
        self.next_u64() as usize
    }

    pub fn next_f64(&mut self) -> f64 {
        // Generate a value in [0, 1)
        (self.next_u64() as f64) / (u64::MAX as f64)
    }
}

/// Summary record for IForest model
#[derive(Debug, Clone)]
pub struct IForestSummaryRecord {
    pub observe: String,
    pub vlan: i64,
    pub proto: String,
    pub num_trees: i32,
    pub subsample_size: i32,
    pub extension_level: i32,
    pub num_features: i32,
    pub feature_names: String,
    pub min: f64,
    pub max: f64,
    pub skewness: f64,
    pub avg: f64,
    pub std: f64,
    pub mad: f64,
    pub median: f64,
    pub quantile: f64,
    pub low: f64,
    pub medium: f64,
    pub high: f64,
    pub severe: f64,
    pub critical: f64,
}

/// Container for IForest models per observation/vlan/proto
pub struct IForestModels {
    pub observe: String,
    pub vlan: i64,
    pub proto: String,
    pub forest: Option<ExtendedIsolationForest>,
    pub feature_list: Vec<String>,
    pub low: f64,
    pub medium: f64,
    pub high: f64,
    pub severe: f64,
    pub critical: f64,
}

impl IForestModels {
    /// Create a new IForestModels container
    pub fn new(observe: &str, vlan: i64, proto: &str) -> Self {
        IForestModels {
            observe: observe.to_string(),
            vlan,
            proto: proto.to_string(),
            forest: None,
            feature_list: Vec::new(),
            low: 0.0,
            medium: 0.0,
            high: 0.0,
            severe: 0.0,
            critical: 0.0,
        }
    }

    /// Serialize the model to DuckDB
    pub fn serialize(&self, conn: &mut Connection) -> Result<(), Error> {
        if let Some(ref forest) = self.forest {
            // Create tables if they don't exist
            conn.execute_batch(IFOREST_TREES).map_err(|e| {
                Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e))
            })?;

            let forest_bytes = forest.to_bytes()?;

            // Store the entire forest as a single blob
            let sql = "INSERT INTO iforest_trees (observe, vlan, proto, tree_index, tree_data) VALUES (?, ?, ?, ?, ?)";
            conn.execute(sql, params![&self.observe, &self.vlan, &self.proto, 0i32, forest_bytes])
                .map_err(|e| {
                    Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e))
                })?;
        }
        Ok(())
    }

    /// Deserialize the model from DuckDB
    pub fn deserialize(&mut self, conn: &Connection) -> Result<(), Error> {
        // Load forest data
        let sql = format!(
            "SELECT tree_data FROM iforest_trees WHERE observe='{}' AND vlan={} AND proto='{}' AND tree_index=0;",
            self.observe, self.vlan, self.proto
        );

        let mut stmt = conn.prepare(&sql).map_err(|e| {
            Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e))
        })?;

        let forest_data: Vec<u8> = stmt
            .query_row([], |row| row.get(0))
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        self.forest = Some(ExtendedIsolationForest::from_bytes(&forest_data)?);

        // Load summary data
        let sql_summary = format!(
            "SELECT * FROM iforest_summary WHERE observe='{}' AND vlan={} AND proto='{}';",
            self.observe, self.vlan, self.proto
        );

        let mut stmt = conn.prepare(&sql_summary).map_err(|e| {
            Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e))
        })?;

        let summary: IForestSummaryRecord = stmt
            .query_row([], |row| {
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
            })
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        self.low = summary.low;
        self.medium = summary.medium;
        self.high = summary.high;
        self.severe = summary.severe;
        self.critical = summary.critical;
        self.feature_list = summary.feature_names.split(',').map(|s| s.to_string()).collect();

        Ok(())
    }

    /// Score flows and update the database
    pub fn score(&self, db_connection: &mut Connection) -> Result<u64, Error> {
        let forest = match &self.forest {
            Some(f) => f,
            None => return Err(Error::new(std::io::ErrorKind::Other, "Forest not loaded")),
        };

        let mut score_appender = db_connection
            .appender("score_table")
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        let sql_command = format!(
            "SELECT * EXCLUDE(tag, hbos_map, ndpi_risk_list) FROM flow WHERE observe='{}' AND dvlan={} AND proto='{}' AND (snonemptypktcnt>0 OR dnonemptypktcnt>0);",
            self.observe, self.vlan, self.proto,
        );

        let mut stmt = db_connection
            .prepare(&sql_command)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        let record_iter = stmt
            .query_map([], |row| {
                Ok(MemFlowRecord {
                    stream: row.get(0).expect("missing value"),
                    id: row.get(1).expect("missing value"),
                    observe: row.get(2).expect("missing value"),
                    stime: row.get(3).expect("missing value"),
                    etime: row.get(4).expect("missing value"),
                    dur: row.get(5).expect("missing value"),
                    rtt: row.get(6).expect("missing value"),
                    pcr: row.get(7).expect("missing value"),
                    proto: row.get(8).expect("missing value"),
                    saddr: row.get(9).expect("missing value"),
                    daddr: row.get(10).expect("missing value"),
                    sport: row.get(11).expect("missing value"),
                    dport: row.get(12).expect("missing value"),
                    iflags: row.get(13).expect("missing value"),
                    uflags: row.get(14).expect("missing value"),
                    stcpseq: row.get(15).expect("missing value"),
                    dtcpseq: row.get(16).expect("missing value"),
                    svlan: row.get(17).expect("missing value"),
                    dvlan: row.get(18).expect("missing value"),
                    spkts: row.get(19).expect("missing value"),
                    dpkts: row.get(20).expect("missing value"),
                    sbytes: row.get(21).expect("missing value"),
                    dbytes: row.get(22).expect("missing value"),
                    sentropy: row.get(23).expect("missing value"),
                    dentropy: row.get(24).expect("missing value"),
                    siat: row.get(25).expect("missing value"),
                    diat: row.get(26).expect("missing value"),
                    sstdeviat: row.get(27).expect("missing value"),
                    dstdeviat: row.get(28).expect("missing value"),
                    dtcpurg: row.get(29).expect("missing value"),
                    stcpurg: row.get(30).expect("missing value"),
                    ssmallpktcnt: row.get(31).expect("missing value"),
                    dsmallpktcnt: row.get(32).expect("missing value"),
                    slargepktcnt: row.get(33).expect("missing value"),
                    dlargepktcnt: row.get(34).expect("missing value"),
                    snonemptypktcnt: row.get(35).expect("missing value"),
                    dnonemptypktcnt: row.get(36).expect("missing value"),
                    sfirstnonemptycnt: row.get(37).expect("missing value"),
                    dfirstnonemptycnt: row.get(38).expect("missing value"),
                    smaxpktsize: row.get(39).expect("missing value"),
                    dmaxpktsize: row.get(40).expect("missing value"),
                    sstdevpayload: row.get(41).expect("missing value"),
                    dstdevpayload: row.get(42).expect("missing value"),
                    spd: row.get(43).expect("missing value"),
                    reason: row.get(44).expect("missing value"),
                    smac: row.get(45).expect("missing value"),
                    dmac: row.get(46).expect("missing value"),
                    scountry: row.get(47).expect("missing value"),
                    dcountry: row.get(48).expect("missing value"),
                    scity: row.get(49).expect("missing value"),
                    dcity: row.get(50).expect("missing value"),
                    sasn: row.get(51).expect("missing value"),
                    dasn: row.get(52).expect("missing value"),
                    sasnorg: row.get(53).expect("missing value"),
                    dasnorg: row.get(54).expect("missing value"),
                    orient: row.get(55).expect("missing value"),
                    hbos_score: row.get(56).expect("missing value"),
                    hbos_severity: row.get(57).expect("missing value"),
                    appid: row.get(58).expect("missing value"),
                    category: row.get(59).unwrap_or("".to_string()),
                    risk_bits: row.get(60).expect("missing value"),
                    risk_score: row.get(61).expect("missing value"),
                    risk_severity: row.get(62).expect("missing value"),
                    trigger: row.get(63).expect("missing value"),
                })
            })
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        let mut count: u64 = 0;
        for record in record_iter {
            let flow_record = record.map_err(|e| {
                Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e))
            })?;

            // Extract features from the flow record
            let features = self.extract_features(&flow_record);

            // Score using the forest
            let iforest_score = forest.score(&features);

            // Determine severity based on thresholds
            let severity = if iforest_score >= self.critical {
                Severity::Critical as u8
            } else if iforest_score >= self.severe {
                Severity::Severe as u8
            } else if iforest_score >= self.high {
                Severity::High as u8
            } else if iforest_score >= self.medium {
                Severity::Medium as u8
            } else if iforest_score >= self.low {
                Severity::Low as u8
            } else {
                Severity::None as u8
            };

            score_appender
                .append_row(params![flow_record.id, iforest_score, severity])
                .map_err(|e| {
                    Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e))
                })?;

            count += 1;
        }

        score_appender
            .flush()
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;
        drop(score_appender);

        Ok(count)
    }

    /// Extract numerical features from a flow record based on feature_list
    fn extract_features(&self, record: &MemFlowRecord) -> Vec<f64> {
        let mut features = Vec::with_capacity(self.feature_list.len());

        for feature_name in &self.feature_list {
            let value = match feature_name.as_str() {
                "dur" => record.dur as f64,
                "rtt" => record.rtt as f64,
                "pcr" => record.pcr as f64,
                "sport" => record.sport as f64,
                "dport" => record.dport as f64,
                "spkts" => record.spkts as f64,
                "dpkts" => record.dpkts as f64,
                "sbytes" => record.sbytes as f64,
                "dbytes" => record.dbytes as f64,
                "sentropy" => record.sentropy as f64,
                "dentropy" => record.dentropy as f64,
                "siat" => record.siat as f64,
                "diat" => record.diat as f64,
                "sstdeviat" => record.sstdeviat as f64,
                "dstdeviat" => record.dstdeviat as f64,
                "ssmallpktcnt" => record.ssmallpktcnt as f64,
                "dsmallpktcnt" => record.dsmallpktcnt as f64,
                "slargepktcnt" => record.slargepktcnt as f64,
                "dlargepktcnt" => record.dlargepktcnt as f64,
                "snonemptypktcnt" => record.snonemptypktcnt as f64,
                "dnonemptypktcnt" => record.dnonemptypktcnt as f64,
                "sfirstnonemptycnt" => record.sfirstnonemptycnt as f64,
                "dfirstnonemptycnt" => record.dfirstnonemptycnt as f64,
                "smaxpktsize" => record.smaxpktsize as f64,
                "dmaxpktsize" => record.dmaxpktsize as f64,
                "sstdevpayload" => record.sstdevpayload as f64,
                "dstdevpayload" => record.dstdevpayload as f64,
                "sasn" => record.sasn as f64,
                "dasn" => record.dasn as f64,
                _ => 0.0, // Unknown features default to 0
            };
            features.push(value);
        }

        features
    }

    /// Build the forest model from training data
    pub fn build(
        &mut self,
        parquet_list: &str,
        feature_list: &[String],
        num_trees: usize,
        subsample_size: usize,
        extension_level: usize,
    ) -> Result<(), Error> {
        let current_utc: DateTime<Utc> = Utc::now();
        let rfc3339_name: String = current_utc.to_rfc3339();
        let db_input_file = format!("iforest_build.{}.tmp", rfc3339_name.replace(":", "-"));
        let _db_remove = DbFileRemover {
            path: db_input_file.clone().into(),
        };

        let conn = duckdb_open(&db_input_file, 2)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        // Load data from parquet
        let sql_command = format!(
            "CREATE OR REPLACE TABLE flow AS (SELECT * FROM read_parquet({})
             WHERE observe='{}' AND dvlan = {} AND proto='{}');",
            parquet_list, self.observe, self.vlan, self.proto
        );
        conn.execute_batch(&sql_command)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        // Build feature selection query
        let feature_sql = feature_list.join(", ");
        let sql_query = format!(
            "SELECT {} FROM flow WHERE snonemptypktcnt > 0 OR dnonemptypktcnt > 0;",
            feature_sql
        );

        let mut stmt = conn.prepare(&sql_query)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        // Extract training data
        let mut training_data: Vec<Vec<f64>> = Vec::new();
        let num_features = feature_list.len();

        let mut rows = stmt.query([])
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        while let Some(row) = rows.next()
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?
        {
            let mut features = Vec::with_capacity(num_features);
            for i in 0..num_features {
                // Try to get as f64, or convert from other numeric types
                let value: f64 = row.get::<_, f64>(i)
                    .or_else(|_| row.get::<_, i64>(i).map(|v| v as f64))
                    .or_else(|_| row.get::<_, i32>(i).map(|v| v as f64))
                    .or_else(|_| row.get::<_, u64>(i).map(|v| v as f64))
                    .or_else(|_| row.get::<_, u32>(i).map(|v| v as f64))
                    .unwrap_or(0.0);
                features.push(value);
            }
            training_data.push(features);
        }

        println!(
            "iforest: building model with {} samples, {} trees, subsample_size={}, extension_level={}",
            training_data.len(), num_trees, subsample_size, extension_level
        );

        if training_data.is_empty() {
            return Err(Error::new(
                std::io::ErrorKind::Other,
                "No training data available",
            ));
        }

        // Use current timestamp as seed for reproducibility with different runs
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(42);

        // Build the forest
        let forest = ExtendedIsolationForest::new(
            &training_data,
            num_trees,
            subsample_size,
            extension_level,
            feature_list.to_vec(),
            seed,
        );

        self.forest = Some(forest);
        self.feature_list = feature_list.to_vec();

        let _ = conn.close();

        Ok(())
    }

    /// Summarize the model by computing score statistics
    pub fn summarize(
        &mut self,
        parquet_list: &str,
        sink_conn: &mut Connection,
    ) -> Result<(), Error> {
        let forest = match &self.forest {
            Some(f) => f,
            None => return Err(Error::new(std::io::ErrorKind::Other, "Forest not built")),
        };

        let current_utc: DateTime<Utc> = Utc::now();
        let rfc3339_name: String = current_utc.to_rfc3339();

        let db_input_file = format!("iforest_summarize.{}.tmp", rfc3339_name.replace(":", "-"));
        let _db_remove = DbFileRemover {
            path: db_input_file.clone().into(),
        };

        let conn = duckdb_open(&db_input_file, 2)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        // Load data
        let sql_command = format!(
            "CREATE OR REPLACE TABLE flow AS (SELECT * FROM read_parquet({})
             WHERE observe='{}' AND dvlan = {} AND proto='{}');",
            parquet_list, self.observe, self.vlan, self.proto
        );
        conn.execute_batch(&sql_command)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        // Create score table
        conn.execute_batch(IFOREST_SCORE)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        let mut appender = conn.appender("iforest_score")
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        // Build feature query
        let feature_sql = self.feature_list.join(", ");
        let sql_query = format!(
            "SELECT {} FROM flow WHERE snonemptypktcnt > 0 OR dnonemptypktcnt > 0;",
            feature_sql
        );

        let mut stmt = conn.prepare(&sql_query)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        let num_features = self.feature_list.len();
        let mut rows = stmt.query([])
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        while let Some(row) = rows.next()
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?
        {
            let mut features = Vec::with_capacity(num_features);
            for i in 0..num_features {
                let value: f64 = row.get::<_, f64>(i)
                    .or_else(|_| row.get::<_, i64>(i).map(|v| v as f64))
                    .or_else(|_| row.get::<_, i32>(i).map(|v| v as f64))
                    .or_else(|_| row.get::<_, u64>(i).map(|v| v as f64))
                    .or_else(|_| row.get::<_, u32>(i).map(|v| v as f64))
                    .unwrap_or(0.0);
                features.push(value);
            }

            let score = forest.score(&features);
            appender.append_row(params![score])
                .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;
        }

        appender.flush()
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;
        drop(appender);

        // Calculate severity thresholds using histogram
        let (low, medium, high, severe, critical) = self.get_severity_levels(&conn)?;

        // Get statistics
        let mut stmt = conn
            .prepare(
                "SELECT min(score), max(score), skewness(score), avg(score), stddev_pop(score),
                     mad(score), median(score), quantile_cont(score, 0.99999) FROM iforest_score;",
            )
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        let (min, max, skewness, avg, std, mad, median, quantile): (f64, f64, f64, f64, f64, f64, f64, f64) = stmt
            .query_row([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            })
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        self.low = low;
        self.medium = medium;
        self.high = high;
        self.severe = severe;
        self.critical = critical;

        let _ = conn.close();

        // Store summary in sink connection
        sink_conn.execute_batch(IFOREST_SUMMARY)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        let feature_names_str = self.feature_list.join(",");
        let num_trees = forest.num_trees() as i32;
        let subsample_size = forest.subsample_size as i32;
        let extension_level = forest.extension_level as i32;
        let num_features = self.feature_list.len() as i32;

        let mut appender = sink_conn.appender("iforest_summary")
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        appender.append_row(params![
            &self.observe,
            &self.vlan,
            &self.proto,
            num_trees,
            subsample_size,
            extension_level,
            num_features,
            feature_names_str,
            min,
            max,
            skewness,
            avg,
            std,
            mad,
            median,
            quantile,
            low,
            medium,
            high,
            severe,
            critical,
        ]).map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        appender.flush()
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

        println!(
            "[{}/{}/{}] [{}<low,{}<medium,{}<high,{}<severe,{}<critical]",
            self.observe, self.vlan, self.proto, low, medium, high, severe, critical
        );

        Ok(())
    }

    /// Calculate severity levels from score histogram
    fn get_severity_levels(&self, conn: &Connection) -> Result<(f64, f64, f64, f64, f64), Error> {
        // Print histogram for debugging
        {
            let mut stmt = conn
                .prepare("FROM histogram(iforest_score, score);")
                .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

            let hist_iter = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0).expect("missing bin"),
                        row.get::<_, f64>(1).expect("missing boundary"),
                        row.get::<_, String>(2).expect("missing bar"),
                    ))
                })
                .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

            for result in hist_iter {
                let (bin, boundary, bar) = result.map_err(|e| {
                    Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e))
                })?;
                println!(
                    "[{}/{}/{}]\t[{} ({:.4})]: {}",
                    self.observe, self.vlan, self.proto, bin, boundary, bar
                );
            }
        }

        // Get histogram values for thresholds
        let mut low = 0.0;
        let mut medium = 0.0;
        let mut high = 0.0;
        let mut severe = 0.0;
        let mut critical = 0.0;

        {
            let mut stmt = conn
                .prepare("FROM histogram_values(iforest_score, score);")
                .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

            let hist_iter = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, f64>(0).expect("missing boundary"),
                        row.get::<_, usize>(1).expect("missing frequency"),
                    ))
                })
                .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e)))?;

            for result in hist_iter {
                let (boundary, _frequency) = result.map_err(|e| {
                    Error::new(std::io::ErrorKind::Other, format!("DuckDB error: {}", e))
                })?;
                low = medium;
                medium = high;
                high = severe;
                severe = critical;
                critical = boundary;
            }
        }

        Ok((low, medium, high, severe, critical))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_rng() {
        let mut rng = SimpleRng::new(42);
        let v1 = rng.next_u64();
        let v2 = rng.next_u64();
        assert_ne!(v1, v2);

        let f = rng.next_f64();
        assert!(f >= 0.0 && f < 1.0);
    }

    #[test]
    fn test_c_factor() {
        assert_eq!(c_factor(1), 0.0);
        assert_eq!(c_factor(2), 1.0);
        assert!(c_factor(256) > 0.0);
    }

    #[test]
    fn test_eif_basic() {
        // Create some test data
        let data: Vec<Vec<f64>> = vec![
            vec![1.0, 2.0],
            vec![1.1, 2.1],
            vec![0.9, 1.9],
            vec![1.0, 2.0],
            vec![1.2, 2.2],
            vec![10.0, 10.0], // anomaly
        ];

        let forest = ExtendedIsolationForest::new(
            &data,
            100,
            256,
            1,
            vec!["f1".to_string(), "f2".to_string()],
            42,
        );

        let normal_score = forest.score(&[1.0, 2.0]);
        let anomaly_score = forest.score(&[10.0, 10.0]);

        // Anomaly should have higher score
        assert!(anomaly_score > normal_score);
    }
}
