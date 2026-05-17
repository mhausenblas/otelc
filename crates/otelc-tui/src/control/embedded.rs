//! Embedded control plane: an in-process OpAMP server and OTLP receiver.

use super::{
    AgentDetail, ControlEvent, ControlPlane, HealthNode, LogRow, MetricRow, RemoteStatus,
    TelemetrySnapshot,
};
use otelc_opamp::{pb, InstanceUid, OpampEvent, OpampServer, ServerConfig, ServerHandle};
use otelc_otlp::{OtlpReceiver, TelemetryStore};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

type UidMap = Arc<Mutex<HashMap<String, InstanceUid>>>;

/// A control plane backed by an embedded OpAMP server collectors connect to.
pub struct EmbeddedControlPlane {
    handle: ServerHandle,
    uids: UidMap,
}

impl EmbeddedControlPlane {
    /// Start the OpAMP server and OTLP receiver, returning the control plane
    /// and the event stream the UI consumes.
    pub async fn start(
        opamp_addr: SocketAddr,
        otlp_addr: SocketAddr,
    ) -> anyhow::Result<(Box<dyn ControlPlane>, mpsc::Receiver<ControlEvent>)> {
        let store = OtlpReceiver::start(otlp_addr).await?;
        let (handle, opamp_events) = OpampServer::start(ServerConfig {
            listen: opamp_addr,
            otlp_offer: Some(format!("http://{otlp_addr}")),
        })
        .await?;

        let (tx, rx) = mpsc::channel(256);
        let uids: UidMap = Arc::new(Mutex::new(HashMap::new()));

        tokio::spawn(translate(opamp_events, tx.clone(), uids.clone()));
        tokio::spawn(poll_telemetry(store, tx));

        Ok((Box::new(Self { handle, uids }), rx))
    }
}

impl ControlPlane for EmbeddedControlPlane {
    fn mode(&self) -> &'static str {
        "embedded"
    }

    fn endpoint(&self) -> String {
        format!("ws://{}/v1/opamp", self.handle.local_addr())
    }

    fn push_config(&self, uid: &str, yaml: &str) -> Result<(), String> {
        let instance = self
            .uids
            .lock()
            .unwrap()
            .get(uid)
            .copied()
            .ok_or_else(|| "agent is not connected".to_string())?;
        self.handle
            .offer_remote_config(&instance, yaml)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn restart(&self, uid: &str) -> Result<(), String> {
        let instance = self
            .uids
            .lock()
            .unwrap()
            .get(uid)
            .copied()
            .ok_or_else(|| "agent is not connected".to_string())?;
        self.handle
            .send_restart(&instance)
            .map_err(|e| e.to_string())
    }
}

/// Translate raw OpAMP events into accumulated [`AgentDetail`] snapshots.
async fn translate(
    mut events: mpsc::Receiver<OpampEvent>,
    tx: mpsc::Sender<ControlEvent>,
    uids: UidMap,
) {
    let mut details: HashMap<InstanceUid, AgentDetail> = HashMap::new();
    while let Some(event) = events.recv().await {
        match event {
            OpampEvent::Message { uid, msg } => {
                uids.lock().unwrap().insert(uid.to_string(), uid);
                let detail = details
                    .entry(uid)
                    .or_insert_with(|| AgentDetail::new(uid.to_string()));
                merge(detail, &msg);
                let _ = tx
                    .send(ControlEvent::AgentUpserted(Box::new(detail.clone())))
                    .await;
            }
            OpampEvent::Disconnected { uid } => {
                details.remove(&uid);
                uids.lock().unwrap().remove(&uid.to_string());
                let _ = tx
                    .send(ControlEvent::AgentDisconnected(uid.to_string()))
                    .await;
            }
        }
    }
}

/// Periodically forward telemetry snapshots from the OTLP store.
async fn poll_telemetry(store: Arc<TelemetryStore>, tx: mpsc::Sender<ControlEvent>) {
    let mut interval = tokio::time::interval(Duration::from_millis(1500));
    loop {
        interval.tick().await;
        for key in store.keys() {
            let Some(agent) = store.snapshot(&key) else {
                continue;
            };
            let snapshot = TelemetrySnapshot {
                metrics: agent
                    .latest_metrics()
                    .into_iter()
                    .map(|m| MetricRow {
                        name: m.name,
                        value: m.value,
                        unit: m.unit,
                    })
                    .collect(),
                logs: agent
                    .logs
                    .iter()
                    .rev()
                    .take(300)
                    .map(|l| LogRow {
                        time_unix_nano: l.time_unix_nano,
                        severity: l.severity.clone(),
                        body: l.body.clone(),
                    })
                    .collect(),
                span_count: agent.span_count,
            };
            if tx
                .send(ControlEvent::Telemetry(key, snapshot))
                .await
                .is_err()
            {
                return;
            }
        }
    }
}

/// Merge a sparse `AgentToServer` message into an accumulated detail snapshot.
fn merge(detail: &mut AgentDetail, msg: &pb::AgentToServer) {
    detail.last_seen = Instant::now();
    detail.sequence_num = msg.sequence_num;
    detail.capabilities = otelc_opamp::capabilities::describe(msg.capabilities)
        .into_iter()
        .map(|(name, enabled)| (name.to_string(), enabled))
        .collect();

    if let Some(desc) = &msg.agent_description {
        detail.identifying = kv_pairs(&desc.identifying_attributes);
        detail.non_identifying = kv_pairs(&desc.non_identifying_attributes);
        detail.name = display_name(&detail.identifying, &detail.non_identifying, &detail.uid);
        detail.version = attr(&detail.identifying, "service.version").unwrap_or_default();
    }
    if let Some(health) = &msg.health {
        detail.healthy = health.healthy;
        detail.status = health.status.clone();
        detail.start_time_unix_nano = health.start_time_unix_nano;
        detail.health = Some(health_node("agent", health));
    }
    if let Some(effective) = &msg.effective_config {
        detail.effective_config = extract_config(effective);
    }
    if let Some(remote) = &msg.remote_config_status {
        detail.remote_status = Some(RemoteStatus {
            state: remote_status_label(remote.status),
            error: remote.error_message.clone(),
        });
    }
}

fn health_node(name: &str, health: &pb::ComponentHealth) -> HealthNode {
    let mut children: Vec<HealthNode> = health
        .component_health_map
        .iter()
        .map(|(k, v)| health_node(k, v))
        .collect();
    children.sort_by(|a, b| a.name.cmp(&b.name));
    HealthNode {
        name: name.to_string(),
        healthy: health.healthy,
        status: health.status.clone(),
        last_error: health.last_error.clone(),
        children,
    }
}

fn kv_pairs(attrs: &[pb::KeyValue]) -> Vec<(String, String)> {
    attrs
        .iter()
        .map(|kv| {
            let value = kv.value.as_ref().map(anyvalue_string).unwrap_or_default();
            (kv.key.clone(), value)
        })
        .collect()
}

fn anyvalue_string(value: &pb::AnyValue) -> String {
    match &value.value {
        Some(pb::any_value::Value::StringValue(s)) => s.clone(),
        Some(pb::any_value::Value::BoolValue(b)) => b.to_string(),
        Some(pb::any_value::Value::IntValue(i)) => i.to_string(),
        Some(pb::any_value::Value::DoubleValue(d)) => d.to_string(),
        _ => String::new(),
    }
}

fn attr(pairs: &[(String, String)], key: &str) -> Option<String> {
    pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

fn display_name(
    identifying: &[(String, String)],
    non_identifying: &[(String, String)],
    uid: &str,
) -> String {
    attr(non_identifying, "host.name")
        .or_else(|| attr(identifying, "service.name"))
        .unwrap_or_else(|| uid.rsplit('-').next().unwrap_or(uid).to_string())
}

fn extract_config(effective: &pb::EffectiveConfig) -> String {
    let Some(map) = &effective.config_map else {
        return String::new();
    };
    if let Some(file) = map.config_map.get("") {
        return String::from_utf8_lossy(&file.body).to_string();
    }
    map.config_map
        .values()
        .map(|f| String::from_utf8_lossy(&f.body).to_string())
        .collect::<Vec<_>>()
        .join("\n---\n")
}

fn remote_status_label(status: i32) -> String {
    match status {
        x if x == pb::RemoteConfigStatuses::Applied as i32 => "APPLIED",
        x if x == pb::RemoteConfigStatuses::Applying as i32 => "APPLYING",
        x if x == pb::RemoteConfigStatuses::Failed as i32 => "FAILED",
        _ => "UNSET",
    }
    .to_string()
}
