#![deny(missing_docs)]

use serde::Serialize;

use crate::net::{
    packet::{GPacket, PacketId},
    serialization::{GScript, GString, serialize::serialize_to_vector},
};

use super::FromClientPacketId;

/// NcWeaponGet packet.
#[derive(Debug, Serialize)]
pub struct NcWeaponAdd {
    /// The name of the weapon
    pub weapon: GString,
    /// The image of the weapon
    pub image: GString,
    /// The script of the weapon
    pub script: GScript,
}

impl NcWeaponAdd {
    /// Create a new NcWeaponGet packet.
    pub fn new<S>(weapon: S, image: S, script: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            weapon: GString(weapon.into()),
            image: GString(image.into()),
            script: GScript(script.into()),
        }
    }
}

impl GPacket for NcWeaponAdd {
    fn id(&self) -> PacketId {
        PacketId::FromClient(FromClientPacketId::NcWeaponAdd)
    }

    fn data(&self) -> Vec<u8> {
        serialize_to_vector(self).expect("Failed to serialize NcWeaponAdd packet")
    }
}
