use std::sync::Arc;

use crate::io::io_vec::IntoSyncGraalReader;
use crate::net::packet::{GPacket, PacketConversionError, PacketId};

use super::FromServerPacketId;

/// NcWeaponGet packet received from server
#[derive(Debug)]
pub struct NcWeaponGetImpl {
    /// The name of the script
    pub script_name: String,
    /// The script image
    pub img: String,
    /// The script
    pub script: String,
}

impl NcWeaponGetImpl {
    /// Deserialize a NpcWeaponScript packet from raw bytes
    pub fn from_bytes(data: Vec<u8>) -> Result<Self, PacketConversionError> {
        let mut reader = data.into_sync_graal_reader();

        // Read the script name
        let script_name = reader.read_gstring()?;

        // Read the script image
        let img = reader.read_gstring()?;

        // Read the script
        let script = reader.read_to_end()?;

        // Convert \xa7 to \n
        let script: String = String::from_utf8(
            script
                .iter()
                .map(|&c| if c == 0xa7 { 0xa } else { c })
                .collect(),
        )
        .map_err(|e| {
            PacketConversionError::Other(format!("Failed to convert script to string: {:?}", e))
        })?;

        Ok(Self {
            script_name,
            img,
            script,
        })
    }
}

impl TryFrom<Arc<dyn GPacket>> for NcWeaponGetImpl {
    type Error = PacketConversionError;

    fn try_from(packet: Arc<dyn GPacket>) -> Result<Self, Self::Error> {
        if !matches!(
            packet.id(),
            PacketId::FromServer(FromServerPacketId::NcWeaponGet)
        ) {
            return Err(PacketConversionError::Other(
                "Expected NcWeaponGet packet".to_string(),
            ));
        }

        Self::from_bytes(packet.data().clone())
    }
}
