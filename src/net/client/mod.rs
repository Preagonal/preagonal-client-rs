#![deny(missing_docs)]

use std::sync::Arc;

use thiserror::Error;
use tokio::sync::mpsc::error::SendError;

use crate::net::{packet::GPacket, protocol::ProtocolError};

/// This module defines the GClient.
pub mod gclient;
/// This module defines the NpcControl.
pub mod nc;
/// This module defines the RemoteControl.
pub mod rc;

use std::time::Duration;

/// A struct that contains the configuration for the GClientConfig client.
#[derive(Debug, Clone)]
pub struct GClientConfig {
    /// The host of the server.
    pub host: String,
    /// The port of the server.
    pub port: u16,
    /// The protocol version of the RC client.
    pub rc_protocol_version: String,
    /// The protocol version of the NC client.
    pub nc_protocol_version: String,
    /// The login configuration.
    pub login: GClientLoginConfig,
    /// The timeout for events that have a response.
    pub timeout: Duration,
    /// When we should automatically disconnect the NpcControl after a period of inactivity.
    pub nc_auto_disconnect: Duration,
}

/// A struct that contains the RcLoginConfig
#[derive(Debug, Clone)]
pub struct GClientLoginConfig {
    /// The username of the client.
    pub username: String,
    /// The password of the client.
    pub password: String,
    /// The identification of the client.
    pub identification: Vec<String>,
}

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

    /// Unsupported protocol version.
    #[error("Unsupported protocol version")]
    UnsupportedProtocolVersion,

    /// Other
    #[error("Other error: {0}")]
    Other(String),
}
