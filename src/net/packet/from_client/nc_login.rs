#![deny(missing_docs)]

use serde::Serialize;

use crate::net::{
    packet::{GPacket, PacketId},
    serialization::{GString, serialize::serialize_to_vector},
};

use super::FromClientPacketId;

/// NcLogin packet.
#[derive(Debug, Serialize)]
pub struct NcLogin {
    /// Version.
    pub version: String,
    /// Account.
    pub account: GString,
    /// Password.
    pub password: GString,
}

impl NcLogin {
    /// Create a new RcLogin packet.
    pub fn new<S>(version: S, account: S, password: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            version: version.into(),
            account: GString(account.into()),
            password: GString(password.into()),
        }
    }
}

impl GPacket for NcLogin {
    fn id(&self) -> PacketId {
        PacketId::FromClient(FromClientPacketId::NcLogin)
    }

    fn data(&self) -> Vec<u8> {
        serialize_to_vector(&self).expect("Failed to serialize NcLogin packet")
    }
}
