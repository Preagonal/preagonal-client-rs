#![deny(missing_docs)]

use std::{fmt::Debug, sync::Arc};

use from_client::FromClientPacketId;
use from_server::FromServerPacketId;
use thiserror::Error;

use crate::io::GraalIoError;

/// Module containing packets for communication.
pub mod from_client;

/// Module containing packets for communication.
pub mod from_server;

/// Trait representing a packet for communication.
pub trait GPacket: std::fmt::Debug + Send + Sync {
    /// Get the id of the packet.
    fn id(&self) -> PacketId;
    /// Get the data of the packet.
    fn data(&self) -> Vec<u8>;
}

/// A single PacketEvent, used for communication between tokio tasks.
#[derive(Debug, Clone)]
pub struct PacketEvent {
    /// The packet being sent / received.
    pub packet: Arc<dyn GPacket>,
}

/// Struct representing an unstructured GPacket
pub struct GPacketImpl {
    id: PacketId,
    data: Vec<u8>,
}

impl GPacket for GPacketImpl {
    fn id(&self) -> PacketId {
        self.id
    }

    fn data(&self) -> Vec<u8> {
        self.data.clone()
    }
}

impl Debug for GPacketImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GPacket")
            .field("id", &self.id)
            .field("data", &String::from_utf8_lossy(&self.data))
            .finish()
    }
}

/// Struct representing a builder for a GPacket.
pub struct GPacketBuilder {
    id: PacketId,
    data: Vec<u8>,
}

impl GPacketBuilder {
    /// Create a new GPacketBuilder.
    pub fn new(id: PacketId) -> Self {
        Self {
            id,
            data: Vec::new(),
        }
    }

    /// Set the data of the packet.
    pub fn with_data(mut self, data: Vec<u8>) -> Self {
        self.data = data;
        self
    }

    /// Build the packet.
    pub fn build(self) -> Arc<dyn GPacket> {
        Arc::new(GPacketImpl {
            id: self.id,
            data: self.data,
        })
    }
}

/// Enum representing the flow of a packet.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum PacketId {
    /// Packet sent from the client to the server.
    FromClient(FromClientPacketId),
    /// Packet sent from the server to the client.
    FromServer(FromServerPacketId),
}

impl From<PacketId> for u8 {
    fn from(packet: PacketId) -> u8 {
        match packet {
            PacketId::FromClient(id) => id.into(),
            PacketId::FromServer(id) => id.into(),
        }
    }
}

impl From<PacketId> for u64 {
    fn from(packet: PacketId) -> u64 {
        match packet {
            PacketId::FromClient(id) => id.into(),
            PacketId::FromServer(id) => id.into(),
        }
    }
}

impl From<PacketId> for usize {
    fn from(packet: PacketId) -> usize {
        match packet {
            PacketId::FromClient(id) => id.into(),
            PacketId::FromServer(id) => id.into(),
        }
    }
}

/// Enum representing an error when parsing a packet.
#[derive(Debug, Error)]
pub enum PacketConversionError {
    /// GraalIoError
    #[error("GraalIo error: {0}")]
    GraalIo(#[from] GraalIoError),

    /// Error for an invalid packet id.
    #[error("Invalid packet id: {0}")]
    InvalidId(u8),

    /// Other error.
    #[error("Other error: {0}")]
    Other(String),
}

/// Trait representing a packet for communication.
pub trait PacketIdTrait:
    Into<u8>
    + Into<u32>
    + std::fmt::Debug
    + TryFrom<u8>
    + TryFrom<String>
    + TryFrom<&'static str>
    + PartialEq
    + Eq
    + Clone
    + Copy
    + std::hash::Hash
{
    /// Get the number of defined packets.
    ///
    /// # Returns
    /// - The number of packets.
    fn count() -> usize;

    /// Get a list of all defined packets.
    ///
    /// # Returns
    /// - A slice containing all defined packets.
    fn all() -> &'static [Self];
}

/// Macro to define packets for communication.
#[macro_export]
macro_rules! define_packets {
    (
        $enum_name:ident {
            $( $variant:ident $( [ $builder:ty ] )? = $value:expr ),* $(,)?
        }
    ) => {
        /// Enum representing packets for communication.
        ///
        /// Each variant corresponds to a packet, with its numeric value
        /// defined as a `u8`.
        #[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
        #[repr(u8)]
        pub enum $enum_name {
            $(
                #[doc = concat!("Represents the packet ", stringify!($variant))]
                $variant = $value,
            )*
        }

        impl $crate::net::packet::PacketIdTrait for $enum_name {
            fn count() -> usize {
                0 $(+ { let _ = stringify!($variant); 1 })*
            }

            fn all() -> &'static [$enum_name] {
                &[
                    $(
                        $enum_name::$variant,
                    )*
                ]
            }
        }

        // Implement `From<$enum_name>` for `u8`.
        impl From<$enum_name> for u8 {
            fn from(packet: $enum_name) -> u8 {
                packet as u8
            }
        }

        // Implement `From<$enum_name>` for `u32`.
        impl From<$enum_name> for u32 {
            fn from(packet: $enum_name) -> u32 {
                packet as u32
            }
        }

        // Implement `From<$enum_name>` for `u64`.
        impl From<$enum_name> for u64 {
            fn from(packet: $enum_name) -> u64 {
                packet as u64
            }
        }

        // Implement `From<$enum_name>` for `usize`.
        impl From<$enum_name> for usize {
            fn from(packet: $enum_name) -> usize {
                packet as usize
            }
        }

        // Implement `Display` for the enum.
        impl std::fmt::Display for $enum_name {
            /// Converts the enum to a human-readable string.
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", match self {
                    $(
                        $enum_name::$variant => stringify!($variant),
                    )*
                })
            }
        }

        // Implement `TryFrom<u8>` for the enum.
        impl TryFrom<u8> for $enum_name {
            type Error = $crate::net::packet::PacketConversionError;

            fn try_from(byte: u8) -> Result<Self, Self::Error> {
                match byte {
                    $(
                        $value => Ok($enum_name::$variant),
                    )*
                    _ => Err($crate::net::packet::PacketConversionError::InvalidId(byte)),
                }
            }
        }

        impl TryFrom<u16> for $enum_name {
            type Error = $crate::net::packet::PacketConversionError;

            fn try_from(value: u16) -> Result<Self, Self::Error> {
                match value as u8 {
                    $(
                        $value => Ok($enum_name::$variant),
                    )*
                    _ => Err($crate::net::packet::PacketConversionError::InvalidId(value as u8)),
                }
            }
        }

        impl TryFrom<u32> for $enum_name {
            type Error = $crate::net::packet::PacketConversionError;

            fn try_from(value: u32) -> Result<Self, Self::Error> {
                match value as u8 {
                    $(
                        $value => Ok($enum_name::$variant),
                    )*
                    _ => Err($crate::net::packet::PacketConversionError::InvalidId(value as u8)),
                }
            }
        }

        impl TryFrom<u64> for $enum_name {
            type Error = $crate::net::packet::PacketConversionError;

            fn try_from(value: u64) -> Result<Self, Self::Error> {
                match value as u8 {
                    $(
                        $value => Ok($enum_name::$variant),
                    )*
                    _ => Err($crate::net::packet::PacketConversionError::InvalidId(value as u8)),
                }
            }
        }

        impl TryFrom<usize> for $enum_name {
            type Error = $crate::net::packet::PacketConversionError;

            fn try_from(value: usize) -> Result<Self, Self::Error> {
                match value as u8 {
                    $(
                        $value => Ok($enum_name::$variant),
                    )*
                    _ => Err($crate::net::packet::PacketConversionError::InvalidId(value as u8)),
                }
            }
        }

        // Implement `TryFrom<String>` for the enum.
        impl TryFrom<String> for $enum_name {
            type Error = std::io::Error;

            fn try_from(s: String) -> Result<Self, Self::Error> {
                match s.as_ref() {
                    $(
                        stringify!($variant) => Ok($enum_name::$variant),
                    )*
                    _ => Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Invalid packet name: {}", s),
                    )),
                }
            }
        }

        // Implement `TryFrom<&str>` for the enum.
        impl TryFrom<&str> for $enum_name {
            type Error = std::io::Error;

            fn try_from(s: &str) -> Result<Self, Self::Error> {
                match s {
                    $(
                        stringify!($variant) => Ok($enum_name::$variant),
                    )*
                    _ => Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Invalid packet name: {}", s),
                    )),
                }
            }
        }

    //     impl $enum_name {
    //         /// Builds a packet using the provided data.
    //         /// If a custom builder is provided for this packet variant, it calls
    //         /// `<CustomBuilder>::from_data(data)`. Otherwise, it constructs a default
    //         /// `GPacketImpl` with the given id and data.
    //         pub fn build_packet(self, data: Vec<u8>) -> Box<dyn $crate::net::packet::GPacket> {
    //             match self {
    //                 $(
    //                     $enum_name::$variant => {
    //                         define_packets!(@build_packet_case [ $( $builder )? ] { data: data, id: crate::net::packet::PacketId::FromServer($enum_name::$variant) })
    //                     }
    //                 ),*
    //             }
    //         }
    //     }
    // };

    // (@build_packet_case [ $builder:ty ] { data: $data:ident, id: $id:expr } ) => {
    //     Box::new(<$builder>::from_data($data))
    // };

    // // Helper rule when no custom builder type is provided.
    // (@build_packet_case [] { data: $data:ident, id: $id:expr } ) => {
    //     Box::new($crate::net::packet::GPacketImpl { id: $id, data: $data })
    // };
    };
}
