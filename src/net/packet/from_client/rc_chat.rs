#![deny(missing_docs)]

use crate::net::packet::{GPacket, PacketId};

use super::FromClientPacketId;

/// RcChat packet.
#[derive(Debug)]
pub struct RcChat {
    /// Message
    pub message: String,
}

impl RcChat {
    /// Create a new RcChat packet.
    pub fn new(message: String) -> Self {
        Self { message }
    }

    fn serialize(&self) -> Vec<u8> {
        // Simply turn the message into a byte vector.
        self.message.as_bytes().to_vec()
    }
}

impl GPacket for RcChat {
    fn id(&self) -> PacketId {
        PacketId::FromClient(FromClientPacketId::RcChat)
    }

    fn data(&self) -> Vec<u8> {
        self.serialize()
    }
}
