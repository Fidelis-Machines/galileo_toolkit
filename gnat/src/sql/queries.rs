/*
 * Galileo Network Analytics (GNA) Toolkit
 *
 * Copyright 2024-2025 Fidelis Farm & Technologies, LLC
 * All Rights Reserved.
 * See license information in LICENSE.
 */

// HBOS queries
pub const SELECT_HBOS_SUMMARY: &str =
    "SELECT * FROM hbos_summary WHERE observe=? AND vlan=? AND proto=?;";

pub const MODEL_DISTINCT_OBSERVATIONS: &str =
    "SELECT DISTINCT observe, vlan, proto FROM histogram_summary WHERE 1=1 GROUP BY ALL ORDER BY ALL;";

pub const PARQUET_DISTINCT_OBSERVATIONS: &str =
    "SELECT DISTINCT observe, dvlan AS vlan, proto FROM flow WHERE 1=1 GROUP BY ALL ORDER BY ALL;";

pub const SELECT_DISTINCT_HISTOGRAM_FEATURES: &str =
    "SELECT DISTINCT name FROM histogram_summary;";

pub const SELECT_DISTINCT_OBSERVE_HISTOGRAMS: &str =
    "SELECT DISTINCT observe, vlan, proto, name, histogram FROM histogram_summary
     WHERE observe=? AND vlan=? AND proto=? GROUP BY ALL ORDER BY ALL;";

// Score table operations
pub const CREATE_SCORE_TABLE: &str =
    "CREATE TABLE score_table (id UUID, hbos_score DOUBLE, hbos_severity UTINYINT);";

pub const CREATE_HBOS_MAP_TABLE: &str =
    "CREATE TABLE hbos_map_table (id UUID, risk_list VARCHAR[], hbos_map MAP(VARCHAR, DOUBLE));";

pub const INSERT_SCORE: &str =
    "INSERT INTO score_table (id, hbos_score, hbos_severity) VALUES (?, ?, ?);";

// Flow table updates
pub const UPDATE_FLOW_WITH_SCORES: &str =
    "UPDATE flow
     SET hbos_score = score_table.hbos_score, hbos_severity = score_table.hbos_severity
     FROM score_table WHERE flow.id = score_table.id;
     UPDATE flow
     SET hbos_map = hbos_map_table.hbos_map, ndpi_risk_list = hbos_map_table.risk_list
     FROM hbos_map_table WHERE flow.id = hbos_map_table.id;";

// Stream validation
pub const COUNT_DISTINCT_STREAMS: &str =
    "SELECT count(DISTINCT stream) FROM read_parquet(?);";

// Trigger counting
pub const COUNT_TRIGGERS: &str =
    "SELECT count() FROM flow WHERE trigger > 0;";

pub const COUNT_TRIGGERS_BY_SEVERITY: &str =
    "SELECT count() FROM flow WHERE trigger > 0 AND hbos_severity >= ? AND hbos_severity < ?;";

// HBOS summary creation
pub const HBOS_SUMMARY_TABLE: &str =
    "CREATE TABLE IF NOT EXISTS hbos_summary (
        observe VARCHAR,
        vlan BIGINT,
        proto VARCHAR,
        min DOUBLE,
        max DOUBLE,
        skewness DOUBLE,
        avg DOUBLE,
        std DOUBLE,
        mad DOUBLE,
        median DOUBLE,
        quantile DOUBLE,
        low DOUBLE,
        medium DOUBLE,
        high DOUBLE,
        severe DOUBLE,
        critical DOUBLE
    );";

pub const HBOS_SCORE_TABLE: &str =
    "CREATE TABLE IF NOT EXISTS hbos_score (score DOUBLE);";

// Model days check
pub const SELECT_DATASET_DAYS: &str =
    "SELECT date_diff('day', first, last) + 1 AS days
     FROM (SELECT MIN(stime) AS first, MAX(stime) AS last FROM read_parquet(?));";

// Reputation DB queries
pub const CREATE_REPUTATION_TABLE: &str =
    "CREATE TABLE IF NOT EXISTS reputation (
        ipAddress VARCHAR,
        isPublic BOOLEAN,
        ipVersion SMALLINT,
        isWhitelist BOOLEAN,
        abuseConfidence SMALLINT,
        countryCode VARCHAR,
        usageType VARCHAR,
        isp VARCHAR,
        domain VARCHAR,
        hostnames VARCHAR,
        isTorExitNode BOOLEAN,
        totalReports INTEGER,
        numDistinctUsers INTEGER,
        lastReportedAt VARCHAR,
        observationPoint VARCHAR,
        cachedAt TIMESTAMP
    );";

pub const SELECT_IPS_NOT_IN_CACHE: &str =
    "SELECT daddr AS ipAddress, observe AS observationPoint FROM read_parquet(?)
     WHERE (trigger > 0) AND (hbos_severity >= ?) AND (dasnorg != 'private')
     AND (daddr, observe) NOT IN (SELECT ipAddress, observationPoint FROM reputation)
     GROUP BY ALL;";

pub const DELETE_OLD_CACHE_ENTRIES: &str =
    "DELETE FROM reputation WHERE cachedAt < NOW() - INTERVAL 24 HOUR;";

pub const EXPORT_REPUTATION_DATA: &str =
    "COPY (SELECT *, year(cachedAt) AS year, month(cachedAt) AS month, day(cachedAt) AS day FROM reputation)
     TO ? (PARTITION_BY (year, month, day), FORMAT 'parquet', OVERWRITE_OR_IGNORE TRUE);";

// IForest (Extended Isolation Forest) queries
pub const IFOREST_DISTINCT_OBSERVATIONS: &str =
    "SELECT DISTINCT observe, vlan, proto FROM iforest_summary WHERE 1=1 GROUP BY ALL ORDER BY ALL;";

pub const SELECT_IFOREST_SUMMARY: &str =
    "SELECT * FROM iforest_summary WHERE observe=? AND vlan=? AND proto=?;";

pub const IFOREST_SUMMARY_TABLE: &str =
    "CREATE TABLE IF NOT EXISTS iforest_summary (
        observe VARCHAR,
        vlan BIGINT,
        proto VARCHAR,
        num_trees INTEGER,
        subsample_size INTEGER,
        extension_level INTEGER,
        num_features INTEGER,
        feature_names VARCHAR,
        min DOUBLE,
        max DOUBLE,
        skewness DOUBLE,
        avg DOUBLE,
        std DOUBLE,
        mad DOUBLE,
        median DOUBLE,
        quantile DOUBLE,
        low DOUBLE,
        medium DOUBLE,
        high DOUBLE,
        severe DOUBLE,
        critical DOUBLE
    );";

pub const IFOREST_TREES_TABLE: &str =
    "CREATE TABLE IF NOT EXISTS iforest_trees (
        observe VARCHAR,
        vlan BIGINT,
        proto VARCHAR,
        tree_index INTEGER,
        tree_data BLOB
    );";

pub const IFOREST_SCORE_TABLE: &str =
    "CREATE TABLE IF NOT EXISTS iforest_score (score DOUBLE);";
