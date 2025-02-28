#![deny(missing_docs)]

use serde::Serialize;

use crate::net::{
    packet::{GPacket, PacketId},
    serialization::serialize::serialize_to_vector,
};

use super::FromClientPacketId;

/// NcWeaponGet packet.
#[derive(Debug, Serialize)]
pub struct NcWeaponGet {
    /// The name of the weapon
    pub weapon: String,
}

impl NcWeaponGet {
    /// Create a new NcWeaponGet packet.
    pub fn new<S>(weapon: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            weapon: weapon.into(),
        }
    }
}

impl GPacket for NcWeaponGet {
    fn id(&self) -> PacketId {
        PacketId::FromClient(FromClientPacketId::NcWeaponGet)
    }

    fn data(&self) -> Vec<u8> {
        serialize_to_vector(self).expect("Failed to serialize NcWeaponGet packet")
    }
}
