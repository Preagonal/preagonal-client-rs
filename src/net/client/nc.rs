use std::sync::Arc;

use tokio::{io::BufReader, net::TcpStream};

use crate::net::{
    packet::{
        GPacket,
        from_client::{nc_login::NcLogin, nc_weapon_add::NcWeaponAdd, nc_weapon_get::NcWeaponGet},
        from_server::FromServerPacketId,
    },
    protocol::{GProtocolEnum, proto_v4::GProtocolV4},
};

use super::{ClientError, GClientConfig, gclient::GClient};

/// A struct that contains the NpcControlClient.
pub struct NpcControlClient {
    config: GClientConfig,
    /// The internal client.
    pub client: Arc<GClient>,
}

impl NpcControlClient {
    /// Connect to the server.
    pub async fn connect(config: GClientConfig) -> Result<Arc<Self>, ClientError> {
        let cloned_config = config.clone();
        let addr = format!("{}:{}", cloned_config.host, cloned_config.port);
        let stream = TcpStream::connect(&addr).await?;
        let (read_half, write_half) = stream.into_split();
        let reader = BufReader::new(read_half);

        let protocol = GProtocolEnum::V4(GProtocolV4::new(reader, write_half));
        let client = GClient::connect(cloned_config, protocol).await?;
        Ok(Arc::new(Self { config, client }))
    }

    /// Disconnect from the server and destroy all client tasks.
    pub async fn disconnect(&self) {
        self.client.shutdown().await;
    }

    /// Login to the server.
    pub async fn login(&self) -> Result<(), ClientError> {
        let login_packet = NcLogin::new(
            self.config.nc_protocol_version.clone(),
            self.config.login.username.clone(),
            self.config.login.password.clone(),
        );
        {
            log::debug!("Sending v4 login packet: {:?}", login_packet);
            self.client.send_packet(Arc::new(login_packet)).await?;
        }
        Ok(())
    }

    /// Query NC weapon.
    pub async fn nc_get_weapon(
        &self,
        weapon_name: String,
    ) -> Result<Arc<dyn GPacket>, ClientError> {
        let get_weapon_packet = NcWeaponGet::new(weapon_name);
        self.client
            .send_and_receive(Arc::new(get_weapon_packet), FromServerPacketId::NcWeaponGet)
            .await
    }

    /// Create a weapon.
    pub async fn nc_add_weapon(
        &self,
        weapon_name: String,
        weapon_img: String,
        weapon_script: String,
    ) -> Result<(), ClientError> {
        let add_weapon_packet = NcWeaponAdd::new(weapon_name, weapon_img, weapon_script);
        self.client.send_packet(Arc::new(add_weapon_packet)).await?;
        Ok(())
    }
}
