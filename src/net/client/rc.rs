use std::sync::{Arc, Weak};

use tokio::{
    io::{AsyncWriteExt, BufReader},
    net::TcpStream,
    sync::{Mutex, RwLock},
    task::JoinHandle,
    time::{Duration, Instant},
};

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
    pub client: RwLock<Option<Arc<GClient>>>,
    npc_control: RwLock<Option<Arc<NpcControlClient>>>,
    last_activity: Mutex<Instant>,
    auto_disconnect_task: Mutex<Option<JoinHandle<()>>>,
}

/// The RemoteControlClient trait.
pub trait RemoteControlClientTrait {
    /// Query the NPC server address from the RC server.
    fn rc_query_nc_addr(&self) -> impl Future<Output = Result<Arc<dyn GPacket>, ClientError>>;
}

impl RemoteControlClient {
    /// Connect to the server.
    pub async fn new(config: &ClientConfig) -> Result<Arc<Self>, ClientError> {
        let rc_specific_config = match &config.client_type {
            ClientType::RemoteControl(rc_config) => rc_config,
            _ => return Err(ClientError::UnsupportedProtocolVersion),
        };

        let client = Arc::new(Self {
            config: config.clone(),
            rc_specific_config: rc_specific_config.clone(),
            client: RwLock::new(None),
            npc_control: RwLock::new(None),
            last_activity: Mutex::new(Instant::now()),
            auto_disconnect_task: Mutex::new(None),
        });

        // Start the auto-disconnect task
        client.start_auto_disconnect_task().await;

        Ok(client)
    }

    /// Start the auto-disconnect task
    async fn start_auto_disconnect_task(&self) {
        let disconnect_after = self.rc_specific_config.nc_auto_disconnect;
        let self_weak = Arc::downgrade(&Arc::new(self.clone()));

        let task = tokio::spawn(async move {
            while let Some(self_ref) = Self::upgrade_weak(&self_weak) {
                tokio::time::sleep(Duration::from_secs(5)).await; // Check every 5 seconds

                let should_disconnect = {
                    let last = self_ref.last_activity.lock().await;
                    last.elapsed() >= disconnect_after
                };

                if should_disconnect {
                    let maybe_npc = {
                        let mut npc_control = self_ref.npc_control.write().await;
                        npc_control.take()
                    };

                    if let Some(npc) = maybe_npc {
                        log::info!("Auto-disconnecting from NPC server");
                        let _ = npc.disconnect().await;
                    }
                }
            }
        });

        let mut auto_disconnect_guard = self.auto_disconnect_task.lock().await;
        *auto_disconnect_guard = Some(task);
    }

    // Helper to safely upgrade a weak reference
    fn upgrade_weak(weak_ref: &Weak<Self>) -> Option<Arc<Self>> {
        weak_ref.upgrade()
    }

    /// Reconnect to the server.
    pub async fn connect(&self) -> Result<(), ClientError> {
        log::debug!("Attempting to connect to server");
        let host = &self.config.host;
        let port = &self.config.port;
        let addr = format!("{}:{}", host, port);
        log::debug!("Attempting to connect to address: {}", addr);
        let stream = TcpStream::connect(&addr).await?;
        log::debug!("Connected to server");
        let (read_half, write_half) = stream.into_split();
        let reader = BufReader::new(read_half);

        let protocol = GProtocolEnum::V5(GProtocolV5::new(reader, write_half));
        log::debug!("Creating GClient");
        let client = GClient::connect(&self.config, protocol).await?;

        log::debug!("Locking on client write lock");
        let mut client_guard = self.client.write().await;
        *client_guard = Some(client);
        log::debug!("Unlocked client write lock");
        Ok(())
    }

    /// Login to the server.
    pub async fn login(&self) -> Result<(), ClientError> {
        // Get the client reference without holding the lock during the operation
        let client = {
            let client_guard = self.client.read().await;
            match client_guard.as_ref() {
                Some(client) => Arc::clone(client),
                None => return Err(ClientError::ClientNotConnected),
            }
        };

        let v5_code: u8 = rand::random::<u8>() & 0x7f;
        let login_packet = RcLogin::new(
            v5_code,
            self.rc_specific_config.version.clone(),
            self.config.login.auth.account_name.clone(),
            self.config.login.auth.password.clone(),
            self.config.login.auth.identification.clone(),
        );

        log::debug!("Sending v5 login packet: {:?}", login_packet);
        self.send_packet(Arc::new(login_packet)).await?;
        client.set_codec(v5_code).await?;

        Ok(())
    }

    /// Get the NPC control client (internal).
    async fn get_npc_control(&self) -> Result<Arc<NpcControlClient>, ClientError> {
        // Update last activity timestamp first
        {
            let mut last_activity = self.last_activity.lock().await;
            *last_activity = Instant::now();
        }

        // Check if NPC control is enabled in config
        log::debug!("Checking if NPC control is set in config");
        let npc_control_config = self
            .rc_specific_config
            .npc_control
            .as_ref()
            .ok_or(ClientError::NcNotEnabled)?;

        // First check if we already have an NPC control client
        {
            let npc_control_guard = self.npc_control.read().await;
            if let Some(npc) = npc_control_guard.as_ref() {
                return Ok(Arc::clone(npc));
            }
        }

        // We need to create a new NPC control client
        log::debug!("Creating new NpcControlClient");

        // Query the NPC server address without holding the lock
        let nc_addr_packet = self.rc_query_nc_addr().await?;
        log::debug!("Received NPC Server Address packet: {:?}", nc_addr_packet);

        // Parse the address
        let mut data: String = nc_addr_packet.data().iter().map(|&b| b as char).collect();
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

        // Create the NPC client config
        let mut config_clone = self.config.clone();
        config_clone.host = npc_host.to_string();
        config_clone.port = npc_port;
        config_clone.client_type = ClientType::NpcControl(npc_control_config.clone());

        // Connect and login to the NPC server
        let npc_control = NpcControlClient::new(&config_clone)?;
        npc_control.connect().await?;
        npc_control.login().await?;

        // Store the new NPC control client, but check again if another thread already created one
        let mut npc_control_guard = self.npc_control.write().await;
        if npc_control_guard.is_none() {
            *npc_control_guard = Some(Arc::clone(&npc_control));
        }

        Ok(npc_control)
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

        // If an error occurs that indicates a connection issue, clear the cached NPC control.
        if let Err(ref err) = result {
            log::warn!(
                "Clearing NPC control client due to connection error: {:?}",
                err
            );
            let mut lock = self.npc_control.write().await;
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
    async fn disconnect(&self) -> Result<(), ClientError> {
        // Get the client without holding the lock during the operation
        let client = {
            let client_guard = self.client.read().await;
            match client_guard.as_ref() {
                Some(client) => Arc::clone(client),
                None => return Err(ClientError::ClientNotConnected),
            }
        };

        // Also disconnect NPC client if it exists
        {
            let npc_control = {
                let npc_guard = self.npc_control.read().await;
                npc_guard.as_ref().map(Arc::clone)
            };

            if let Some(npc) = npc_control {
                let _ = npc.disconnect().await;
            }
        }

        // Cancel auto-disconnect task
        {
            let mut task_guard = self.auto_disconnect_task.lock().await;
            if let Some(task) = task_guard.take() {
                task.abort();
            }
        }

        // Disconnect the main client
        client.disconnect().await
    }

    async fn send_and_receive(
        &self,
        packet: Arc<dyn GPacket + Send>,
        response_packet: FromServerPacketId,
    ) -> Result<Arc<dyn GPacket>, ClientError> {
        // Get the client without holding the lock during the operation
        let client = {
            let client_guard = self.client.read().await;
            match client_guard.as_ref() {
                Some(client) => Arc::clone(client),
                None => return Err(ClientError::ClientNotConnected),
            }
        };

        client.send_and_receive(packet, response_packet).await
    }

    async fn send_packet(&self, packet: Arc<dyn GPacket + Send>) -> Result<(), ClientError> {
        // Get the client without holding the lock during the operation
        let client = {
            let client_guard = self.client.read().await;
            match client_guard.as_ref() {
                Some(client) => Arc::clone(client),
                None => return Err(ClientError::ClientNotConnected),
            }
        };

        client.send_packet(packet).await
    }

    async fn register_event_handler<F, Fut>(
        &self,
        packet_id: crate::net::packet::PacketId,
        handler: F,
    ) -> Result<(), ClientError>
    where
        F: Fn(crate::net::packet::PacketEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        // Get the client without holding the lock during the operation
        let client = {
            let client_guard = self.client.read().await;
            match client_guard.as_ref() {
                Some(client) => Arc::clone(client),
                None => return Err(ClientError::ClientNotConnected),
            }
        };

        client.register_event_handler(packet_id, handler).await
    }

    async fn wait_for_tasks(&self) -> Result<(), ClientError> {
        // Get the client without holding the lock during the operation
        let client = {
            let client_guard = self.client.read().await;
            match client_guard.as_ref() {
                Some(client) => Arc::clone(client),
                None => return Err(ClientError::ClientNotConnected),
            }
        };

        client.wait_for_tasks().await
    }

    async fn register_disconnect_handler<F, Fut>(&self, handler: F) -> Result<(), ClientError>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        // Get the client without holding the lock during the operation
        let client = {
            let client_guard = self.client.read().await;
            match client_guard.as_ref() {
                Some(client) => Arc::clone(client),
                None => return Err(ClientError::ClientNotConnected),
            }
        };

        client.register_disconnect_handler(handler).await
    }
}

// Add Clone implementation for the auto-disconnect task
impl Clone for RemoteControlClient {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            rc_specific_config: self.rc_specific_config.clone(),
            client: RwLock::new(None),
            npc_control: RwLock::new(None),
            last_activity: Mutex::new(Instant::now()),
            auto_disconnect_task: Mutex::new(None),
        }
    }
}
