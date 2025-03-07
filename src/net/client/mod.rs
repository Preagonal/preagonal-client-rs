#![deny(missing_docs)]

use std::future::Future;
use std::sync::Arc;

use thiserror::Error;
use tokio::sync::mpsc::error::SendError;

use crate::net::{packet::GPacket, protocol::ProtocolError};

/// This module defines the game client.
pub mod game;
/// This module defines the GClient.
pub mod gclient;
/// This module defines the NpcControl.
pub mod nc;
/// This module defines the RemoteControl.
pub mod rc;

use super::packet::{
    PacketConversionError, PacketEvent, PacketId, from_server::FromServerPacketId,
};

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

    /// Unsupported protocol version.
    #[error("NPC Control is not enabled in the config")]
    NcNotEnabled,

    /// Packet conversion error.
    #[error("Packet conversion error: {0}")]
    PacketConversionError(#[from] PacketConversionError),

    /// Other
    #[error("Other error: {0}")]
    Other(String),
}
/// GClientTrait, which all clients must implement.
pub trait GClientTrait {
    // Note: `connection` and `login` will differ depending on the client implementation, so
    // we won't put that in the trait.

    /// Disconnect from the server.
    fn disconnect(&self) -> impl Future<Output = ()> + Send;

    /// Send a packet, and wait for a response.
    fn send_and_receive(
        &self,
        packet: Arc<dyn GPacket + Send>,
        response_packet: FromServerPacketId,
    ) -> impl Future<Output = Result<Arc<dyn GPacket>, ClientError>> + Send;

    /// Send a raw GClient packet.
    fn send_packet(
        &self,
        packet: Arc<dyn GPacket + Send>,
    ) -> impl Future<Output = Result<(), ClientError>> + Send;

    /// Register an event handler.
    fn register_event_handler<F, Fut>(
        &self,
        packet_id: PacketId,
        handler: F,
    ) -> impl Future<Output = ()> + Send + '_
    where
        F: Fn(PacketEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static;

    /// Wait for tasks to finish.
    fn wait_for_tasks(&self) -> impl Future<Output = ()> + Send;
}
