#![deny(missing_docs)]

use serde::Serialize;

use crate::net::{
    packet::{GPacket, PacketId},
    serialization::serialize::serialize_to_vector,
};

use super::FromClientPacketId;

/// RcNcQuery packet.
#[derive(Debug, Serialize)]
pub struct RcNcQuery {
    /// Unknown u16 field
    pub unknown: u16,
    /// Message
    pub message: String,
}

impl RcNcQuery {
    /// Create a new RcChat packet.
    pub fn new<S>(message: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            unknown: 2,
            message: message.into(),
        }
    }
}

impl GPacket for RcNcQuery {
    fn id(&self) -> PacketId {
        PacketId::FromClient(FromClientPacketId::NpcServerQuery)
    }

    fn data(&self) -> Vec<u8> {
        serialize_to_vector(self).expect("Failed to serialize RcNcQuery packet")
    }
}
