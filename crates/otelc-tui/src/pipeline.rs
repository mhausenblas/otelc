//! Parse an OpenTelemetry Collector effective config into a pipeline graph.
//!
//! The graph is derived purely from `service.pipelines` plus the set of
//! declared `connectors`. A connector that appears as an exporter in one
//! pipeline and a receiver in another bridges the two, which is what makes the
//! overall topology a DAG rather than a set of independent chains.

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

/// A parsed collector topology.
#[derive(Debug, Clone, Default)]
pub struct PipelineGraph {
    pub pipelines: Vec<Pipeline>,
    pub connectors: BTreeSet<String>,
}

/// One `service.pipelines` entry.
#[derive(Debug, Clone)]
pub struct Pipeline {
    pub name: String,
    pub receivers: Vec<String>,
    pub processors: Vec<String>,
    pub exporters: Vec<String>,
}

/// A connector bridging two pipelines: `(connector, from_pipeline, to_pipeline)`.
pub type Bridge = (String, String, String);

impl PipelineGraph {
    /// True if `name` names a declared connector.
    pub fn is_connector(&self, name: &str) -> bool {
        self.connectors.contains(name)
    }

    /// All connector bridges in the topology.
    pub fn bridges(&self) -> Vec<Bridge> {
        let mut out = Vec::new();
        for connector in &self.connectors {
            let from: Vec<&str> = self
                .pipelines
                .iter()
                .filter(|p| p.exporters.iter().any(|e| e == connector))
                .map(|p| p.name.as_str())
                .collect();
            let to: Vec<&str> = self
                .pipelines
                .iter()
                .filter(|p| p.receivers.iter().any(|r| r == connector))
                .map(|p| p.name.as_str())
                .collect();
            for f in &from {
                for t in &to {
                    out.push((connector.clone(), f.to_string(), t.to_string()));
                }
            }
        }
        out
    }
}

#[derive(Deserialize)]
struct RawConfig {
    connectors: Option<serde_yaml_ng::Value>,
    service: Option<RawService>,
}

#[derive(Deserialize)]
struct RawService {
    #[serde(default)]
    pipelines: BTreeMap<String, RawPipeline>,
}

#[derive(Deserialize, Default)]
struct RawPipeline {
    #[serde(default)]
    receivers: Vec<String>,
    #[serde(default)]
    processors: Vec<String>,
    #[serde(default)]
    exporters: Vec<String>,
}

/// Parse a collector config YAML document into a [`PipelineGraph`].
pub fn parse(yaml: &str) -> Result<PipelineGraph, String> {
    let raw: RawConfig = serde_yaml_ng::from_str(yaml).map_err(|e| e.to_string())?;
    let connectors = match raw.connectors {
        Some(serde_yaml_ng::Value::Mapping(map)) => map
            .into_iter()
            .filter_map(|(k, _)| k.as_str().map(str::to_string))
            .collect(),
        _ => BTreeSet::new(),
    };
    let pipelines = raw
        .service
        .map(|s| s.pipelines)
        .unwrap_or_default()
        .into_iter()
        .map(|(name, p)| Pipeline {
            name,
            receivers: p.receivers,
            processors: p.processors,
            exporters: p.exporters,
        })
        .collect();
    Ok(PipelineGraph {
        pipelines,
        connectors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
connectors:
  spanmetrics: {}
exporters:
  otlphttp: {}
service:
  pipelines:
    traces:
      receivers: [otlp]
      processors: [batch]
      exporters: [otlphttp, spanmetrics]
    metrics:
      receivers: [otlp, spanmetrics]
      exporters: [otlphttp]
"#;

    #[test]
    fn parses_pipelines_and_connectors() {
        let graph = parse(SAMPLE).unwrap();
        assert_eq!(graph.pipelines.len(), 2);
        assert!(graph.is_connector("spanmetrics"));
        assert!(!graph.is_connector("otlp"));
    }

    #[test]
    fn detects_connector_bridge() {
        let graph = parse(SAMPLE).unwrap();
        let bridges = graph.bridges();
        assert_eq!(
            bridges,
            vec![("spanmetrics".into(), "traces".into(), "metrics".into())]
        );
    }

    #[test]
    fn empty_config_is_ok() {
        let graph = parse("{}").unwrap();
        assert!(graph.pipelines.is_empty());
    }
}
