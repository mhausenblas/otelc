//! The embedded OpAMP server.
//!
//! [`OpampServer::start`] binds a TCP listener and accepts OpAMP WebSocket
//! connections. Agent activity is reported on an [`OpampEvent`] channel, and a
//! cloneable [`ServerHandle`] lets callers push remote-config offers and
//! restart commands back to specific agents.

mod registry;
mod ws;

use crate::capabilities::server_capabilities;
use crate::event::OpampEvent;
use crate::instance_uid::InstanceUid;
use crate::message::{config_hash, yaml_config_map};
use crate::pb;
use registry::Registry;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// Configuration for the embedded OpAMP server.
pub struct ServerConfig {
    /// Address to listen on, e.g. `127.0.0.1:4320`.
    pub listen: SocketAddr,
    /// OTLP endpoint offered to agents for their own telemetry, if any.
    pub otlp_offer: Option<String>,
}

/// The embedded OpAMP server.
pub struct OpampServer;

impl OpampServer {
    /// Bind the listener and start accepting connections.
    pub async fn start(
        cfg: ServerConfig,
    ) -> std::io::Result<(ServerHandle, mpsc::Receiver<OpampEvent>)> {
        let listener = TcpListener::bind(cfg.listen).await?;
        let local_addr = listener.local_addr()?;
        let registry = Arc::new(Registry::default());
        let (event_tx, event_rx) = mpsc::channel(256);

        let accept_registry = registry.clone();
        let otlp_offer = cfg.otlp_offer;
        tokio::spawn(async move {
            info!(%local_addr, "OpAMP server listening");
            loop {
                match listener.accept().await {
                    Ok((stream, peer)) => {
                        let registry = accept_registry.clone();
                        let events = event_tx.clone();
                        let offer = otlp_offer.clone();
                        tokio::spawn(async move {
                            if let Err(e) = ws::serve(stream, registry, events, offer).await {
                                debug!(%peer, error = %e, "connection ended");
                            }
                        });
                    }
                    Err(e) => error!(error = %e, "accept failed"),
                }
            }
        });

        Ok((
            ServerHandle {
                registry,
                local_addr,
            },
            event_rx,
        ))
    }
}

/// A cloneable handle for sending commands to connected agents.
#[derive(Clone)]
pub struct ServerHandle {
    registry: Arc<Registry>,
    local_addr: SocketAddr,
}

impl ServerHandle {
    /// The address the server is listening on.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Number of currently connected agents.
    pub fn agent_count(&self) -> usize {
        self.registry.count()
    }

    /// Offer a new remote configuration to an agent. Returns the config hash
    /// the agent is expected to echo back in its `RemoteConfigStatus`.
    pub fn offer_remote_config(&self, uid: &InstanceUid, yaml: &str) -> Result<Vec<u8>, SendError> {
        let hash = config_hash(yaml.as_bytes());
        let msg = pb::ServerToAgent {
            instance_uid: uid.to_vec(),
            capabilities: server_capabilities(),
            remote_config: Some(pb::AgentRemoteConfig {
                config: Some(yaml_config_map(yaml)),
                config_hash: hash.clone(),
            }),
            ..Default::default()
        };
        self.push(uid, msg)?;
        Ok(hash)
    }

    /// Ask an agent to restart.
    pub fn send_restart(&self, uid: &InstanceUid) -> Result<(), SendError> {
        let msg = pb::ServerToAgent {
            instance_uid: uid.to_vec(),
            capabilities: server_capabilities(),
            command: Some(pb::ServerToAgentCommand {
                r#type: pb::CommandType::Restart as i32,
            }),
            ..Default::default()
        };
        self.push(uid, msg)
    }

    fn push(&self, uid: &InstanceUid, msg: pb::ServerToAgent) -> Result<(), SendError> {
        let tx = self.registry.sender(uid).ok_or(SendError::NotConnected)?;
        tx.try_send(msg).map_err(|_| SendError::ChannelFull)
    }
}

/// Failure delivering a command to an agent.
#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("agent is not connected")]
    NotConnected,
    #[error("agent outbound channel is full")]
    ChannelFull,
}
