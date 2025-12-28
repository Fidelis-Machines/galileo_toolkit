![galileo](img/gnat-light.png#gh-light-mode-only)
![galileo](img/gnat-dark.png#gh-dark-mode-only)

# Galileo Network Analytics Toolkit

The Galileo Network Analytics Toolkit (GNAT) is a robust microservices-oriented framework for building network data pipelines specifically tailored for cybersecurity operations. Built with modularity and scalability at its core, this toolkit is designed to empower security teams to construct robust threat hunting and anomaly detection stacks that can adapt to evolving security landscapes.

## Designed for Microsegmented Networks

GNAT is designed primarily for networks that have been **microsegmented**, including:

- **IoT (Internet of Things)** - Smart home devices, sensors, wearables
- **IIoT (Industrial Internet of Things)** - Manufacturing systems, SCADA, PLCs
- **IoRT (Internet of Robotic Things)** - Autonomous systems, robotic process automation

These environments present unique challenges: devices whose traffic patterns fall outside traditional network baselines, limited visibility into proprietary protocols, and the need to detect anomalies in constrained, purpose-built systems. GNAT provides the tools to establish behavioral baselines and identify deviations that may indicate compromise or misconfiguration.

## Key Components

### gnat - Pipeline Processing Tools
A suite of composable CLI utilities for flow data processing:
- `gnat_collect` - Collect and aggregate flow data
- `gnat_import` / `gnat_export` - Data interchange between formats
- `gnat_merge` / `gnat_split` - Combine or partition datasets
- `gnat_hbos_model` - HBOS model training for anomaly detection
- `gnat_hbos` - Histogram-Based Outlier Score anomaly detection
- `gnat_iforest_model` - Extended Isolation Forest model training
- `gnat_iforest` - Extended Isolation Forest anomaly detection
- `gnat_geoip` - Geographic IP enrichment (MaxMind)
- `gnat_reputation` - IP reputation enrichment (AbuseIPDB)
- `gnat_tag` / `gnat_rule` - Flow tagging and rule-based classification
- `gnat_store` / `gnat_cache` - Persistent and in-memory storage
- `gnat_sample` - Statistical sampling for large datasets

## Unix Philosophy

GNAT embraces the core principle of building Unix command-line interfaces: create small, focused programs that do one thing well and can be combined to accomplish more complex tasks. This philosophy emphasizes modularity, simplicity, and the use of text streams (JSON Lines) as a universal interface.

By applying these principles to cybersecurity analytics, GNAT allows teams to build sophisticated solutions from composable, single-purpose command-line tools that can be orchestrated together to create powerful workflows. This approach enables high availability, fault tolerance, and seamless integration with existing security infrastructure.

## Use Cases

GNAT is particularly well-suited for:

- **Security Operations Centers (SOCs)** - Real-time monitoring and threat detection
- **Managed Security Service Providers (MSSPs)** - Multi-tenant analytics at scale
- **OT/ICS Security Teams** - Visibility into industrial and operational technology networks
- **IoT Security Researchers** - Device fingerprinting and behavioral analysis
- **Threat Hunters** - Historical analysis and proactive threat discovery

The stack supports near real-time monitoring, historical data analysis, and proactive threat hunting initiatives, making it ideal for both reactive and proactive security measures.

## Technology Stack

- **Rust** - Memory-safe, high-performance core components
- **DuckDB** - Embedded analytics database for Parquet processing
- **nDPI** - Deep packet inspection for protocol identification
- **JA4** - Next-generation network fingerprinting

## Documentation

Detailed documentation for individual tools is available in the `gnat_docs/` directory:

- [HBOS Anomaly Detection](gnat_docs/hbos.md) - Histogram-based outlier scoring
- [IForest Anomaly Detection](gnat_docs/iforest.md) - Extended Isolation Forest
- [GeoIP Enrichment](gnat_docs/geoip.md) - Geographic IP lookup
- [IP Reputation](gnat_docs/reputation.md) - Threat intelligence integration

Configuration documentation:
- [Flow Rule Configuration](gnat/src/pipeline/rule.md) - Rule-based alerting
- [Flow Tagging](gnat/src/pipeline/tag.md) - Flow labeling

## Additional Information

For an overview, please refer to the documentation: [Galileo Toolkit](https://galileotoolkit.org/)

