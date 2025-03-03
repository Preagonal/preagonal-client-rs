use std::sync::Arc;

use crate::io::io_vec::IntoSyncGraalReader;
use crate::net::packet::{GPacket, PacketConversionError, PacketId};

use super::FromServerPacketId;

/// NpcWeaponScript packet received from server
#[derive(Debug)]
pub struct NpcWeaponScript {
    /// The type of script (e.g., "weapon")
    pub script_type: String,
    /// The name of the script
    pub script_name: String,
    /// The weapon bytecode
    pub weapon_bytecode: Vec<u8>,
}

impl NpcWeaponScript {
    /// Deserialize a NpcWeaponScript packet from raw bytes
    pub fn from_bytes(data: Vec<u8>) -> Result<Self, PacketConversionError> {
        let data_len = data.len();
        let mut reader = data.into_sync_graal_reader();
        let header_length = reader.read_gu16()? as usize;
        let header = reader.read_exact(header_length)?;

        log::debug!(
            "NpcWeaponScript header length {}, data length: {}",
            header_length,
            data_len
        );

        // Read the script header
        let mut header_reader = header.into_sync_graal_reader();
        let parts = header_reader.split_vec(b',')?;
        if parts.len() < 2 {
            return Err(PacketConversionError::Other(
                "Invalid NpcWeaponScript packet".to_string(),
            ));
        }

        let script_type = String::from_utf8(parts[0].clone()).map_err(|_| {
            PacketConversionError::Other(
                "Failed to parse UTF-8 String from script type".to_string(),
            )
        })?;
        let script_name = String::from_utf8(parts[1].clone()).map_err(|_| {
            PacketConversionError::Other(
                "Failed to parse UTF-8 String from script name".to_string(),
            )
        })?;

        // Read the remaining bytes as weapon bytecode
        let weapon_bytecode = reader.read_to_end()?;

        Ok(Self {
            script_type,
            script_name,
            weapon_bytecode,
        })
    }
}

impl TryFrom<Arc<dyn GPacket>> for NpcWeaponScript {
    type Error = PacketConversionError;

    fn try_from(packet: Arc<dyn GPacket>) -> Result<Self, Self::Error> {
        if !matches!(
            packet.id(),
            PacketId::FromServer(FromServerPacketId::NpcWeaponScript)
        ) {
            return Err(PacketConversionError::Other(
                "Expected NpcWeaponScript packet".to_string(),
            ));
        }

        Self::from_bytes(packet.data().clone())
    }
}
