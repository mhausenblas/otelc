//! Per-connection OpAMP WebSocket handling.

use super::registry::Registry;
use crate::capabilities::{agent_has, server_capabilities};
use crate::error::OpampError;
use crate::event::OpampEvent;
use crate::framing;
use crate::instance_uid::InstanceUid;
use crate::pb;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{info, warn};

/// Drive a single agent connection until the socket closes.
pub async fn serve(
    stream: TcpStream,
    registry: Arc<Registry>,
    events: mpsc::Sender<OpampEvent>,
    otlp_offer: Option<String>,
) -> Result<(), OpampError> {
    let ws = tokio_tungstenite::accept_async(stream)
        .await
        .map_err(|e| OpampError::Ws(e.to_string()))?;
    let (mut write, mut read) = ws.split();
    let (push_tx, mut push_rx) = mpsc::channel::<pb::ServerToAgent>(64);

    let mut uid: Option<InstanceUid> = None;
    let mut first_message = true;

    loop {
        tokio::select! {
            incoming = read.next() => {
                let Some(frame) = incoming else { break };
                let frame = frame.map_err(|e| OpampError::Ws(e.to_string()))?;
                match frame {
                    WsMessage::Binary(data) => {
                        let a2s: pb::AgentToServer = match framing::decode(&data) {
                            Ok(m) => m,
                            Err(e) => {
                                warn!(error = %e, "dropping malformed OpAMP frame");
                                continue;
                            }
                        };
                        let agent_uid = InstanceUid::from_bytes(&a2s.instance_uid)
                            .unwrap_or_else(InstanceUid::new_v7);
                        if uid.is_none() {
                            uid = Some(agent_uid);
                            registry.insert(agent_uid, push_tx.clone());
                            info!(uid = %agent_uid, "agent connected");
                        }
                        let caps = a2s.capabilities;
                        let closing = a2s.agent_disconnect.is_some();
                        let _ = events
                            .send(OpampEvent::Message { uid: agent_uid, msg: Box::new(a2s) })
                            .await;

                        let mut reply = pb::ServerToAgent {
                            instance_uid: agent_uid.to_vec(),
                            capabilities: server_capabilities(),
                            ..Default::default()
                        };
                        if first_message {
                            first_message = false;
                            if let Some(endpoint) = otlp_offer.as_deref() {
                                if reports_own_telemetry(caps) {
                                    reply.connection_settings =
                                        Some(own_telemetry_offer(endpoint));
                                }
                            }
                        }
                        send(&mut write, &reply).await?;
                        if closing { break; }
                    }
                    WsMessage::Ping(payload) => {
                        write.send(WsMessage::Pong(payload)).await
                            .map_err(|e| OpampError::Ws(e.to_string()))?;
                    }
                    WsMessage::Close(_) => break,
                    _ => {}
                }
            }
            outgoing = push_rx.recv() => {
                if let Some(msg) = outgoing {
                    send(&mut write, &msg).await?;
                }
            }
        }
    }

    if let Some(uid) = uid {
        registry.remove(&uid);
        let _ = events.send(OpampEvent::Disconnected { uid }).await;
        info!(uid = %uid, "agent disconnected");
    }
    Ok(())
}

async fn send<S>(write: &mut S, msg: &pb::ServerToAgent) -> Result<(), OpampError>
where
    S: SinkExt<WsMessage> + Unpin,
{
    write
        .send(WsMessage::Binary(framing::encode(msg).into()))
        .await
        .map_err(|_| OpampError::Ws("failed to send frame".to_string()))
}

fn reports_own_telemetry(caps: u64) -> bool {
    agent_has(caps, pb::AgentCapabilities::ReportsOwnMetrics)
        || agent_has(caps, pb::AgentCapabilities::ReportsOwnLogs)
        || agent_has(caps, pb::AgentCapabilities::ReportsOwnTraces)
}

fn own_telemetry_offer(otlp_endpoint: &str) -> pb::ConnectionSettingsOffers {
    let settings = pb::TelemetryConnectionSettings {
        destination_endpoint: otlp_endpoint.to_string(),
        ..Default::default()
    };
    pb::ConnectionSettingsOffers {
        own_metrics: Some(settings.clone()),
        own_logs: Some(settings.clone()),
        own_traces: Some(settings),
        ..Default::default()
    }
}
