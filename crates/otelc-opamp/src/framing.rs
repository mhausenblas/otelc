//! OpAMP WebSocket transport framing.
//!
//! Each OpAMP WebSocket binary message is a varint-encoded header followed by
//! the protobuf payload. The current spec mandates a header value of `0`; the
//! decoder simply consumes the varint regardless of value, so a future
//! non-zero header degrades gracefully into "skip the header".

use crate::error::OpampError;
use prost::Message;

/// Encode an OpAMP message into a WebSocket binary frame payload.
pub fn encode<M: Message>(msg: &M) -> Vec<u8> {
    let mut buf = Vec::with_capacity(msg.encoded_len() + 1);
    prost::encoding::encode_varint(0, &mut buf);
    msg.encode(&mut buf)
        .expect("encoding into a Vec is infallible");
    buf
}

/// Decode an OpAMP message from a WebSocket binary frame payload.
pub fn decode<M: Message + Default>(data: &[u8]) -> Result<M, OpampError> {
    let mut buf: &[u8] = data;
    let _header = prost::encoding::decode_varint(&mut buf)
        .map_err(|e| OpampError::Framing(format!("invalid frame header: {e}")))?;
    M::decode(buf).map_err(|e| OpampError::Decode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pb::AgentToServer;

    #[test]
    fn round_trip() {
        let original = AgentToServer {
            instance_uid: vec![7u8; 16],
            sequence_num: 42,
            capabilities: 0x01,
            ..Default::default()
        };
        let frame = encode(&original);
        // First byte is the varint header and MUST be 0.
        assert_eq!(frame[0], 0);
        let decoded: AgentToServer = decode(&frame).unwrap();
        assert_eq!(decoded.sequence_num, 42);
        assert_eq!(decoded.instance_uid, vec![7u8; 16]);
    }

    #[test]
    fn known_vector() {
        // Header byte 0x00, then an empty protobuf message.
        let decoded: AgentToServer = decode(&[0x00]).unwrap();
        assert_eq!(decoded.sequence_num, 0);
    }

    #[test]
    fn rejects_empty() {
        let err = decode::<AgentToServer>(&[]);
        assert!(err.is_err());
    }
}
