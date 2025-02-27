#![deny(missing_docs)]

use crate::{
    io::{GraalIoError, io_vec::IntoSyncGraalWriterRef},
    net::packet::{GPacket, PacketId},
};

use super::FromClientPacketId;

/// RcLogin packet.
#[derive(Debug)]
pub struct RcLogin {
    /// Encryption key.
    pub encryption_key: u32,
    /// Version.
    pub version: String,
    /// Account.
    pub account: String,
    /// Password.
    pub password: String,
    /// Platform.
    pub platform: String,
    /// PC IDs
    pub pc_ids: Vec<String>,
}

impl RcLogin {
    /// Create a new RcLogin packet.
    pub fn new(
        encryption_key: u32,
        version: String,
        account: String,
        password: String,
        platform: String,
        pc_ids: Vec<String>,
    ) -> Self {
        Self {
            encryption_key,
            version,
            account,
            password,
            platform,
            pc_ids,
        }
    }

    fn serialize(&self) -> Result<Vec<u8>, GraalIoError> {
        let mut vec = Vec::new();
        {
            let mut writer = vec.into_sync_graal_writer();

            writer.write_gu8(self.encryption_key.into())?;
            writer.write_bytes(self.version.as_bytes())?;
            writer.write_gstring(&self.account)?;
            writer.write_gstring(&self.password)?;
            writer.write_bytes(self.platform.as_bytes())?;
            writer.write_bytes(b",\"")?;
            for (i, pc_id) in self.pc_ids.iter().enumerate() {
                if i > 0 {
                    writer.write_bytes(b"\",\"")?;
                }
                writer.write_bytes(pc_id.as_bytes())?;
            }
            writer.write_bytes(b"\"")?;
            writer.flush()?;
        }

        Ok(vec)
    }
}

impl GPacket for RcLogin {
    fn id(&self) -> PacketId {
        PacketId::FromClient(FromClientPacketId::RcLogin)
    }

    fn data(&self) -> Vec<u8> {
        self.serialize()
            .expect("Failed to serialize RcLogin packet")
    }
}
