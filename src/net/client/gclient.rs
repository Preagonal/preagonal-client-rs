#![deny(missing_docs)]

use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::{
    io::BufReader,
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::{Mutex, Notify, RwLock, oneshot},
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

// Event handler for disconnecting.
type DisconnectHandler = dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync;

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
    /// Disconnect handler.
    disconnect_handler: OnceLock<Arc<DisconnectHandler>>,
    /// Client configuration.
    config: ClientConfig,
    /// Shutdown notifier for GClient tasks.
    shutdown: Arc<Notify>,
    /// JoinSet for background tasks.
    join_set: Arc<Mutex<JoinSet<()>>>,
    /// Flag to track if disconnect has been handled
    is_disconnected: Arc<RwLock<bool>>,
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
        let is_disconnected = Arc::new(RwLock::new(false));

        // Store the handles in a JoinSet for later joining.
        let join_set = tokio::task::JoinSet::new();
        let join_set: Arc<Mutex<JoinSet<()>>> = Arc::new(Mutex::new(join_set));

        let gclient = Arc::new(GClient {
            protocol: Arc::clone(&protocol),
            pending_requests: Arc::clone(&pending_requests),
            event_handlers: Arc::clone(&event_handlers),
            disconnect_handler: OnceLock::new(),
            config: config.clone(),
            shutdown: Arc::clone(&shutdown),
            join_set: Arc::clone(&join_set),
            is_disconnected: Arc::clone(&is_disconnected),
        });

        let read_handle = Self::read_loop_fut(
            Arc::clone(&protocol),
            Arc::clone(&pending_requests),
            Arc::clone(&event_handlers),
            Arc::clone(&shutdown),
            Arc::clone(&join_set),
            Arc::clone(&gclient),
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
        client: Arc<GClient>,
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
                            log::error!("Error reading packet, initiating disconnect: {:?}", e);
                            client.handle_disconnect_internal("read error").await;
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

    /// Internal method to handle disconnection logic
    async fn handle_disconnect_internal(&self, reason: &str) {
        // Use a write lock to ensure only one thread can initiate disconnect
        let mut is_disconnected = self.is_disconnected.write().await;

        // If already disconnected, don't do anything
        if *is_disconnected {
            return;
        }

        log::debug!("Handling disconnect (reason: {})", reason);

        // Mark as disconnected first to prevent re-entry
        *is_disconnected = true;

        // Notify all waiting tasks to shut down
        self.shutdown.notify_waiters();

        // Drop the write lock before waiting for tasks to complete
        drop(is_disconnected);

        // Wait for all tasks to complete with a timeout to avoid deadlocks
        log::debug!("Waiting for tasks to complete");
        let _ = self.wait_for_tasks_with_timeout().await;
        log::debug!("All tasks completed or timed out");

        // Call the disconnect handler if it is set
        if let Some(handler) = self.disconnect_handler.get() {
            log::debug!("Calling registered disconnect handler");
            handler().await;
        }

        log::debug!("Disconnect handling completed");
    }

    /// Wait for tasks to complete with a timeout to avoid deadlocks
    async fn wait_for_tasks_with_timeout(&self) -> Result<(), ClientError> {
        // Take the JoinSet
        let join_set_owned = {
            let mut join_set_mut = self.join_set.lock().await;
            std::mem::take(&mut *join_set_mut)
        };

        // Create a timeout future for the join_all operation
        let timeout_duration = Duration::from_secs(5); // 5 second timeout
        let join_future = async {
            let mut set = join_set_owned;

            // Try to join tasks with a timeout for each one
            while let Some(result) = set.join_next().await {
                match result {
                    Ok(_) => log::trace!("Task completed successfully"),
                    Err(e) => log::warn!("Task panicked: {:?}", e),
                }
            }
        };

        match timeout(timeout_duration, join_future).await {
            Ok(_) => {
                log::debug!("All tasks completed gracefully");
                Ok(())
            }
            Err(_) => {
                log::warn!(
                    "Timed out waiting for tasks to complete after {} seconds",
                    timeout_duration.as_secs()
                );
                // We'll continue with disconnect even if tasks didn't complete
                Ok(())
            }
        }
    }
}

impl GClientTrait for GClient {
    async fn disconnect(&self) -> Result<(), ClientError> {
        self.handle_disconnect_internal("explicit disconnect call")
            .await;
        Ok(())
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
        let mut map = self.event_handlers.lock().await;
        let entry = map.entry(packet_id).or_insert_with(Vec::new);
        entry.push(Arc::new(move |event: PacketEvent| {
            Box::pin(handler(event)) as Pin<Box<dyn Future<Output = ()> + Send + 'static>>
        }) as Arc<EventHandlerFn>);

        Ok(())
    }

    /// Send a raw GClient packet.
    async fn send_packet(&self, packet: Arc<dyn GPacket + Send>) -> Result<(), ClientError> {
        // Check if already disconnected
        if *self.is_disconnected.read().await {
            return Err(ClientError::ClientNotConnected);
        }

        log::trace!("Sending packet: {:?}", packet);
        let result = self
            .protocol
            .write(packet.as_ref())
            .await
            .map_err(ClientError::Protocol);

        if let Err(err) = result {
            log::error!("Failed to send packet: {:?}", err);
            self.handle_disconnect_internal("send packet error").await;
            return Err(err);
        } else {
            log::debug!("Packet sent successfully.");
        }

        Ok(())
    }

    async fn send_and_receive(
        &self,
        packet: Arc<dyn GPacket + Send>,
        response_packet: FromServerPacketId,
    ) -> Result<Arc<dyn GPacket>, ClientError> {
        // Check if already disconnected
        if *self.is_disconnected.read().await {
            return Err(ClientError::ClientNotConnected);
        }

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
                    log::error!("Failed to receive packet response: {:?}", error);
                    self.handle_disconnect_internal("receive error").await;
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
                self.handle_disconnect_internal("timeout").await;
                return Err(error);
            }
        };

        Ok(packet)
    }

    /// Block until all tasks have completed.
    async fn wait_for_tasks(&self) -> Result<(), ClientError> {
        self.wait_for_tasks_with_timeout().await
    }

    async fn register_disconnect_handler<F, Fut>(&self, handler: F) -> Result<(), ClientError>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        // set the disconnect handler
        let handler = Arc::new(move || {
            let fut = handler();
            Box::pin(fut) as Pin<Box<dyn Future<Output = ()> + Send>>
        }) as Arc<DisconnectHandler>;
        self.disconnect_handler.set(handler).unwrap_or(());

        Ok(())
    }
}
