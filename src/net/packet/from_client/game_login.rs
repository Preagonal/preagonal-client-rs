#![deny(missing_docs)]

use serde::Serialize;

use crate::net::{
    packet::{GPacket, PacketId},
    serialization::{GString, serialize::serialize_to_vector},
};

use super::FromClientPacketId;

/// RcLogin packet.
#[derive(Debug, Serialize)]
pub struct GameLogin {
    /// Version.
    pub version: String,
    /// Account.
    pub account: GString,
    /// Password.
    pub password: GString,
    /// PC IDs
    pub identification: Vec<String>,
}

impl GameLogin {
    /// Create a new RcLogin packet.
    pub fn new<S>(version: S, account: S, password: S, identification: Vec<String>) -> Self
    where
        S: Into<String>,
    {
        Self {
            version: version.into(),
            account: GString(account.into()),
            password: GString(password.into()),
            identification,
        }
    }
}

impl GPacket for GameLogin {
    fn id(&self) -> PacketId {
        PacketId::FromClient(FromClientPacketId::V6Login)
    }

    fn data(&self) -> Vec<u8> {
        serialize_to_vector(&self).expect("Failed to serialize RcLogin packet")
    }
}
