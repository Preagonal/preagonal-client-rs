#![deny(missing_docs)]

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};
use tokio::{
    io::BufReader,
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::{Mutex, Notify, oneshot},
    task::JoinSet,
    time::timeout,
};

use crate::net::{
    packet::{GPacket, PacketEvent, PacketId, from_server::FromServerPacketId},
    protocol::{GProtocolEnum, Protocol},
};

use super::{ClientError, GClientConfig};

/// An asynchronous event handler function that receives a PacketEvent.
type EventHandlerFn = dyn Fn(PacketEvent) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync;

/// Defines the pending requests type.
type PendingRequests = Arc<Mutex<HashMap<PacketId, oneshot::Sender<Arc<dyn GPacket>>>>>;

/// GClient is a high-level client for communicating with GServer over a variety of protocols.
pub struct GClient {
    /// The underlying protocol implementation
    protocol: Arc<GProtocolEnum<BufReader<OwnedReadHalf>, OwnedWriteHalf>>,
    /// Mapping from expected packet type (PacketId) to pending response channels.
    pending_requests: PendingRequests,
    /// Mapping from PacketId to registered event handlers.
    event_handlers: Arc<Mutex<HashMap<PacketId, Vec<Arc<EventHandlerFn>>>>>,
    /// Client configuration.
    config: GClientConfig,
    /// Shutdown notifier for GClient tasks.
    shutdown: Arc<Notify>,
    /// JoinSet for background tasks.
    join_set: Arc<Mutex<JoinSet<()>>>,
}

impl GClient {
    /// Connect to the server using the provided configuration.
    ///
    /// This spawns background tasks for reading and sending packets,
    /// and returns an Arc-wrapped GClient.
    pub async fn connect(
        config: GClientConfig,
        protocol: GProtocolEnum<BufReader<OwnedReadHalf>, OwnedWriteHalf>,
    ) -> Result<Arc<Self>, ClientError> {
        let protocol = Arc::new(protocol);

        let pending_requests = Arc::new(Mutex::new(HashMap::new()));
        let event_handlers = Arc::new(Mutex::new(HashMap::new()));
        let shutdown = Arc::new(Notify::new());

        // Store the handles in a JoinSet for later joining.
        let join_set = tokio::task::JoinSet::new();
        let join_set: Arc<Mutex<JoinSet<()>>> = Arc::new(Mutex::new(join_set));

        let read_handle = Self::read_loop_fut(
            Arc::clone(&protocol),
            Arc::clone(&pending_requests),
            Arc::clone(&event_handlers),
            Arc::clone(&shutdown),
            Arc::clone(&join_set),
        );

        // Add the handles to the JoinSet, but first lock the mutex.
        {
            let mut join_set_mut = join_set.lock().await;
            join_set_mut.spawn(read_handle);
        }

        let gclient = Arc::new(GClient {
            protocol: Arc::clone(&protocol),
            pending_requests: Arc::clone(&pending_requests),
            event_handlers: Arc::clone(&event_handlers),
            config,
            shutdown: Arc::clone(&shutdown),
            join_set: Arc::clone(&join_set),
        });

        Ok(gclient)
    }

    /// Send a packet and wait for a response.
    pub async fn send_and_receive(
        &self,
        packet: Arc<dyn GPacket + Send>,
        response_packet: FromServerPacketId,
    ) -> Result<Arc<dyn GPacket>, ClientError> {
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(PacketId::FromServer(response_packet), tx);
        }
        self.send_packet(packet).await?;
        let packet = timeout(self.config.timeout, rx)
            .await
            .map_err(ClientError::Timeout)?
            .map_err(ClientError::Recv)?;
        Ok(packet)
    }

    /// Send a raw GClient packet.
    pub async fn send_packet(&self, packet: Arc<dyn GPacket + Send>) -> Result<(), ClientError> {
        log::debug!("Sending packet: {:?}", packet);
        self.protocol
            .write(packet.as_ref())
            .await
            .map_err(ClientError::Protocol)?;
        log::debug!("Packet sent successfully.");
        Ok(())
    }

    /// Set the encryption key for the client. Only applicable to v5 protocol.
    pub async fn set_codec(&self, key: u8) -> Result<(), ClientError> {
        Ok(match &*self.protocol {
            GProtocolEnum::V5(proto) => {
                proto.set_encryption_key(key)?;
            }
            _ => {
                return Err(ClientError::UnsupportedProtocolVersion);
            }
        })
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

    /// Send shutdown signal, and call wait_for_tasks.
    pub async fn shutdown(&self) {
        log::debug!("Shutting down. Waiting for tasks to complete...");
        self.shutdown.notify_waiters();
        self.wait_for_tasks().await;
    }

    /// Block until all tasks have completed.
    pub async fn wait_for_tasks(&self) {
        let join_set_owned = {
            let mut join_set_mut = self.join_set.lock().await;
            std::mem::take(&mut *join_set_mut)
        };
        join_set_owned.join_all().await;
    }

    /// Receiving packets loop.
    fn read_loop_fut(
        protocol: Arc<GProtocolEnum<BufReader<OwnedReadHalf>, OwnedWriteHalf>>,
        pending_requests: PendingRequests,
        event_handlers: Arc<Mutex<HashMap<PacketId, Vec<Arc<EventHandlerFn>>>>>,
        shutdown: Arc<Notify>,
        join_set: Arc<Mutex<JoinSet<()>>>,
    ) -> impl Future<Output = ()> {
        async move {
            loop {
                tokio::select! {
                    packet_result = async {
                        protocol.read().await
                    } => {
                        match packet_result {
                            Ok(packet) => {
                                let packet_id = packet.id();
                                let mut pending = pending_requests.lock().await;
                                if let Some(sender) = pending.remove(&packet_id) {
                                    log::debug!("Received response packet: {:?}", packet);
                                    let _ = sender.send(packet.clone());
                                } else {
                                    log::debug!("Received unsolicited packet: {:?}", packet);
                                    let event = PacketEvent { packet: packet.clone() };
                                    let handlers = {
                                        let map = event_handlers.lock().await;
                                        map.get(&packet_id).cloned()
                                    };
                                    if let Some(handler_vec) = handlers {
                                        for handler in handler_vec {
                                            let event_clone = event.clone();
                                            let shutdown_clone = Arc::clone(&shutdown);
                                            let task = async move {
                                                tokio::select! {
                                                    _ = shutdown_clone.notified() => {
                                                        log::debug!("Callback task received shutdown signal, exiting early.");
                                                    }
                                                    _ = (handler)(event_clone) => {
                                                        // Callback completed normally.
                                                    }
                                                }
                                            };
                                            let mut join_set_mut = join_set.lock().await;
                                            join_set_mut.spawn(task);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Error reading packet: {:?}", e);
                                shutdown.notify_waiters();
                                break;
                            }
                        }
                    }
                    _ = shutdown.notified() => {
                        log::debug!("Shutdown signal received in read loop.");
                        break;
                    }
                }
            }
        }
    }
}
