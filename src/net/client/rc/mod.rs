#![deny(missing_docs)]

use config::RcConfig;
use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};
use tokio::{
    io::BufReader,
    net::TcpStream,
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::{Mutex, Notify, mpsc, oneshot},
    time::timeout,
};

use crate::{
    consts::{NC_PROTOCOL_VERSION, RC_PROTOCOL_VERSION},
    net::{
        packet::{
            GPacket, PacketEvent, PacketId,
            from_client::{
                nc_login::NcLogin, nc_query::RcNcQuery, nc_weapon_add::NcWeaponAdd,
                nc_weapon_get::NcWeaponGet, rc_login::RcLogin,
            },
            from_server::FromServerPacketId,
        },
        protocol::{GProtocolEnum, Protocol, proto_v4::GProtocolV4, proto_v5::GProtocolV5},
    },
};

/// The config module contains the configuration for the RemoteControl client.
pub mod config;

use super::ClientError;

/// An asynchronous event handler function that receives a PacketEvent.
pub type EventHandlerFn =
    dyn Fn(PacketEvent) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync;

/// A struct representing the NPC control.
pub struct NpcControl {
    /// Channel for sending outgoing NC packets.
    outgoing_tx: mpsc::Sender<Arc<dyn GPacket + Send>>,
    /// Shutdown notifier for the NPC side.
    shutdown: Arc<Notify>,
}

type PendingRequests = Arc<Mutex<HashMap<PacketId, oneshot::Sender<Arc<dyn GPacket>>>>>;

/// RemoteControl is a high-level client for communicating with the RC server,
/// and (optionally) automatically managing an NPC connection.
pub struct RemoteControl {
    /// The underlying RC protocol implementation (v5).
    rc_protocol: Arc<Mutex<GProtocolEnum<BufReader<OwnedReadHalf>, OwnedWriteHalf>>>,
    /// Channel for sending outgoing RC packets.
    rc_outgoing_tx: mpsc::Sender<Arc<dyn GPacket + Send>>,
    /// Mapping from expected packet type (PacketId) to pending response channels.
    pending_requests: PendingRequests,
    /// Mapping from PacketId to registered event handlers.
    event_handlers: Arc<Mutex<HashMap<PacketId, Vec<Arc<EventHandlerFn>>>>>,
    /// Client configuration.
    config: RcConfig,
    /// Shutdown notifier for RC tasks.
    rc_shutdown: Arc<Notify>,
    /// The underlying NC protocol implementation (v4) if connected.
    npc_control: Arc<Mutex<Option<NpcControl>>>,
}

impl RemoteControl {
    /// Connect to the RC server using the provided configuration.
    ///
    /// This spawns background tasks for reading and sending RC packets,
    /// and returns an Arc-wrapped RemoteControl.
    pub async fn connect(config: RcConfig) -> Result<Arc<Self>, ClientError> {
        let addr = format!("{}:{}", config.host, config.port);
        let stream = TcpStream::connect(&addr).await?;
        let (read_half, write_half) = stream.into_split();
        let reader = BufReader::new(read_half);
        let protocol = GProtocolEnum::V5(GProtocolV5::new(reader, write_half));
        let protocol = Arc::new(Mutex::new(protocol));

        // Outgoing RC channel.
        let (rc_outgoing_tx, rc_outgoing_rx) = mpsc::channel::<Arc<dyn GPacket + Send>>(100);
        let pending_requests = Arc::new(Mutex::new(HashMap::new()));
        let event_handlers = Arc::new(Mutex::new(HashMap::new()));
        let rc_shutdown = Arc::new(Notify::new());

        let rc = Arc::new(RemoteControl {
            rc_protocol: Arc::clone(&protocol),
            rc_outgoing_tx,
            pending_requests: Arc::clone(&pending_requests),
            event_handlers: Arc::clone(&event_handlers),
            config,
            rc_shutdown: Arc::clone(&rc_shutdown),
            npc_control: Arc::new(Mutex::new(None)),
        });

        // Spawn RC send and read loops.
        Self::spawn_send_loop(
            Arc::clone(&protocol),
            rc_outgoing_rx,
            Arc::clone(&rc_shutdown),
        );
        Self::spawn_read_loop(
            Arc::clone(&protocol),
            Arc::clone(&pending_requests),
            Arc::clone(&event_handlers),
            Arc::clone(&rc_shutdown),
        );

        Ok(rc)
    }

    /// Spawns a task that continuously sends packets from the given channel.
    fn spawn_send_loop(
        protocol: Arc<Mutex<GProtocolEnum<BufReader<OwnedReadHalf>, OwnedWriteHalf>>>,
        mut outgoing_rx: mpsc::Receiver<Arc<dyn GPacket + Send>>,
        shutdown: Arc<Notify>,
    ) {
        tokio::spawn(async move {
            while let Some(packet) = outgoing_rx.recv().await {
                let mut proto = protocol.lock().await;
                log::debug!(
                    "Sending RC packet: {:?}",
                    String::from_utf8_lossy(&packet.data())
                );
                if let Err(e) = proto.write(packet.as_ref()).await {
                    log::error!("Failed to send RC packet: {:?}", e);
                    shutdown.notify_waiters();
                    break;
                }
            }
            log::error!("Outgoing RC channel closed, terminating send loop.");
            shutdown.notify_waiters();
        });
    }

    /// Spawns a task that continuously reads packets from the protocol.
    fn spawn_read_loop(
        protocol: Arc<Mutex<GProtocolEnum<BufReader<OwnedReadHalf>, OwnedWriteHalf>>>,
        pending_requests: PendingRequests,
        event_handlers: Arc<Mutex<HashMap<PacketId, Vec<Arc<EventHandlerFn>>>>>,
        shutdown: Arc<Notify>,
    ) {
        tokio::spawn(async move {
            loop {
                let packet_result = {
                    let mut proto = protocol.lock().await;
                    proto.read().await
                };
                match packet_result {
                    Ok(packet) => {
                        let packet_id = packet.id();
                        let mut pending = pending_requests.lock().await;
                        if let Some(sender) = pending.remove(&packet_id) {
                            let _ = sender.send(packet.clone());
                        } else {
                            log::debug!("Received unsolicited RC packet: {:?}", packet);
                            let event = PacketEvent {
                                packet: packet.clone(),
                            };
                            let handlers = {
                                let map = event_handlers.lock().await;
                                map.get(&packet_id).cloned()
                            };
                            if let Some(handler_vec) = handlers {
                                for handler in handler_vec {
                                    let event_clone = event.clone();
                                    tokio::spawn(async move {
                                        (handler)(event_clone).await;
                                    });
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Error reading RC packet: {:?}", e);
                        shutdown.notify_waiters();
                        break;
                    }
                }
            }
        });
    }

    /// Send a raw RC packet.
    pub async fn send_rc_packet(&self, packet: Arc<dyn GPacket + Send>) -> Result<(), ClientError> {
        self.rc_outgoing_tx
            .send(packet)
            .await
            .map_err(ClientError::Send)
    }

    /// Send a raw NC packet.
    pub async fn send_nc_packet(&self, packet: Arc<dyn GPacket + Send>) -> Result<(), ClientError> {
        self.ensure_npc_control().await?;
        let npc_lock = self.npc_control.lock().await;
        if let Some(npc) = &*npc_lock {
            npc.outgoing_tx
                .send(packet)
                .await
                .map_err(ClientError::Send)
        } else {
            Err(ClientError::Other("NPC control not connected".into()))
        }
    }

    /// Send a login packet to the RC server.
    pub async fn rc_login(&self) -> Result<(), ClientError> {
        let encryption_key: u8 = rand::random::<u8>() & 0x7f;
        let login_packet = RcLogin::new(
            encryption_key,
            RC_PROTOCOL_VERSION,
            &self.config.login.username,
            &self.config.login.password,
            self.config.login.identification.clone(),
        );
        {
            let mut proto = self.rc_protocol.lock().await;
            log::debug!("Sending RC login packet: {:?}", login_packet);

            proto.write(&login_packet).await?;
            if let GProtocolEnum::V5(proto) = &mut *proto {
                proto.set_encryption_key(encryption_key);
            } else {
                log::warn!("RcLogin used on non-v5 protocol");
            }
        }
        Ok(())
    }

    /// Query the NPC server address from the RC server.
    pub async fn query_nc_addr(&self) -> Result<Arc<dyn GPacket>, ClientError> {
        let query_packet = RcNcQuery::new("location");
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(PacketId::FromServer(FromServerPacketId::NpcServerAddr), tx);
        }
        self.send_rc_packet(Arc::new(query_packet)).await?;
        let packet = timeout(self.config.timeout, rx)
            .await
            .map_err(ClientError::Timeout)?
            .map_err(ClientError::Recv)?;
        Ok(packet)
    }

    /// Register an event handler for a specific PacketId.
    pub async fn register_event_handler<F, Fut>(&self, packet_id: PacketId, handler: F)
    where
        F: Fn(PacketEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let mut map = self.event_handlers.lock().await;
        let entry = map.entry(packet_id).or_insert_with(Vec::new);
        entry.push(Arc::new(move |event: PacketEvent| {
            Box::pin(handler(event)) as Pin<Box<dyn Future<Output = ()> + Send>>
        }) as Arc<EventHandlerFn>);
    }

    /// Block until the RC connection is shutdown.
    pub async fn wait_shutdown(&self) {
        self.rc_shutdown.notified().await;
    }

    /// Ensure that the NPC control is connected.
    ///
    /// This method queries the NPC server address, creates an NPC control using the v4 protocol,
    /// logs in, and caches the connection. It also spawns tasks for handling NC packets,
    /// auto-disconnecting after a timeout, and cleaning up on error.
    pub async fn ensure_npc_control(&self) -> Result<(), ClientError> {
        {
            let npc_lock = self.npc_control.lock().await;
            if npc_lock.is_some() {
                return Ok(());
            }
        }

        // Query NPC server address.
        let npc_packet = self.query_nc_addr().await?;
        // Parse the NPC address.
        let mut data: String = npc_packet.data().iter().map(|&b| b as char).collect();
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

        // Connect to the NPC server using V4.
        let addr = format!("{}:{}", npc_host, npc_port);
        let stream = TcpStream::connect(&addr).await?;
        let (read_half, write_half) = stream.into_split();
        let reader = BufReader::new(read_half);
        let mut nc_protocol = GProtocolEnum::V4(GProtocolV4::new(reader, write_half));

        // Send login packet to NC server.
        let login_packet = NcLogin::new(
            NC_PROTOCOL_VERSION,
            &self.config.login.username,
            &self.config.login.password,
        );
        nc_protocol.write(&login_packet).await?;
        let protocol = Arc::new(Mutex::new(nc_protocol));

        // Create NC outgoing channel and shutdown notifier.
        let (nc_outgoing_tx, nc_outgoing_rx) = mpsc::channel::<Arc<dyn GPacket + Send>>(100);
        let nc_shutdown = Arc::new(Notify::new());
        let npc_control = NpcControl {
            outgoing_tx: nc_outgoing_tx,
            shutdown: Arc::clone(&nc_shutdown),
        };
        Self::spawn_send_loop(
            Arc::clone(&protocol),
            nc_outgoing_rx,
            Arc::clone(&nc_shutdown),
        );
        Self::spawn_read_loop(
            Arc::clone(&protocol),
            Arc::clone(&self.pending_requests),
            Arc::clone(&self.event_handlers),
            Arc::clone(&nc_shutdown),
        );
        // Spawn auto-disconnect timer.
        {
            let npc_control_handle = Arc::clone(&self.npc_control);
            let auto_timeout = self.config.nc_auto_disconnect;
            tokio::spawn(async move {
                tokio::time::sleep(auto_timeout).await;
                let mut lock = npc_control_handle.lock().await;
                if lock.is_some() {
                    log::info!("Auto-disconnecting NPC control after timeout.");
                    *lock = None;
                }
            });
        }

        // Spawn NPC Control cleanup on shutdown.
        {
            let npc_control_handle = Arc::clone(&self.npc_control);
            let nc_shutdown = Arc::clone(&npc_control.shutdown);
            tokio::spawn(async move {
                nc_shutdown.notified().await;
                let mut lock = npc_control_handle.lock().await;
                if lock.as_ref().is_some() {
                    *lock = None;
                }
            });
        }

        // Cache the NPC control.
        {
            let mut lock = self.npc_control.lock().await;
            *lock = Some(npc_control);
        }

        Ok(())
    }

    /// Query NC weapon.
    pub async fn nc_get_weapon(
        &self,
        weapon_name: String,
    ) -> Result<Arc<dyn GPacket>, ClientError> {
        let get_weapon_packet = NcWeaponGet::new(weapon_name);
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(PacketId::FromServer(FromServerPacketId::NcWeaponGet), tx);
        }
        self.send_nc_packet(Arc::new(get_weapon_packet)).await?;
        let packet = timeout(self.config.timeout, rx)
            .await
            .map_err(ClientError::Timeout)?
            .map_err(ClientError::Recv)?;
        Ok(packet)
    }

    /// Create a weapon.
    pub async fn nc_add_weapon(
        &self,
        weapon_name: String,
        weapon_img: String,
        weapon_script: String,
    ) -> Result<(), ClientError> {
        let add_weapon_packet = NcWeaponAdd::new(weapon_name, weapon_img, weapon_script);
        self.send_nc_packet(Arc::new(add_weapon_packet)).await?;
        Ok(())
    }
}
