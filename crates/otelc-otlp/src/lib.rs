//! An embedded OTLP/gRPC receiver.
//!
//! OpAMP can instruct an OpenTelemetry Collector to send its *own* telemetry
//! (metrics, logs, traces about itself) to an OTLP endpoint. This crate hosts
//! that endpoint and keeps the most recent samples in a bounded in-memory
//! store, keyed by the agent's `service.instance.id` resource attribute.

pub mod model;
pub mod receiver;
pub mod store;

pub use model::{AgentTelemetry, LogLine, MetricPoint};
pub use receiver::OtlpReceiver;
pub use store::TelemetryStore;
