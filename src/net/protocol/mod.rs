#![deny(missing_docs)]

use std::sync::Arc;

use proto_v4::GProtocolV4;
use proto_v5::GProtocolV5;
use proto_v6::GProtocolV6;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::io::GraalIoError;

use super::packet::{GPacket, PacketConversionError};

/// Module containing the v4 protocol implementation.
pub mod proto_v4;
/// Module containing the v5 protocol implementation.
pub mod proto_v5;
/// Module containing the v6 protocol implementation.
pub mod proto_v6;
/// The v6 protocol header format.
pub mod proto_v6_header;

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

    /// Invalid header format.
    #[error("Invalid header format: {0}")]
    InvalidHeaderFormat(String),

    /// Invalid header
    #[error("Invalid header: {0}")]
    InvalidHeader(String),

    /// Invalid packet length.
    #[error("Invalid packet length. {0} > {1}")]
    InvalidPacketLength(usize, usize),

    /// Rsa decryption error.
    #[error("Rsa decryption error: {0}")]
    RsaDecryption(#[from] rsa::errors::Error),

    /// General decryption error.
    #[error("Decryption error: {0}")]
    Decryption(String),

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

    /// Shutdown the protocol
    fn shutdown(
        &self
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
    /// The v6 protocol.
    V6(GProtocolV6<R, W>),
}

impl<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send> Protocol for GProtocolEnum<R, W> {
    async fn read(&self) -> Result<Arc<dyn GPacket>, ProtocolError> {
        match self {
            GProtocolEnum::V4(proto) => proto.read().await,
            GProtocolEnum::V5(proto) => proto.read().await,
            GProtocolEnum::V6(proto) => proto.read().await,
        }
    }

    async fn write(&self, packet: &(dyn GPacket + Send)) -> Result<(), ProtocolError> {
        match self {
            GProtocolEnum::V4(proto) => proto.write(packet).await,
            GProtocolEnum::V5(proto) => proto.write(packet).await,
            GProtocolEnum::V6(proto) => proto.write(packet).await,
        }
    }

    async fn shutdown(
            &self
        ) -> Result<(), ProtocolError> {
        match self {
            GProtocolEnum::V4(proto) => proto.shutdown().await,
            GProtocolEnum::V5(proto) => proto.shutdown().await,
            GProtocolEnum::V6(proto) => proto.shutdown().await,
        }
    }

    fn version(&self) -> u8 {
        match self {
            GProtocolEnum::V4(proto) => proto.version(),
            GProtocolEnum::V5(proto) => proto.version(),
            GProtocolEnum::V6(proto) => proto.version(),
        }
    }
}
