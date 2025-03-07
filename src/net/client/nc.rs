use std::sync::Arc;

use tokio::{io::BufReader, net::TcpStream, sync::RwLock};

use crate::{
    config::{ClientConfig, ClientType, NpcControlConfig},
    net::{
        client::GClientTrait,
        packet::{
            GPacket, PacketEvent, PacketId,
            from_client::{
                nc_login::NcLogin, nc_weapon_add::NcWeaponAdd, nc_weapon_get::NcWeaponGet,
            },
            from_server::{FromServerPacketId, nc_weapon_get::NcWeaponGetImpl},
        },
        protocol::{GProtocolEnum, proto_v4::GProtocolV4},
    },
};

use super::{ClientError, gclient::GClient};
use std::future::Future;

/// A struct that contains the NpcControlClient.
pub struct NpcControlClient {
    config: ClientConfig,
    nc_specific_config: NpcControlConfig,
    /// The internal client.
    pub client: RwLock<Option<Arc<GClient>>>,
}

/// The NpcControlClient trait.
pub trait NpcControlClientTrait {
    /// Query NC weapon.
    fn nc_get_weapon(
        &self,
        weapon_name: String,
    ) -> impl Future<Output = Result<NcWeaponGetImpl, ClientError>>;
    /// Create a weapon.
    fn nc_add_weapon(
        &self,
        weapon_name: String,
        weapon_img: String,
        weapon_script: String,
    ) -> impl Future<Output = Result<(), ClientError>>;
}

impl NpcControlClient {
    /// Create a new NpcControlClient instance without connecting.
    pub fn new(config: &ClientConfig) -> Result<Arc<Self>, ClientError> {
        let nc_specific_config = match &config.client_type {
            ClientType::NpcControl(nc_config) => nc_config,
            _ => return Err(ClientError::UnsupportedProtocolVersion),
        };

        Ok(Arc::new(Self {
            config: config.clone(),
            nc_specific_config: nc_specific_config.clone(),
            client: RwLock::new(None),
        }))
    }

    /// Connect to the server.
    pub async fn connect(&self) -> Result<(), ClientError> {
        let host = &self.config.host;
        let port = self.config.port;
        let addr = format!("{}:{}", host, port);
        let stream = TcpStream::connect(&addr).await?;
        let (read_half, write_half) = stream.into_split();
        let reader = BufReader::new(read_half);

        let protocol = GProtocolEnum::V4(GProtocolV4::new(reader, write_half));
        let client = GClient::connect(&self.config, protocol).await?;

        let mut client_guard = self.client.write().await;
        *client_guard = Some(client);
        Ok(())
    }

    /// Get the internal client or return an error if not connected.
    async fn get_client(&self) -> Result<Arc<GClient>, ClientError> {
        let client_guard = self.client.read().await;
        match client_guard.as_ref() {
            Some(client) => Ok(Arc::clone(client)),
            None => Err(ClientError::ClientNotConnected),
        }
    }

    /// Login to the server.
    pub async fn login(&self) -> Result<(), ClientError> {
        let login_packet = NcLogin::new(
            self.nc_specific_config.version.clone(),
            self.config.login.auth.account_name.clone(),
            self.config.login.auth.password.clone(),
        );

        log::debug!("Sending v4 login packet: {:?}", login_packet);
        self.send_packet(Arc::new(login_packet)).await?;

        Ok(())
    }
}

impl NpcControlClientTrait for NpcControlClient {
    /// Query NC weapon.
    async fn nc_get_weapon(&self, weapon_name: String) -> Result<NcWeaponGetImpl, ClientError> {
        let get_weapon_packet = NcWeaponGet::new(weapon_name);
        let response = self
            .send_and_receive(Arc::new(get_weapon_packet), FromServerPacketId::NcWeaponGet)
            .await?;

        Ok(NcWeaponGetImpl::try_from(response)?)
    }

    /// Create a weapon.
    async fn nc_add_weapon(
        &self,
        weapon_name: String,
        weapon_img: String,
        weapon_script: String,
    ) -> Result<(), ClientError> {
        let add_weapon_packet = NcWeaponAdd::new(weapon_name, weapon_img, weapon_script);
        self.send_packet(Arc::new(add_weapon_packet)).await?;
        Ok(())
    }
}

impl GClientTrait for NpcControlClient {
    async fn disconnect(&self) -> Result<(), ClientError> {
        let client = self.get_client().await?;
        client.disconnect().await
    }

    async fn send_and_receive(
        &self,
        packet: Arc<dyn GPacket + Send>,
        response_packet: FromServerPacketId,
    ) -> Result<Arc<dyn GPacket>, ClientError> {
        let client = self.get_client().await?;
        client.send_and_receive(packet, response_packet).await
    }

    async fn send_packet(&self, packet: Arc<dyn GPacket + Send>) -> Result<(), ClientError> {
        let client = self.get_client().await?;
        client.send_packet(packet).await
    }

    async fn register_event_handler<F, Fut>(
        &self,
        packet_id: PacketId,
        handler: F,
    ) -> Result<(), ClientError>
    where
        F: Fn(PacketEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let client = self.get_client().await?;
        client.register_event_handler(packet_id, handler).await
    }

    async fn register_disconnect_handler<F, Fut>(&self, handler: F) -> Result<(), ClientError>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let client = self.get_client().await?;
        client.register_disconnect_handler(handler).await
    }

    async fn wait_for_tasks(&self) -> Result<(), ClientError> {
        let client = self.get_client().await?;
        client.wait_for_tasks().await
    }
}
