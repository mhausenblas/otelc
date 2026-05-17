//! Routing table mapping connected agents to their outbound channels.

use crate::instance_uid::InstanceUid;
use crate::pb::ServerToAgent;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::mpsc;

/// Tracks the push channel for every live agent connection so the server can
/// deliver unsolicited `ServerToAgent` messages (config offers, restarts).
#[derive(Default)]
pub struct Registry {
    senders: Mutex<HashMap<InstanceUid, mpsc::Sender<ServerToAgent>>>,
}

impl Registry {
    pub fn insert(&self, uid: InstanceUid, tx: mpsc::Sender<ServerToAgent>) {
        self.senders.lock().unwrap().insert(uid, tx);
    }

    pub fn remove(&self, uid: &InstanceUid) {
        self.senders.lock().unwrap().remove(uid);
    }

    pub fn sender(&self, uid: &InstanceUid) -> Option<mpsc::Sender<ServerToAgent>> {
        self.senders.lock().unwrap().get(uid).cloned()
    }

    pub fn count(&self) -> usize {
        self.senders.lock().unwrap().len()
    }
}
