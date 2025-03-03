#![deny(missing_docs)]

use std::sync::Arc;
use std::{future::Future, pin::Pin};

use game::GameClient;
use nc::NpcControlClient;
use rc::RemoteControlClient;
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

use std::time::Duration;

use super::{
    packet::{PacketEvent, PacketId, from_server::FromServerPacketId},
    protocol::{proto_v6::EncryptionKeys, proto_v6_header::GProtocolV6HeaderFormat},
};

/// An enum that holds all possible GClients.
#[derive(Clone)]
pub enum GClientEnum {
    /// The NpcControl client.
    NpcControl(Arc<NpcControlClient>),
    /// The RemoteControl client.
    RemoteControl(Arc<RemoteControlClient>),
    /// The Game client.
    Game(Arc<GameClient>),
}

type DisconnectFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
type SendReceiveFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Arc<dyn GPacket>, ClientError>> + Send + 'a>>;
type SendPacketFuture<'a> = Pin<Box<dyn Future<Output = Result<(), ClientError>> + Send + 'a>>;
type EventHandlerFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

#[allow(refining_impl_trait)]
impl GClientTrait for GClientEnum {
    fn disconnect(&self) -> DisconnectFuture<'_> {
        match self {
            GClientEnum::NpcControl(client) => Box::pin(client.disconnect()),
            GClientEnum::RemoteControl(client) => Box::pin(client.disconnect()),
            GClientEnum::Game(client) => Box::pin(client.disconnect()),
        }
    }

    fn send_and_receive(
        &self,
        packet: Arc<dyn GPacket + Send>,
        response_packet: FromServerPacketId,
    ) -> SendReceiveFuture<'_> {
        match self {
            GClientEnum::NpcControl(client) => {
                Box::pin(client.send_and_receive(packet, response_packet))
            }
            GClientEnum::RemoteControl(client) => {
                Box::pin(client.send_and_receive(packet, response_packet))
            }
            GClientEnum::Game(client) => Box::pin(client.send_and_receive(packet, response_packet)),
        }
    }

    fn send_packet(&self, packet: Arc<dyn GPacket + Send>) -> SendPacketFuture<'_> {
        match self {
            GClientEnum::NpcControl(client) => Box::pin(client.send_packet(packet)),
            GClientEnum::RemoteControl(client) => Box::pin(client.send_packet(packet)),
            GClientEnum::Game(client) => Box::pin(client.send_packet(packet)),
        }
    }

    fn register_event_handler<F, Fut>(
        &self,
        packet_id: PacketId,
        handler: F,
    ) -> EventHandlerFuture<'_>
    where
        F: Fn(PacketEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        match self {
            GClientEnum::NpcControl(client) => {
                Box::pin(client.register_event_handler(packet_id, handler))
            }
            GClientEnum::RemoteControl(client) => {
                Box::pin(client.register_event_handler(packet_id, handler))
            }
            GClientEnum::Game(client) => {
                Box::pin(client.register_event_handler(packet_id, handler))
            }
        }
    }

    fn wait_for_tasks(&self) -> DisconnectFuture<'_> {
        match self {
            GClientEnum::NpcControl(client) => Box::pin(client.wait_for_tasks()),
            GClientEnum::RemoteControl(client) => Box::pin(client.wait_for_tasks()),
            GClientEnum::Game(client) => Box::pin(client.wait_for_tasks()),
        }
    }
}

/// A struct that contains the configuration for the GClientConfig client.
#[derive(Debug, Clone)]
pub struct GClientConfig {
    /// The host of the server.
    pub host: String,
    /// The port of the server.
    pub port: u16,
    /// The login configuration.
    pub login: GClientLoginConfig,
    /// The timeout for events that have a response.
    pub timeout: Duration,
    /// The type of client.
    pub client_type: GClientConfigType,
}

/// An enum that contains the different types of clients.
#[derive(Debug, Clone)]
pub enum GClientConfigType {
    /// The NpcControl client.
    NpcControl(NpcControlConfig),
    /// The RemoteControl client.
    RemoteControl(RemoteControlConfig),
    /// The Game client.
    Game(GameClientConfig),
}

/// A struct that contains the configuration for the NpcControlClient.
#[derive(Debug, Clone)]
pub struct NpcControlConfig {
    /// The protocol version of the NC client.
    pub version: String,
}

/// A struct that contains the configuration for the RemoteControlClient.
#[derive(Debug, Clone)]
pub struct RemoteControlConfig {
    /// The protocol version of the NC client.
    pub version: String,
    /// When we should automatically disconnect the NpcControl after a period of inactivity.
    pub nc_auto_disconnect: Duration,
}

/// A struct that contains the configuration for the GameClient.
#[derive(Debug, Clone)]
pub struct GameClientConfig {
    /// The protocol version of the game client.
    pub version: String,
    /// Encryption keys.
    pub encryption_keys: EncryptionKeys,
    /// The header format.
    pub header_format: GProtocolV6HeaderFormat,
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
