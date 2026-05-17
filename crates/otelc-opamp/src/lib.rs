//! OpAMP wire types, WebSocket framing, and an embedded OpAMP server.
//!
//! This crate is deliberately UI-agnostic: it speaks the Open Agent Management
//! Protocol and surfaces agent activity as [`OpampEvent`]s on a channel.

/// Generated OpAMP protobuf types (`package opamp.proto.v1`).
pub mod pb {
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/opamp.proto.v1.rs"));
}

pub mod capabilities;
pub mod error;
pub mod event;
pub mod framing;
pub mod instance_uid;
pub mod message;
pub mod server;

pub use error::OpampError;
pub use event::OpampEvent;
pub use instance_uid::InstanceUid;
pub use server::{OpampServer, SendError, ServerConfig, ServerHandle};
