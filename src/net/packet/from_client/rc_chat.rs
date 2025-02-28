#![deny(missing_docs)]

use serde::Serialize;

use crate::net::{
    packet::{GPacket, PacketId},
    serialization::serialize::serialize_to_vector,
};

use super::FromClientPacketId;

/// RcChat packet.
#[derive(Debug, Serialize)]
pub struct RcChat {
    /// Message
    pub message: String,
}

impl RcChat {
    /// Create a new RcChat packet.
    pub fn new<S>(message: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            message: message.into(),
        }
    }
}

impl GPacket for RcChat {
    fn id(&self) -> PacketId {
        PacketId::FromClient(FromClientPacketId::RcChat)
    }

    fn data(&self) -> Vec<u8> {
        serialize_to_vector(self).expect("Failed to serialize RcChat packet")
    }
}
