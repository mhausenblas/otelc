//! The control-plane abstraction.
//!
//! The UI talks only to a [`ControlPlane`] and consumes [`ControlEvent`]s; it
//! never touches OpAMP or OTLP types directly. Two implementations exist:
//! [`embedded::EmbeddedControlPlane`] (an in-process OpAMP server + OTLP
//! receiver) and [`external::ExternalControlPlane`] (an adapter onto a
//! third-party OpAMP server).

pub mod embedded;
pub mod external;

use std::time::Instant;

/// A node in an agent's component-health tree.
#[derive(Clone, Debug)]
pub struct HealthNode {
    pub name: String,
    pub healthy: bool,
    pub status: String,
    pub last_error: String,
    pub children: Vec<HealthNode>,
}

/// The status of the last remote-config offer.
#[derive(Clone, Debug)]
pub struct RemoteStatus {
    pub state: String,
    pub error: String,
}

/// Everything the UI knows about one agent.
#[derive(Clone, Debug)]
pub struct AgentDetail {
    pub uid: String,
    pub name: String,
    pub version: String,
    pub healthy: bool,
    pub status: String,
    pub identifying: Vec<(String, String)>,
    pub non_identifying: Vec<(String, String)>,
    pub capabilities: Vec<(String, bool)>,
    pub effective_config: String,
    pub health: Option<HealthNode>,
    pub remote_status: Option<RemoteStatus>,
    pub start_time_unix_nano: u64,
    pub sequence_num: u64,
    pub connected_at: Instant,
    pub last_seen: Instant,
}

impl AgentDetail {
    fn new(uid: String) -> Self {
        let now = Instant::now();
        Self {
            name: uid.clone(),
            uid,
            version: String::new(),
            healthy: true,
            status: String::new(),
            identifying: Vec::new(),
            non_identifying: Vec::new(),
            capabilities: Vec::new(),
            effective_config: String::new(),
            health: None,
            remote_status: None,
            start_time_unix_nano: 0,
            sequence_num: 0,
            connected_at: now,
            last_seen: now,
        }
    }
}

/// A numeric metric observation for the UI.
#[derive(Clone, Debug)]
pub struct MetricRow {
    pub name: String,
    pub value: f64,
    pub unit: String,
}

/// A log line for the UI.
#[derive(Clone, Debug)]
pub struct LogRow {
    pub time_unix_nano: u64,
    pub severity: String,
    pub body: String,
}

/// A snapshot of one agent's own-telemetry.
#[derive(Clone, Debug, Default)]
pub struct TelemetrySnapshot {
    pub metrics: Vec<MetricRow>,
    pub logs: Vec<LogRow>,
    pub span_count: u64,
}

/// An update pushed from the control plane to the UI.
#[derive(Debug)]
pub enum ControlEvent {
    /// An agent was added or updated.
    AgentUpserted(Box<AgentDetail>),
    /// An agent's connection closed.
    AgentDisconnected(String),
    /// Fresh telemetry for an agent, keyed by UID.
    Telemetry(String, TelemetrySnapshot),
    /// A human-readable status message.
    Notice(String),
}

/// Commands the UI can issue, regardless of the underlying mode.
pub trait ControlPlane: Send + Sync {
    /// A short label for the operating mode.
    fn mode(&self) -> &'static str;
    /// The endpoint this control plane is bound to or talking to.
    fn endpoint(&self) -> String;
    /// Offer a new remote configuration to an agent.
    fn push_config(&self, uid: &str, yaml: &str) -> Result<(), String>;
    /// Ask an agent to restart.
    fn restart(&self, uid: &str) -> Result<(), String>;
}
