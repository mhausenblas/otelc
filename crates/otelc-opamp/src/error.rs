/// Errors produced while handling OpAMP traffic.
#[derive(Debug, thiserror::Error)]
pub enum OpampError {
    #[error("frame error: {0}")]
    Framing(String),
    #[error("protobuf decode error: {0}")]
    Decode(String),
    #[error("websocket error: {0}")]
    Ws(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
