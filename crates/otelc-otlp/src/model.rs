//! UI-facing telemetry models.

use std::collections::VecDeque;

/// A single numeric metric observation.
#[derive(Clone, Debug)]
pub struct MetricPoint {
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub time_unix_nano: u64,
}

/// A single log record.
#[derive(Clone, Debug)]
pub struct LogLine {
    pub time_unix_nano: u64,
    pub severity: String,
    pub body: String,
}

/// All telemetry retained for one agent. The buffers are bounded; oldest
/// entries are evicted first.
#[derive(Clone, Debug, Default)]
pub struct AgentTelemetry {
    pub metrics: VecDeque<MetricPoint>,
    pub logs: VecDeque<LogLine>,
    pub span_count: u64,
}

impl AgentTelemetry {
    /// The most recent value observed for each distinct metric name.
    pub fn latest_metrics(&self) -> Vec<MetricPoint> {
        let mut seen = std::collections::HashMap::new();
        for point in &self.metrics {
            seen.insert(point.name.clone(), point.clone());
        }
        let mut out: Vec<MetricPoint> = seen.into_values().collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }
}
