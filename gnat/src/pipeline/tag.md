# Flow Tag Configuration

This document describes the syntax for the JSON configuration file used to define flow tags in the Galileo Network Analytics (GNA) Toolkit.

## Overview

Flow tags allow you to label network flows based on matching criteria. When a flow matches the specified conditions, a tag (label) is appended to the flow's `tag` field. Up to 16 tags can be applied to a single flow.

## Configuration File Format

The configuration file is a JSON array containing one or more tag rule objects.

```json
[
  {
    "tag": "tag_name",
    "observe": "observation_domain",
    "proto": "protocol",
    "saddr": "source_address",
    "sport": 443,
    "daddr": "destination_address",
    "dport": 80,
    "ndpi_appid": "application_id",
    "orient": "flow_orientation"
  }
]
```

## Field Descriptions

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `tag` | string | **Yes** | - | The tag/label name to apply when the rule matches |
| `observe` | string | No | `""` | Observation domain prefix to match |
| `proto` | string | No | `""` | Protocol to match (exact match) |
| `saddr` | string | No | `""` | Source IP address prefix to match |
| `sport` | integer | No | `0` | Source port to match (exact match, 0 means any) |
| `daddr` | string | No | `""` | Destination IP address prefix to match |
| `dport` | integer | No | `0` | Destination port to match (exact match, 0 means any) |
| `ndpi_appid` | string | No | `""` | nDPI application ID prefix to match |
| `orient` | string | No | `""` | Flow orientation prefix (e.g., "00", "01", "10", "11") |

## Matching Behavior

- **Required field**: Only `tag` is required. All other fields are optional.
- **Empty/zero fields**: Fields with empty strings (`""`) or zero values (`0` for ports) are ignored in matching.
- **Prefix matching**: The fields `observe`, `saddr`, `daddr`, `ndpi_appid`, and `orient` use **prefix matching** (the flow value must start with the specified string).
- **Exact matching**: The fields `proto`, `sport`, and `dport` use **exact matching**.
- **Multiple conditions**: When multiple fields are specified, ALL conditions must match (AND logic).
- **Tag uniqueness**: A tag is only added if it doesn't already exist on the flow.

## Examples

### Example 1: Tag HTTPS Traffic

Tag all flows with destination port 443 as "https":

```json
[
  {
    "tag": "https",
    "dport": 443
  }
]
```

### Example 2: Tag Internal Network Traffic

Tag traffic from the 10.0.0.0/8 network:

```json
[
  {
    "tag": "internal",
    "saddr": "10."
  }
]
```

### Example 3: Tag DNS Traffic

Tag UDP traffic on port 53:

```json
[
  {
    "tag": "dns",
    "proto": "UDP",
    "dport": 53
  }
]
```

### Example 4: Multiple Tags

Apply multiple tags with different rules:

```json
[
  {
    "tag": "web",
    "dport": 80
  },
  {
    "tag": "web",
    "dport": 443
  },
  {
    "tag": "corporate",
    "saddr": "192.168.1."
  },
  {
    "tag": "external-dns",
    "proto": "UDP",
    "dport": 53,
    "daddr": "8.8."
  }
]
```

### Example 5: Tag by Application

Tag flows identified as SSH by nDPI:

```json
[
  {
    "tag": "ssh-traffic",
    "ndpi_appid": "SSH"
  }
]
```

### Example 6: Complex Rule

Tag specific server traffic with multiple conditions:

```json
[
  {
    "tag": "prod-webserver",
    "observe": "datacenter-1",
    "proto": "TCP",
    "daddr": "192.168.100.10",
    "dport": 443
  }
]
```

## Usage

The tag configuration file is specified using the `tag` option when running the tag pipeline:

```
--options "tag=/path/to/tag-config.json"
```

## Limits

- Maximum of **16 tags** per flow (`TAG_LIMIT`)
- Tags are stored as a list in the flow's `tag` field

## Notes

- Rules are processed in order; multiple rules can match and add different tags to the same flow.
- Prefix matching on IP addresses allows for subnet-style matching (e.g., `"192.168."` matches any IP starting with `192.168.`).
- The `orient` field represents flow orientation with values like "00", "01", "10", "11".
