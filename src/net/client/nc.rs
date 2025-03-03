use std::sync::Arc;

use tokio::{io::BufReader, net::TcpStream};

use crate::net::{
    client::GClientTrait,
    packet::{
        GPacket,
        from_client::{nc_login::NcLogin, nc_weapon_add::NcWeaponAdd, nc_weapon_get::NcWeaponGet},
        from_server::FromServerPacketId,
    },
    protocol::{GProtocolEnum, proto_v4::GProtocolV4},
};

use super::{ClientError, GClientConfig, GClientConfigType, NpcControlConfig, gclient::GClient};

/// A struct that contains the NpcControlClient.
pub struct NpcControlClient {
    config: GClientConfig,
    nc_specific_config: NpcControlConfig,
    /// The internal client.
    pub client: Arc<GClient>,
}

/// The NpcControlClient trait.
pub trait NpcControlClientTrait {
    /// Query NC weapon.
    fn nc_get_weapon(
        &self,
        weapon_name: String,
    ) -> impl Future<Output = Result<Arc<dyn GPacket>, ClientError>>;
    /// Create a weapon.
    fn nc_add_weapon(
        &self,
        weapon_name: String,
        weapon_img: String,
        weapon_script: String,
    ) -> impl Future<Output = Result<(), ClientError>>;
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

        let nc_specific_config = match &config.client_type {
            GClientConfigType::NpcControl(nc_config) => nc_config,
            _ => return Err(ClientError::UnsupportedProtocolVersion),
        };

        Ok(Arc::new(Self {
            config: config.clone(),
            nc_specific_config: nc_specific_config.clone(),
            client,
        }))
    }

    /// Login to the server.
    pub async fn login(&self) -> Result<(), ClientError> {
        let login_packet = NcLogin::new(
            self.nc_specific_config.version.clone(),
            self.config.login.username.clone(),
            self.config.login.password.clone(),
        );
        {
            log::debug!("Sending v4 login packet: {:?}", login_packet);
            self.send_packet(Arc::new(login_packet)).await?;
        }
        Ok(())
    }
}

impl NpcControlClientTrait for NpcControlClient {
    /// Query NC weapon.
    async fn nc_get_weapon(&self, weapon_name: String) -> Result<Arc<dyn GPacket>, ClientError> {
        let get_weapon_packet = NcWeaponGet::new(weapon_name);
        self.client
            .send_and_receive(Arc::new(get_weapon_packet), FromServerPacketId::NcWeaponGet)
            .await
    }

    /// Create a weapon.
    async fn nc_add_weapon(
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

impl GClientTrait for NpcControlClient {
    async fn disconnect(&self) {
        self.client.disconnect().await
    }

    async fn send_and_receive(
        &self,
        packet: Arc<dyn GPacket + Send>,
        response_packet: FromServerPacketId,
    ) -> Result<Arc<dyn GPacket>, ClientError> {
        self.client.send_and_receive(packet, response_packet).await
    }

    async fn send_packet(&self, packet: Arc<dyn GPacket + Send>) -> Result<(), ClientError> {
        self.client.send_packet(packet).await
    }

    async fn register_event_handler<F, Fut>(
        &self,
        packet_id: crate::net::packet::PacketId,
        handler: F,
    ) where
        F: Fn(crate::net::packet::PacketEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.client.register_event_handler(packet_id, handler).await
    }

    async fn wait_for_tasks(&self) {
        self.client.wait_for_tasks().await
    }
}
