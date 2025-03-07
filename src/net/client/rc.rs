use std::sync::Arc;

use tokio::{io::BufReader, net::TcpStream, sync::Mutex, time::Instant};

use crate::{
    config::{ClientConfig, ClientType, RemoteControlConfig},
    net::{
        client::GClientTrait,
        packet::{
            GPacket,
            from_client::{nc_query::RcNcQuery, rc_login::RcLogin},
            from_server::{FromServerPacketId, nc_weapon_get::NcWeaponGetImpl},
        },
        protocol::{GProtocolEnum, proto_v5::GProtocolV5},
    },
};

use super::{
    ClientError,
    gclient::GClient,
    nc::{NpcControlClient, NpcControlClientTrait},
};

/// A struct that contains the RemoteControlClient.
pub struct RemoteControlClient {
    config: ClientConfig,
    rc_specific_config: RemoteControlConfig,
    /// The internal client.
    pub client: Arc<GClient>,
    npc_control: Mutex<Option<Arc<NpcControlClient>>>,
    last_activity: Mutex<Instant>,
}

/// The RemoteControlClient trait.
pub trait RemoteControlClientTrait {
    /// Query the NPC server address from the RC server.
    fn rc_query_nc_addr(&self) -> impl Future<Output = Result<Arc<dyn GPacket>, ClientError>>;
}

impl RemoteControlClient {
    /// Connect to the server.
    pub async fn connect(config: &ClientConfig) -> Result<Arc<Self>, ClientError> {
        let host = &config.host;
        let port = config.port;
        let addr = format!("{}:{}", host, port);
        let stream = TcpStream::connect(&addr).await?;
        let (read_half, write_half) = stream.into_split();
        let reader = BufReader::new(read_half);

        let protocol = GProtocolEnum::V5(GProtocolV5::new(reader, write_half));
        let client = GClient::connect(config, protocol).await?;

        let rc_specific_config = match &config.client_type {
            ClientType::RemoteControl(rc_config) => rc_config,
            _ => return Err(ClientError::UnsupportedProtocolVersion),
        };

        let client = Arc::new(Self {
            config: config.clone(),
            rc_specific_config: rc_specific_config.clone(),
            client,
            npc_control: Mutex::new(None),
            last_activity: Mutex::new(Instant::now()),
        });

        // Spawn the auto disconnect task.
        let disconnect_after = rc_specific_config.nc_auto_disconnect;
        let self_clone = Arc::clone(&client);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(disconnect_after).await;
                let should_disconnect = {
                    let last = self_clone.last_activity.lock().await;
                    last.elapsed() >= disconnect_after
                };
                if should_disconnect {
                    let maybe_npc = {
                        let mut npc_control_guard = self_clone.npc_control.lock().await;
                        npc_control_guard.take()
                    };
                    if let Some(npc) = maybe_npc {
                        log::info!("Auto-disconnecting from NPC server");
                        npc.disconnect().await;
                    }
                }
            }
        });
        Ok(client)
    }

    /// Login to the server.
    pub async fn login(&self) -> Result<(), ClientError> {
        let v5_code: u8 = rand::random::<u8>() & 0x7f;
        let login_packet = RcLogin::new(
            v5_code,
            self.rc_specific_config.version.clone(),
            self.config.login.auth.account_name.clone(),
            self.config.login.auth.password.clone(),
            self.config.login.auth.identification.clone(),
        );
        {
            log::debug!("Sending v5 login packet: {:?}", login_packet);
            self.send_packet(Arc::new(login_packet)).await?;
            self.client.set_codec(v5_code).await?;
        }
        Ok(())
    }

    /// Get the NPC control client (internal).
    async fn get_npc_control(&self) -> Result<Arc<NpcControlClient>, ClientError> {
        log::debug!("Checking if NPC control is set in config");
        let npc_control = self
            .rc_specific_config
            .npc_control
            .as_ref()
            .ok_or(ClientError::NcNotEnabled)?;
        log::debug!("Checking for existing NpcControlClient");
        let mut npc_control_guard = self.npc_control.lock().await;
        if npc_control_guard.is_none() {
            log::debug!("Creating new NpcControlClient");
            let mut config_clone = self.config.clone();

            let nc_addr_packet = self.rc_query_nc_addr().await?;
            log::debug!("Received NPC Server Address packet: {:?}", nc_addr_packet);

            let mut data: String = nc_addr_packet.data().iter().map(|&b| b as char).collect();
            // Adjust parsing as needed.
            data.remove(0);
            data.remove(0);
            let parts: Vec<&str> = data.split(',').collect();
            let npc_host = parts
                .first()
                .ok_or_else(|| ClientError::Other("Missing NPC host".into()))?;
            let npc_port = parts
                .get(1)
                .ok_or_else(|| ClientError::Other("Missing NPC port".into()))?
                .parse::<u16>()
                .map_err(|_| ClientError::Other("Invalid NPC port".into()))?;
            log::debug!("Parsed NPC server address: {}:{}", npc_host, npc_port);

            config_clone.host = npc_host.to_string();
            config_clone.port = npc_port;
            config_clone.client_type = ClientType::NpcControl(npc_control.clone());
            let npc_control = NpcControlClient::connect(&config_clone).await?;
            npc_control.login().await?;

            *npc_control_guard = Some(npc_control);
        }
        // Update last activity timestamp.
        *self.last_activity.lock().await = Instant::now();
        Ok(npc_control_guard.as_ref().unwrap().clone())
    }

    /// Calls `get_npc_control()` and then runs the provided async closure on the result.
    /// If the closure returns an error, the cached NPC control is cleared before returning
    /// the error (internal).
    async fn with_npc_control<T, F, Fut>(&self, f: F) -> Result<T, ClientError>
    where
        F: FnOnce(Arc<NpcControlClient>) -> Fut,
        Fut: std::future::Future<Output = Result<T, ClientError>>,
    {
        // Retrieve the NPC control instance.
        let npc_control = self.get_npc_control().await?;
        // Execute the closure.
        let result = f(npc_control).await;
        // If an error occurs, clear the cached NPC control.
        if result.is_err() {
            log::warn!("Clearing NPC control client due to error");
            let mut lock = self.npc_control.lock().await;
            *lock = None;
        }
        result
    }
}

impl RemoteControlClientTrait for RemoteControlClient {
    async fn rc_query_nc_addr(&self) -> Result<Arc<dyn GPacket>, ClientError> {
        let query_packet = RcNcQuery::new("location");
        self.send_and_receive(Arc::new(query_packet), FromServerPacketId::NpcServerAddr)
            .await
    }
}

impl NpcControlClientTrait for RemoteControlClient {
    async fn nc_get_weapon(&self, weapon_name: String) -> Result<NcWeaponGetImpl, ClientError> {
        self.with_npc_control(|npc| async move { npc.nc_get_weapon(weapon_name).await })
            .await
    }

    async fn nc_add_weapon(
        &self,
        weapon_name: String,
        weapon_img: String,
        weapon_script: String,
    ) -> Result<(), ClientError> {
        self.with_npc_control(|npc| async move {
            npc.nc_add_weapon(weapon_name, weapon_img, weapon_script)
                .await
        })
        .await
    }
}

impl GClientTrait for RemoteControlClient {
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
