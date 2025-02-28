#![deny(missing_docs)]

use config::NcConfig;
use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::{
    io::BufReader,
    net::TcpStream,
    sync::{Mutex, Notify, broadcast, mpsc, oneshot},
    time::timeout,
};

use crate::consts::NC_PROTOCOL_VERSION;
use crate::net::packet::from_client::nc_login::NcLogin;
use crate::net::packet::from_client::nc_weapon_add::NcWeaponAdd;
use crate::net::packet::from_client::nc_weapon_get::NcWeaponGet;
use crate::net::packet::{GPacket, PacketEvent, PacketId, from_server::FromServerPacketId};
use crate::net::protocol::Protocol;
use crate::net::protocol::proto_v4::GProtocolV4;

use super::ClientError;

/// Describes RcConfig
pub mod config;

/// An asynchronous event handler function that receives the RC context and a PacketEvent.
pub type EventHandlerFn =
    dyn Fn(Arc<NpcControl>, PacketEvent) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync;

/// NpcControl is a high-level client for communicating with the NPC server.
pub struct NpcControl {
    /// The underlying protocol implementation.
    protocol: Arc<Mutex<GProtocolV4<BufReader<OwnedReadHalf>, OwnedWriteHalf>>>,
    /// Channel for sending outgoing packets.
    outgoing_tx: mpsc::Sender<Arc<dyn GPacket + Send>>,
    /// Broadcast channel for unsolicited incoming packets.
    pub event_tx: broadcast::Sender<PacketEvent>,
    /// Mapping from expected packet type (PacketId) to pending response channels.
    pending_requests: Arc<Mutex<HashMap<PacketId, oneshot::Sender<Arc<dyn GPacket>>>>>,
    /// Mapping from PacketId to registered event handlers.
    event_handlers: Arc<Mutex<HashMap<PacketId, Vec<Arc<EventHandlerFn>>>>>,
    /// Client configuration.
    config: NcConfig,
    /// A notify used to signal shutdown.
    shutdown: Arc<Notify>,
}

impl NpcControl {
    /// Connect to the RC server using the provided configuration.
    ///
    /// This spawns background tasks for reading and sending packets,
    /// and returns an Arc-wrapped RemoteControl.
    pub async fn connect(config: NcConfig) -> Result<Arc<Self>, ClientError> {
        let addr = format!("{}:{}", config.host, config.port);
        let stream = TcpStream::connect(&addr).await?;
        let (read_half, write_half) = stream.into_split();
        let reader = BufReader::new(read_half);
        let protocol = GProtocolV4::new(reader, write_half);
        let protocol = Arc::new(Mutex::new(protocol));

        // Create an mpsc channel for outgoing packets.
        let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<Arc<dyn GPacket + Send>>(100);
        // Create a broadcast channel for unsolicited events.
        let (event_tx, _) = broadcast::channel::<PacketEvent>(100);
        // Pending requests for request–response matching.
        let pending_requests = Arc::new(Mutex::new(HashMap::<
            PacketId,
            oneshot::Sender<Arc<dyn GPacket>>,
        >::new()));
        // Event handlers registry.
        let event_handlers = Arc::new(Mutex::new(
            HashMap::<PacketId, Vec<Arc<EventHandlerFn>>>::new(),
        ));
        // Shutdown notifier.
        let shutdown = Arc::new(Notify::new());

        let rc = Arc::new(NpcControl {
            protocol: Arc::clone(&protocol),
            outgoing_tx,
            event_tx: event_tx.clone(),
            pending_requests: Arc::clone(&pending_requests),
            event_handlers: Arc::clone(&event_handlers),
            config,
            shutdown: Arc::clone(&shutdown),
        });

        // Spawn a task to handle outgoing packets.
        {
            let protocol = Arc::clone(&protocol);
            tokio::spawn(async move {
                while let Some(packet) = outgoing_rx.recv().await {
                    let mut proto = protocol.lock().await;
                    log::debug!(
                        "Sending packet: {:?}",
                        String::from_utf8_lossy(&packet.data())
                    );
                    if let Err(e) = proto.send_packet(packet.as_ref()).await {
                        log::error!("Failed to send packet: {:?}", e);
                    }
                }
                log::error!("Outgoing channel closed, terminating send task.");
            });
        }

        // Spawn a task to continuously read incoming packets.
        {
            let protocol = Arc::clone(&protocol);
            let pending_requests = Arc::clone(&pending_requests);
            let event_tx = event_tx.clone();
            let event_handlers = Arc::clone(&event_handlers);
            let rc_clone = Arc::clone(&rc);
            let shutdown_clone = Arc::clone(&shutdown);
            tokio::spawn(async move {
                loop {
                    let packet_result = {
                        let mut proto = protocol.lock().await;
                        proto.read().await
                    };
                    match packet_result {
                        Ok(packet) => {
                            let packet_id = packet.id();
                            // Check for a matching pending request.
                            let mut pending = pending_requests.lock().await;
                            if let Some(sender) = pending.remove(&packet_id) {
                                let _ = sender.send(packet.clone());
                            } else {
                                // Otherwise, broadcast the packet as an unsolicited event.
                                log::debug!("Received unsolicited packet: {:?}", packet);
                                let event = PacketEvent {
                                    packet: packet.clone(),
                                };
                                let _ = event_tx.send(event.clone());

                                // Invoke any registered event handlers.
                                let handlers = {
                                    let handlers_map = event_handlers.lock().await;
                                    handlers_map.get(&packet_id).cloned()
                                };
                                if let Some(handler_vec) = handlers {
                                    for handler in handler_vec {
                                        let event_clone = event.clone();
                                        let rc_for_handler = Arc::clone(&rc_clone);
                                        tokio::spawn(async move {
                                            (handler)(rc_for_handler, event_clone).await;
                                        });
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Error reading packet: {:?}", e);
                            // Signal shutdown so that wait_shutdown can unblock.
                            shutdown_clone.notify_waiters();
                            break;
                        }
                    }
                }
            });
        }

        Ok(rc)
    }

    /// Send a raw packet.
    pub async fn send_packet(&self, packet: Arc<dyn GPacket + Send>) -> Result<(), ClientError> {
        self.outgoing_tx
            .send(packet)
            .await
            .map_err(|e| ClientError::Send(e))
    }

    /// Send a login packet.
    ///
    /// This method sends the login packet and (if desired) could await a login confirmation.
    pub async fn login(&self) -> Result<(), ClientError> {
        let login_packet = NcLogin::new(
            NC_PROTOCOL_VERSION,
            &self.config.login.username,
            &self.config.login.password,
        );
        {
            let mut proto = self.protocol.lock().await;
            proto.send_packet(&login_packet).await?;
        }
        Ok(())
    }

    /// Query the NPC server address.
    ///
    /// This sends the query packet and awaits a response of the expected type.
    pub async fn get_weapon(&self, weapon_name: String) -> Result<Arc<dyn GPacket>, ClientError> {
        let get_weapon_packet = NcWeaponGet::new(weapon_name);

        // Create a oneshot channel to await the response.
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(PacketId::FromServer(FromServerPacketId::NcWeaponGet), tx);
        }

        // Send the query packet.
        self.outgoing_tx
            .send(Arc::new(get_weapon_packet))
            .await
            .map_err(|e| ClientError::Send(e))?;

        // Await the response with a configurable timeout.
        let timeout_duration = self.config.timeout;
        let packet = timeout(timeout_duration, rx)
            .await
            .map_err(|e| ClientError::Timeout(e))?
            .map_err(|e| ClientError::Recv(e))?;

        Ok(packet)
    }

    /// Create a weapon
    pub async fn add_weapon(
        &self,
        weapon_name: String,
        weapon_img: String,
        weapon_script: String,
    ) -> Result<(), ClientError> {
        let add_weapon_packet = NcWeaponAdd::new(weapon_name, weapon_img, weapon_script);

        {
            let mut proto = self.protocol.lock().await;
            proto.send_packet(&add_weapon_packet).await?;
        }
        Ok(())
    }

    /// Register an event handler for a specific PacketId.
    ///
    /// The handler is an asynchronous function that takes an Arc<RemoteControl> and a PacketEvent.
    pub async fn register_event_handler<F, Fut>(&self, packet_id: PacketId, handler: F)
    where
        F: Fn(Arc<NpcControl>, PacketEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let mut handlers_map = self.event_handlers.lock().await;
        let entry = handlers_map.entry(packet_id).or_insert_with(Vec::new);
        entry.push(Arc::new(move |rc, event: PacketEvent| {
            Box::pin(handler(rc, event)) as Pin<Box<dyn Future<Output = ()> + Send>>
        }) as Arc<EventHandlerFn>);
    }

    /// Block until the RC connection is shutdown.
    ///
    /// This method awaits a shutdown notification (triggered if the read loop terminates).
    pub async fn wait_shutdown(&self) {
        self.shutdown.notified().await;
    }
}
