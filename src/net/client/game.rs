use std::sync::Arc;

use tokio::{io::BufReader, net::TcpStream, sync::RwLock};

use crate::{
    config::{ClientConfig, ClientType, GameConfig},
    net::{
        client::GClientTrait,
        packet::{
            GPacket, PacketEvent, PacketId, from_client::game_login::GameLogin,
            from_server::FromServerPacketId,
        },
        protocol::{GProtocolEnum, proto_v6::GProtocolV6},
    },
};

use super::{ClientError, gclient::GClient};
use std::future::Future;

/// A struct that contains the GameClient.
pub struct GameClient {
    config: ClientConfig,
    game_specific_config: GameConfig,
    /// The internal client.
    pub client: RwLock<Option<Arc<GClient>>>,
}

/// The GameClientTrait trait.
pub trait GameClientTrait {}

impl GameClient {
    /// Create a new GameClient instance without connecting.
    pub fn new(config: &ClientConfig) -> Result<Arc<Self>, ClientError> {
        let game_specific_config = match &config.client_type {
            ClientType::Game(config) => config,
            _ => return Err(ClientError::UnsupportedProtocolVersion),
        };

        Ok(Arc::new(Self {
            config: config.clone(),
            game_specific_config: game_specific_config.clone(),
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

        let protocol = GProtocolEnum::V6(GProtocolV6::new(
            reader,
            write_half,
            self.game_specific_config.header_format.clone(),
            self.game_specific_config.encryption_keys.clone(),
        ));

        let client = GClient::connect(&self.config, protocol).await?;

        let mut client_guard = self.client.write().await;
        *client_guard = Some(client);

        Ok(())
    }

    /// Static method to create and connect in one step.
    pub async fn create_and_connect(config: &ClientConfig) -> Result<Arc<Self>, ClientError> {
        let client = Self::new(config)?;
        client.connect().await?;
        Ok(client)
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
        let login_packet = GameLogin::new(
            self.game_specific_config.version.clone(),
            self.config.login.auth.account_name.clone(),
            self.config.login.auth.password.clone(),
            self.config.login.auth.identification.clone(),
        );

        log::debug!("Sending v6 login packet: {:?}", login_packet);
        self.send_packet(Arc::new(login_packet)).await?;

        Ok(())
    }
}

impl GameClientTrait for GameClient {}

impl GClientTrait for GameClient {
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
