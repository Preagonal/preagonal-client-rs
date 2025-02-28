#![deny(missing_docs)]

use std::sync::Arc;

use thiserror::Error;
use tokio::sync::mpsc::error::SendError;

use crate::net::{packet::GPacket, protocol::ProtocolError};

/// This module defines the NpcControl client.
pub mod nc;
/// This module defines the RemoteControl client.
pub mod rc;

#[derive(Debug, Error)]
/// Error type for the Rc protocol.
pub enum ClientError {
    /// If a protocol error is encountered.
    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    /// If a send error is encountered.
    #[error("Send error: {0}")]
    Send(#[from] SendError<Arc<dyn GPacket + Send>>),

    /// If a receive error is encountered.
    #[error("Recv error: {0}")]
    Recv(#[from] tokio::sync::oneshot::error::RecvError),

    /// If a timeout error is encountered.
    #[error("Timeout error: {0}")]
    Timeout(#[from] tokio::time::error::Elapsed),

    /// If a TCP error is encountered.
    #[error("TCP error: {0}")]
    Tcp(#[from] std::io::Error),
}
