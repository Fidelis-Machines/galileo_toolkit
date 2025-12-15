# Flow Rule Configuration

This document describes the syntax for the JSON configuration file used to define flow rules in the Galileo Network Analytics (GNA) Toolkit.

## Overview

Flow rules allow you to mark network flows for triggering or ignoring based on matching criteria. When a flow matches the specified conditions, the flow's `trigger` field is set to either `1` (trigger) or `-1` (ignore). This is useful for alerting, filtering, or prioritizing flows based on network security policies.

## Configuration File Format

The configuration file is a JSON array containing one or more rule objects.

```json
[
  {
    "action": "trigger",
    "observe": "observation_domain",
    "proto": "protocol",
    "saddr": "source_address",
    "sport": 443,
    "daddr": "destination_address",
    "dport": 80,
    "appid": "application_id",
    "orient": "flow_orientation",
    "tag": "tag_name",
    "risk_severity": 3,
    "hbos_severity": 2
  }
]
```

## Field Descriptions

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `action` | string | **Yes** | - | Action to take: `"trigger"` (set trigger=1) or `"ignore"` (set trigger=-1) |
| `observe` | string | No | `""` | Observation domain prefix to match |
| `proto` | string | No | `""` | Protocol to match (exact match) |
| `saddr` | string | No | `""` | Source IP address prefix to match |
| `sport` | integer | No | `0` | Source port to match (exact match, 0 means any) |
| `daddr` | string | No | `""` | Destination IP address prefix to match |
| `dport` | integer | No | `0` | Destination port to match (exact match, 0 means any) |
| `appid` | string | No | `""` | nDPI application ID prefix to match |
| `orient` | string | No | `""` | Flow orientation prefix (e.g., "00", "01", "10", "11") |
| `tag` | string | No | `""` | Tag/label to match (checks if flow has this tag) |
| `risk_severity` | integer | No | `0` | Minimum nDPI risk severity to match (0-255, 0 means any) |
| `hbos_severity` | integer | No | `0` | Minimum HBOS severity to match (0-255, 0 means any) |

## Matching Behavior

- **Required field**: Only `action` is required. All other fields are optional.
- **Empty/zero fields**: Fields with empty strings (`""`) or zero values are ignored in matching.
- **Prefix matching**: The fields `observe`, `saddr`, `daddr`, `appid`, and `orient` use **prefix matching** (the flow value must start with the specified string).
- **Exact matching**: The fields `proto`, `sport`, and `dport` use **exact matching**.
- **Threshold matching**: The fields `risk_severity` and `hbos_severity` use **greater-than-or-equal matching**.
- **Tag matching**: The `tag` field checks if the specified tag exists in the flow's tag list.
- **Multiple conditions**: When multiple fields are specified, ALL conditions must match (AND logic).

## Actions

| Action | Trigger Value | Description |
|--------|---------------|-------------|
| `trigger` | `1` | Mark flow for alerting/investigation |
| `ignore` | `-1` | Mark flow to be ignored/suppressed |

## Examples

### Example 1: Trigger on High-Risk Traffic

Trigger on flows with high nDPI risk severity:

```json
[
  {
    "action": "trigger",
    "risk_severity": 3
  }
]
```

### Example 2: Ignore Internal Traffic

Ignore traffic from internal networks:

```json
[
  {
    "action": "ignore",
    "saddr": "10."
  },
  {
    "action": "ignore",
    "saddr": "192.168."
  },
  {
    "action": "ignore",
    "saddr": "172.16."
  }
]
```

### Example 3: Trigger on Anomalous Behavior

Trigger on flows with HBOS anomaly scores indicating severe anomalies:

```json
[
  {
    "action": "trigger",
    "hbos_severity": 4
  }
]
```

### Example 4: Trigger on Suspicious SSH Traffic

Trigger on SSH traffic from external sources:

```json
[
  {
    "action": "trigger",
    "proto": "TCP",
    "dport": 22,
    "appid": "SSH"
  }
]
```

### Example 5: Ignore Known Good Traffic

Ignore DNS traffic to trusted resolvers:

```json
[
  {
    "action": "ignore",
    "proto": "UDP",
    "dport": 53,
    "daddr": "8.8.8.8"
  },
  {
    "action": "ignore",
    "proto": "UDP",
    "dport": 53,
    "daddr": "8.8.4.4"
  }
]
```

### Example 6: Trigger on Tagged Flows

Trigger on flows that have been tagged as suspicious:

```json
[
  {
    "action": "trigger",
    "tag": "suspicious"
  }
]
```

### Example 7: Complex Rule - Trigger on External Server Access

Trigger on external access to production servers with anomalies:

```json
[
  {
    "action": "trigger",
    "observe": "datacenter-prod",
    "proto": "TCP",
    "daddr": "192.168.100.",
    "dport": 443,
    "hbos_severity": 2
  }
]
```

### Example 8: Combined Trigger and Ignore Rules

Use multiple rules to create a policy that triggers on anomalies but ignores known patterns:

```json
[
  {
    "action": "trigger",
    "hbos_severity": 3
  },
  {
    "action": "trigger",
    "risk_severity": 3
  },
  {
    "action": "ignore",
    "tag": "known-scanner"
  },
  {
    "action": "ignore",
    "saddr": "10.0.0.",
    "daddr": "10.0.0."
  }
]
```

## Usage

The rule configuration file is specified using the `rule` option when running the rule pipeline. A model file is also required:

```
--options "model=/path/to/model.duckdb,rule=/path/to/rule-config.json"
```

## Severity Levels Reference

### HBOS Severity Levels

| Level | Value | Description |
|-------|-------|-------------|
| Low | 1 | Minor deviation from normal |
| Medium | 2 | Moderate anomaly |
| High | 3 | Significant anomaly |
| Severe | 4 | Major anomaly requiring attention |
| Critical | 5 | Critical anomaly |

### nDPI Risk Severity Levels

| Level | Value | Description |
|-------|-------|-------------|
| Low | 1 | Low risk |
| Medium | 2 | Medium risk |
| High | 3 | High risk |

## Limits

- Maximum of **1000 rules** recommended per configuration file (warning issued if exceeded)
- Rules with no matching conditions (all fields empty/zero) are skipped

## Notes

- Rules are processed in order; later rules can override earlier ones for the same flow.
- Prefix matching on IP addresses allows for subnet-style matching (e.g., `"192.168."` matches any IP starting with `192.168.`).
- The `orient` field represents flow orientation with values like "00", "01", "10", "11".
- The rule processor requires a pre-trained model file to function.
- Trigger counts are reported after processing, grouped by severity level (low, medium, high, severe).
