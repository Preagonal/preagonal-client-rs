#![deny(missing_docs)]

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};
use tokio::{
    io::BufReader,
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::{Mutex, Notify, oneshot},
    task::JoinSet,
    time::timeout,
};

use crate::{
    config::ClientConfig,
    net::{
        packet::{GPacket, PacketEvent, PacketId, from_server::FromServerPacketId},
        protocol::{GProtocolEnum, Protocol},
    },
};

use super::{ClientError, GClientTrait};

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
    config: ClientConfig,
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
        config: &ClientConfig,
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

        // On v6 clients, send the handshake packet.
        // TODO: We can probably find a better place for this.
        if let GProtocolEnum::V6(proto) = &*protocol {
            proto.send_handshake().await?;
        }

        let gclient = Arc::new(GClient {
            protocol: Arc::clone(&protocol),
            pending_requests: Arc::clone(&pending_requests),
            event_handlers: Arc::clone(&event_handlers),
            config: config.clone(),
            shutdown: Arc::clone(&shutdown),
            join_set: Arc::clone(&join_set),
        });

        Ok(gclient)
    }

    /// Set the encryption key for the client. Only applicable to v5 protocol.
    pub async fn set_codec(&self, key: u8) -> Result<(), ClientError> {
        match &*self.protocol {
            GProtocolEnum::V5(proto) => {
                proto.set_encryption_key(key)?;
            }
            _ => {
                return Err(ClientError::UnsupportedProtocolVersion);
            }
        };
        Ok(())
    }

    /// Receiving packets loop.
    async fn read_loop_fut(
        protocol: Arc<GProtocolEnum<BufReader<OwnedReadHalf>, OwnedWriteHalf>>,
        pending_requests: PendingRequests,
        event_handlers: Arc<Mutex<HashMap<PacketId, Vec<Arc<EventHandlerFn>>>>>,
        shutdown: Arc<Notify>,
        join_set: Arc<Mutex<JoinSet<()>>>,
    ) {
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
                                log::trace!("Received response packet: {:?}", packet);
                                let _ = sender.send(packet.clone());
                            } else {
                                log::trace!("Received unsolicited packet: {:?}", packet);
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
                            log::error!("Error reading packet, shutting down: {:?}", e);
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

impl GClientTrait for GClient {
    async fn disconnect(&self) {
        log::debug!("Shutting down. Waiting for tasks to complete...");
        self.shutdown.notify_waiters();
        self.wait_for_tasks().await;
    }

    async fn register_event_handler<F, Fut>(&self, packet_id: PacketId, handler: F)
    where
        F: Fn(PacketEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let mut map = self.event_handlers.lock().await;
        let entry = map.entry(packet_id).or_insert_with(Vec::new);
        entry.push(Arc::new(move |event: PacketEvent| {
            Box::pin(handler(event)) as Pin<Box<dyn Future<Output = ()> + Send + 'static>>
        }) as Arc<EventHandlerFn>);
    }

    /// Send a raw GClient packet.
    async fn send_packet(&self, packet: Arc<dyn GPacket + Send>) -> Result<(), ClientError> {
        log::trace!("Sending packet: {:?}", packet);
        let result = self
            .protocol
            .write(packet.as_ref())
            .await
            .map_err(ClientError::Protocol);

        if let Err(ref err) = result {
            log::error!("Failed to send packet, shutting down: {:?}", err);
            self.disconnect().await;
        } else {
            log::debug!("Packet sent successfully.");
        }

        result
    }

    async fn send_and_receive(
        &self,
        packet: Arc<dyn GPacket + Send>,
        response_packet: FromServerPacketId,
    ) -> Result<Arc<dyn GPacket>, ClientError> {
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(PacketId::FromServer(response_packet), tx);
        }

        // `send_packet` will already handle shutdown if it fails.
        self.send_packet(packet).await?;

        // Handle receive errors and shutdown
        let packet = match timeout(self.config.timeout, rx).await {
            Ok(recv_result) => match recv_result {
                Ok(packet) => packet,
                // RecvError is encountered when the sender is dropped, which should not happen, but
                // we should handle it just in case.
                Err(err) => {
                    let error = ClientError::Recv(err);
                    log::error!(
                        "Failed to receive packet response, shutting down: {:?}",
                        error
                    );
                    self.disconnect().await;
                    return Err(error);
                }
            },
            Err(err) => {
                let error = ClientError::Timeout(err);
                log::error!(
                    "Expected packet response, but timed out after {} seconds: {:?}",
                    self.config.timeout.as_secs(),
                    error
                );
                self.disconnect().await;
                return Err(error);
            }
        };

        Ok(packet)
    }

    /// Block until all tasks have completed.
    async fn wait_for_tasks(&self) {
        let join_set_owned = {
            let mut join_set_mut = self.join_set.lock().await;
            std::mem::take(&mut *join_set_mut)
        };
        join_set_owned.join_all().await;
    }
}
