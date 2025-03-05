use std::sync::Arc;

use tokio::{io::BufReader, net::TcpStream};

use crate::{
    config::{ClientConfig, ClientType, GameConfig},
    net::{
        client::GClientTrait,
        packet::{GPacket, from_client::game_login::GameLogin, from_server::FromServerPacketId},
        protocol::{GProtocolEnum, proto_v6::GProtocolV6},
    },
};

use super::{ClientError, gclient::GClient};

/// A struct that contains the GameClient.
pub struct GameClient {
    config: ClientConfig,
    game_specific_config: GameConfig,
    /// The internal client.
    pub client: Arc<GClient>,
}

/// The GameClientTrait trait.
pub trait GameClientTrait {}

impl GameClient {
    /// Connect to the server.
    pub async fn connect(config: &ClientConfig) -> Result<Arc<Self>, ClientError> {
        let host = &config.host;
        let port = config.port;
        let addr = format!("{}:{}", host, port);
        let stream = TcpStream::connect(&addr).await?;
        let (read_half, write_half) = stream.into_split();
        let reader = BufReader::new(read_half);

        let game_specific_config = match &config.client_type {
            ClientType::Game(config) => config,
            _ => return Err(ClientError::UnsupportedProtocolVersion),
        };

        let protocol = GProtocolEnum::V6(GProtocolV6::new(
            reader,
            write_half,
            game_specific_config.header_format.clone(),
            game_specific_config.encryption_keys.clone(),
        ));
        let client = GClient::connect(config, protocol).await?;

        Ok(Arc::new(Self {
            config: config.clone(),
            game_specific_config: game_specific_config.clone(),
            client,
        }))
    }

    /// Login to the server.
    pub async fn login(&self) -> Result<(), ClientError> {
        let login_packet = GameLogin::new(
            self.game_specific_config.version.clone(),
            self.config.login.auth.account_name.clone(),
            self.config.login.auth.password.clone(),
            self.config.login.auth.identification.clone(),
        );
        {
            log::debug!("Sending v6 login packet: {:?}", login_packet);
            self.send_packet(Arc::new(login_packet)).await?;
        }
        Ok(())
    }
}

impl GClientTrait for GameClient {
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
