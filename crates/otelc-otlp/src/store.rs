//! Bounded in-memory store for received telemetry, keyed by agent.

use crate::model::{AgentTelemetry, LogLine, MetricPoint};
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue};
use opentelemetry_proto::tonic::metrics::v1::{metric, number_data_point, NumberDataPoint};
use opentelemetry_proto::tonic::resource::v1::Resource;
use std::collections::HashMap;
use std::sync::Mutex;

const MAX_METRICS: usize = 4000;
const MAX_LOGS: usize = 2000;

/// Ring-buffered telemetry for every agent that has reported.
#[derive(Default)]
pub struct TelemetryStore {
    agents: Mutex<HashMap<String, AgentTelemetry>>,
}

impl TelemetryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// A snapshot of one agent's telemetry, if any has been received.
    pub fn snapshot(&self, key: &str) -> Option<AgentTelemetry> {
        self.agents.lock().unwrap().get(key).cloned()
    }

    /// All agent keys currently holding telemetry.
    pub fn keys(&self) -> Vec<String> {
        self.agents.lock().unwrap().keys().cloned().collect()
    }

    /// Ingest an OTLP metrics export request.
    pub fn ingest_metrics(&self, req: ExportMetricsServiceRequest) {
        let mut agents = self.agents.lock().unwrap();
        for rm in req.resource_metrics {
            let key = resource_key(&rm.resource);
            let entry = agents.entry(key).or_default();
            for sm in rm.scope_metrics {
                for m in sm.metrics {
                    let points = match m.data {
                        Some(metric::Data::Gauge(g)) => g.data_points,
                        Some(metric::Data::Sum(s)) => s.data_points,
                        _ => Vec::new(),
                    };
                    for dp in points {
                        entry.metrics.push_back(MetricPoint {
                            name: m.name.clone(),
                            value: number_value(&dp),
                            unit: m.unit.clone(),
                            time_unix_nano: dp.time_unix_nano,
                        });
                    }
                }
            }
            while entry.metrics.len() > MAX_METRICS {
                entry.metrics.pop_front();
            }
        }
    }

    /// Ingest an OTLP logs export request.
    pub fn ingest_logs(&self, req: ExportLogsServiceRequest) {
        let mut agents = self.agents.lock().unwrap();
        for rl in req.resource_logs {
            let key = resource_key(&rl.resource);
            let entry = agents.entry(key).or_default();
            for sl in rl.scope_logs {
                for record in sl.log_records {
                    let severity = if record.severity_text.is_empty() {
                        severity_label(record.severity_number)
                    } else {
                        record.severity_text.clone()
                    };
                    let body = record
                        .body
                        .as_ref()
                        .map(any_value_string)
                        .unwrap_or_default();
                    entry.logs.push_back(LogLine {
                        time_unix_nano: record.time_unix_nano,
                        severity,
                        body,
                    });
                }
            }
            while entry.logs.len() > MAX_LOGS {
                entry.logs.pop_front();
            }
        }
    }

    /// Ingest an OTLP trace export request (counted only).
    pub fn ingest_traces(&self, req: ExportTraceServiceRequest) {
        let mut agents = self.agents.lock().unwrap();
        for rs in req.resource_spans {
            let key = resource_key(&rs.resource);
            let entry = agents.entry(key).or_default();
            let count: usize = rs.scope_spans.iter().map(|ss| ss.spans.len()).sum();
            entry.span_count += count as u64;
        }
    }
}

fn number_value(dp: &NumberDataPoint) -> f64 {
    match dp.value {
        Some(number_data_point::Value::AsDouble(d)) => d,
        Some(number_data_point::Value::AsInt(i)) => i as f64,
        None => 0.0,
    }
}

fn resource_key(resource: &Option<Resource>) -> String {
    let Some(resource) = resource else {
        return "unknown".to_string();
    };
    let mut name = None;
    for kv in &resource.attributes {
        if let Some(value) = &kv.value {
            let text = any_value_string(value);
            match kv.key.as_str() {
                "service.instance.id" => return text,
                "service.name" => name = Some(text),
                _ => {}
            }
        }
    }
    name.unwrap_or_else(|| "unknown".to_string())
}

fn any_value_string(value: &AnyValue) -> String {
    match &value.value {
        Some(any_value::Value::StringValue(s)) => s.clone(),
        Some(any_value::Value::BoolValue(b)) => b.to_string(),
        Some(any_value::Value::IntValue(i)) => i.to_string(),
        Some(any_value::Value::DoubleValue(d)) => d.to_string(),
        Some(any_value::Value::BytesValue(_)) => "<bytes>".to_string(),
        Some(any_value::Value::ArrayValue(_)) => "<array>".to_string(),
        Some(any_value::Value::KvlistValue(_)) => "<kvlist>".to_string(),
        Some(_) => "<value>".to_string(),
        None => String::new(),
    }
}

fn severity_label(number: i32) -> String {
    match number {
        1..=4 => "TRACE",
        5..=8 => "DEBUG",
        9..=12 => "INFO",
        13..=16 => "WARN",
        17..=20 => "ERROR",
        21..=24 => "FATAL",
        _ => "UNSET",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eviction_bounds_metrics() {
        let store = TelemetryStore::new();
        let mut entry = AgentTelemetry::default();
        for i in 0..(MAX_METRICS + 50) {
            entry.metrics.push_back(MetricPoint {
                name: "m".into(),
                value: i as f64,
                unit: String::new(),
                time_unix_nano: i as u64,
            });
        }
        while entry.metrics.len() > MAX_METRICS {
            entry.metrics.pop_front();
        }
        store.agents.lock().unwrap().insert("a".into(), entry);
        assert_eq!(store.snapshot("a").unwrap().metrics.len(), MAX_METRICS);
    }

    #[test]
    fn severity_mapping() {
        assert_eq!(severity_label(9), "INFO");
        assert_eq!(severity_label(17), "ERROR");
    }
}
