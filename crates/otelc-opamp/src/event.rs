//! Events surfaced by the embedded OpAMP server.

use crate::instance_uid::InstanceUid;
use crate::pb;

/// An observation about an agent, emitted on the server's event channel.
#[derive(Debug)]
pub enum OpampEvent {
    /// An `AgentToServer` message was received. Fields are sparse: an agent
    /// only sets fields that changed since its previous message.
    Message {
        uid: InstanceUid,
        msg: Box<pb::AgentToServer>,
    },
    /// The agent's connection closed.
    Disconnected { uid: InstanceUid },
}
