//! The 16-byte OpAMP agent instance identifier.

/// A 16-byte OpAMP instance UID, generated per the UUIDv7 spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstanceUid([u8; 16]);

impl InstanceUid {
    /// Generate a fresh UUIDv7-based instance UID.
    pub fn new_v7() -> Self {
        Self(*uuid::Uuid::now_v7().as_bytes())
    }

    /// Build from raw bytes; OpAMP requires exactly 16 bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 16 {
            return None;
        }
        let mut buf = [0u8; 16];
        buf.copy_from_slice(bytes);
        Some(Self(buf))
    }

    /// The raw 16 bytes.
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// The raw bytes as an owned vector (for protobuf fields).
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    /// A short, human-friendly suffix for dense UI tables.
    pub fn short(&self) -> String {
        let s = self.to_string();
        s.rsplit('-').next().unwrap_or(&s).to_string()
    }
}

impl std::fmt::Display for InstanceUid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", uuid::Uuid::from_bytes(self.0))
    }
}
