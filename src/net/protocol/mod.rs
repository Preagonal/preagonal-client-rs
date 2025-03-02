#![deny(missing_docs)]

use std::sync::Arc;

use proto_v4::GProtocolV4;
use proto_v5::GProtocolV5;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::io::GraalIoError;

use super::packet::{GPacket, PacketConversionError};

/// Module containing the v4 protocol implementation.
pub mod proto_v4;
/// Module containing the v5 protocol implementation.
pub mod proto_v5;

/// Error type for protocol errors.
#[derive(Error, Debug)]
pub enum ProtocolError {
    /// Error for an invalid protocol version.
    #[error("Invalid compression value: {0}")]
    InvalidCompression(u8),

    /// GraalIo error.
    #[error("GraalIo error: {0}")]
    GraalIo(#[from] GraalIoError),

    /// IO Error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Packet conversion error.
    #[error("Packet conversion error: {0}")]
    PacketConversion(#[from] PacketConversionError),

    /// Unexpected empty packet queue.
    #[error("Unexpected empty packet queue")]
    EmptyPacketQueue,

    /// Other error.
    #[error("Other: {0}")]
    Other(String),
}

/// Trait representing a protocol.
pub trait Protocol {
    /// Read the next packet from the protocol.
    fn read(
        &self,
    ) -> impl std::future::Future<Output = Result<Arc<dyn GPacket>, ProtocolError>> + Send;
    /// Write a packet to the protocol.
    fn write(
        &self,
        packet: &(dyn GPacket + Send),
    ) -> impl std::future::Future<Output = Result<(), ProtocolError>> + Send;
    /// Get the protocol version.
    fn version(&self) -> u8;
}

/// Enum representing the protocol version.
pub enum GProtocolEnum<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send> {
    /// The v4 protocol.
    V4(GProtocolV4<R, W>),
    /// The v5 protocol.
    V5(GProtocolV5<R, W>),
    // V6(GProtocolV6<R, W>),
}

impl<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send> Protocol for GProtocolEnum<R, W> {
    async fn read(&self) -> Result<Arc<dyn GPacket>, ProtocolError> {
        match self {
            GProtocolEnum::V4(proto) => proto.read().await,
            GProtocolEnum::V5(proto) => proto.read().await,
            // GProtocolEnum::V6(proto) => proto.read(),
        }
    }

    async fn write(&self, packet: &(dyn GPacket + Send)) -> Result<(), ProtocolError> {
        match self {
            GProtocolEnum::V4(proto) => proto.write(packet).await,
            GProtocolEnum::V5(proto) => proto.write(packet).await,
            // GProtocolEnum::V6(proto) => proto.write(packet),
        }
    }

    fn version(&self) -> u8 {
        match self {
            GProtocolEnum::V4(proto) => proto.version(),
            GProtocolEnum::V5(proto) => proto.version(),
            // GProtocolEnum::V6(proto) => proto.version(),
        }
    }
}
