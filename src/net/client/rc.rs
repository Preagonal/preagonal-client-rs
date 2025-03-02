use std::sync::Arc;

use tokio::{io::BufReader, net::TcpStream, sync::Mutex, time::Instant};

use crate::net::{
    packet::{
        GPacket,
        from_client::{nc_query::RcNcQuery, rc_login::RcLogin},
        from_server::FromServerPacketId,
    },
    protocol::{GProtocolEnum, proto_v5::GProtocolV5},
};

use super::{ClientError, GClientConfig, gclient::GClient, nc::NpcControlClient};

/// A struct that contains the RemoteControlClient.
pub struct RemoteControlClient {
    config: GClientConfig,
    /// The internal client.
    pub client: Arc<GClient>,
    cached_npc_server_address: Mutex<Option<(String, u16)>>,
    npc_control: Mutex<Option<Arc<NpcControlClient>>>,
    last_activity: Mutex<Instant>,
}

impl RemoteControlClient {
    /// Connect to the server.
    pub async fn connect(config: GClientConfig) -> Result<Arc<Self>, ClientError> {
        let cloned_config = config.clone();
        let addr = format!("{}:{}", cloned_config.host, cloned_config.port);
        let stream = TcpStream::connect(&addr).await?;
        let (read_half, write_half) = stream.into_split();
        let reader = BufReader::new(read_half);

        let protocol = GProtocolEnum::V5(GProtocolV5::new(reader, write_half));
        let client = GClient::connect(cloned_config, protocol).await?;

        let client = Arc::new(Self {
            config: config.clone(),
            client,
            npc_control: Mutex::new(None),
            cached_npc_server_address: Mutex::new(None),
            last_activity: Mutex::new(Instant::now()),
        });

        // Spawn the auto disconnect task.
        let disconnect_after = config.nc_auto_disconnect;
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

    /// Disconnect from the server and destroy all client tasks.
    pub async fn disconnect(&self) {
        self.client.shutdown().await;
    }

    /// Login to the server.
    pub async fn login(&self) -> Result<(), ClientError> {
        let v5_code: u8 = rand::random::<u8>() & 0x7f;
        let login_packet = RcLogin::new(
            v5_code,
            self.config.rc_protocol_version.clone(),
            self.config.login.username.clone(),
            self.config.login.password.clone(),
            self.config.login.identification.clone(),
        );
        {
            log::debug!("Sending v5 login packet: {:?}", login_packet);
            self.client.send_packet(Arc::new(login_packet)).await?;
            self.client.set_codec(v5_code).await?;
        }
        Ok(())
    }

    /// Get the NPC control client.
    pub async fn get_npc_control(&self) -> Result<Arc<NpcControlClient>, ClientError> {
        log::debug!("Getting NPC control client");
        // Get the cached NPC server address.
        let mut cached_npc_server_address_guard = self.cached_npc_server_address.lock().await;
        if cached_npc_server_address_guard.is_none() {
            let nc_addr_packet = self.query_nc_addr().await?;
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
            *cached_npc_server_address_guard = Some((npc_host.to_string(), npc_port));
        }

        log::debug!("Checking for existing NpcControlClient");
        let mut npc_control_guard = self.npc_control.lock().await;
        if npc_control_guard.is_none() {
            log::debug!("Creating new NpcControlClient");
            let mut config_clone = self.config.clone();
            let (npc_host, npc_port) = cached_npc_server_address_guard.as_ref().unwrap();

            config_clone.host = npc_host.clone();
            config_clone.port = *npc_port;
            let npc_control = NpcControlClient::connect(config_clone).await?;
            npc_control.login().await?;

            *npc_control_guard = Some(npc_control);
        }
        // Update last activity timestamp.
        *self.last_activity.lock().await = Instant::now();
        Ok(npc_control_guard.as_ref().unwrap().clone())
    }

    /// Query the NPC server address from the RC server.
    pub async fn query_nc_addr(&self) -> Result<Arc<dyn GPacket>, ClientError> {
        let query_packet = RcNcQuery::new("location");
        self.client
            .send_and_receive(Arc::new(query_packet), FromServerPacketId::NpcServerAddr)
            .await
    }
}
