use rc4::StreamCipher;
use rc4::{KeyInit, Rc4, consts::U16};
use rsa::Pkcs1v15Encrypt;
use std::sync::atomic::{AtomicU8, Ordering};
use std::{collections::VecDeque, sync::Arc};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Mutex;

use crate::config::EncryptionKeys;
use crate::consts::V6_PROTOCOL_STRING;
use crate::io::io_async::{AsyncGraalReader, AsyncGraalWriter};
use crate::io::io_vec::IntoSyncGraalReaderRef;
use crate::net::packet::{GPacket, GPacketBuilder, PacketId, from_server::FromServerPacketId};
use crate::net::protocol::ProtocolError;
use crate::net::protocol::proto_v6_header::{GCompressionTypeV6, PacketDirection};
use crate::utils::{compress_zlib, decompress_bzip2, decompress_zlib};

use super::proto_v6_header::{GProtocolV6HeaderFormat, GProtocolV6PacketHeader};

/// GProtocolV6 implements the unified protocol. It holds its own asynchronous reader,
/// writer, and a packet queue. It also maintains state for header format, encryption keys,
/// checksums, and (once encountered) an RC4 key.
pub struct GProtocolV6<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send> {
    reader: Mutex<AsyncGraalReader<R>>,
    writer: Mutex<AsyncGraalWriter<W>>,
    packet_queue: Mutex<VecDeque<Arc<dyn GPacket>>>,
    header_format: GProtocolV6HeaderFormat,
    encryption_keys: EncryptionKeys,
    from_gserver_checksum: AtomicU8,
    to_client_checksum: AtomicU8,
    rc4_key: Mutex<Option<Rc4<U16>>>,
}

impl<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send> GProtocolV6<R, W> {
    /// Create a new GProtocolV6.
    pub fn new(
        reader: R,
        writer: W,
        header_format: GProtocolV6HeaderFormat,
        encryption_keys: EncryptionKeys,
    ) -> Self {
        Self {
            reader: Mutex::new(AsyncGraalReader::from_reader(reader)),
            writer: Mutex::new(AsyncGraalWriter::from_writer(writer)),
            packet_queue: Mutex::new(VecDeque::new()),
            header_format,
            encryption_keys,
            from_gserver_checksum: AtomicU8::new(1),
            to_client_checksum: AtomicU8::new(1),
            rc4_key: Mutex::new(None),
        }
    }

    async fn read_encrypted_stream_bytes(&self, length: usize) -> Result<Vec<u8>, ProtocolError> {
        let mut reader = self.reader.lock().await;
        let mut data = reader.read_exact(length).await?;

        // Get RC4 key
        let mut rc4_key = self.rc4_key.lock().await;
        if let Some(rc4_key) = &mut *rc4_key {
            rc4_key.apply_keystream(&mut data);
        }
        Ok(data)
    }

    /// Process a bundled packet payload recursively. Packets inside a bundle
    /// are processed in the same way as regular packets, but they are not queued
    /// if they are bundles or encryption key packets.
    async fn process_packet_bundle(&self, data_bytes: &Vec<u8>) -> Result<(), ProtocolError> {
        let mut data: VecDeque<u8> = VecDeque::new();
        data.extend(data_bytes);

        while data.len() > self.header_format.header_length {
            // Peek at the header bytes without consuming them from the deque
            let header_buffer: Vec<u8> = data
                .iter()
                .take(self.header_format.header_length)
                .cloned()
                .collect();

            let header = self
                .header_format
                .parse_header(PacketDirection::Incoming, &header_buffer)?;

            log::trace!("Got header: {:?}", header);
            let mut queue_packet = true;
            if data.len() < header.length {
                log::warn!(
                    "Invalid packet length: expected {}, got {}",
                    header.length,
                    data.len()
                );
                break;
            }

            data.drain(..self.header_format.header_length);
            let packet: Vec<u8> = data
                .drain(..(header.length - self.header_format.header_length))
                .collect();

            let packet: Vec<u8> = match header.compression {
                GCompressionTypeV6::CompressionNone => Ok(packet),
                GCompressionTypeV6::CompressionZlib => {
                    decompress_zlib(&packet).map_err(ProtocolError::Io)
                }
                GCompressionTypeV6::CompressionBzip2 => {
                    decompress_bzip2(&packet).map_err(ProtocolError::Io)
                }
            }?;

            log::trace!(
                "Processing packet with id {:?} and size {}",
                header.id,
                header.length
            );

            if header.id == PacketId::FromServer(FromServerPacketId::Bundle) {
                log::trace!("Processing packet bundle");
                queue_packet = false;
                Box::pin(self.process_packet_bundle(&packet)).await?;
            }

            if header.id == PacketId::FromServer(FromServerPacketId::SetEncKey) {
                log::trace!("Processing encryption key packet");
                queue_packet = true;

                let payload = self
                    .encryption_keys
                    .rsa_private_key
                    .decrypt(Pkcs1v15Encrypt, &packet.clone())?;

                let mut enc_packet_queue = payload.into_sync_graal_reader();

                // We subtract 32 because of Graal's weird encoding

                let encryption_type = enc_packet_queue.read_gu8()?;
                if encryption_type == 0 {
                    return Err(ProtocolError::Other(
                        "AES is not supported, consider using RC4.".to_string(),
                    ));
                }

                let key_length = enc_packet_queue.read_gu8()?;
                let key = enc_packet_queue.read_exact(key_length as usize)?;

                let init_length = enc_packet_queue.read_gu8()?;
                let _init = enc_packet_queue.read_exact(init_length as usize)?;

                log::debug!("Setting RC4 key: {:?}", key);

                let key = Rc4::<U16>::new_from_slice(&key)
                    .map_err(|_| ProtocolError::Other("Failed to create RC4 key".to_string()))?;

                let mut rc4_lock = self.rc4_key.lock().await;
                *rc4_lock = Some(key);
            }

            let current_checksum = self.from_gserver_checksum.load(Ordering::Acquire);
            if current_checksum != header.checksum {
                log::warn!(
                    "Invalid checksum: expected {}, got {}",
                    current_checksum,
                    header.checksum
                );
            }

            // Reset the checksum if it's 255
            if header.checksum == 255 {
                self.from_gserver_checksum.store(0, Ordering::Release);
            } else {
                self.from_gserver_checksum
                    .store(header.checksum + 1, Ordering::Release);
            }

            let packet_to_queue = GPacketBuilder::new(header.id).with_data(packet).build();

            if queue_packet {
                log::trace!(
                    "Queueing packet with id {:?} and size {}",
                    header.id,
                    header.length
                );
                self.packet_queue.lock().await.push_back(packet_to_queue);
            } else {
                log::trace!(
                    "Ignoring packet with id {:?} and size {}",
                    header.id,
                    header.length
                );
            }
        }

        Ok(())
    }

    /// Read raw packet data from the input stream
    async fn read_from_stream(&self) -> Result<(), ProtocolError> {
        // Read the header bytes
        let header_length = self.header_format.header_length;
        let header_bytes = self.read_encrypted_stream_bytes(header_length).await?;
        let header = self
            .header_format
            .parse_header(PacketDirection::Incoming, &header_bytes)?;

        // Read the packet data
        let packet_data = self
            .read_encrypted_stream_bytes(header.length - header_length)
            .await?;

        // combine the header and packet data
        let mut combined_data = header_bytes;
        combined_data.extend(packet_data);

        // Process the data
        self.process_packet_bundle(&combined_data).await?;
        Ok(())
    }

    /// Prepare and send a packet. Outgoing encryption is not applied at this time.
    async fn send_packet(&self, packet: &dyn GPacket) -> Result<(), ProtocolError> {
        let mut packet_data = packet.data().clone();
        let mut compression = GCompressionTypeV6::CompressionNone;
        if packet_data.len() > 55 {
            packet_data = compress_zlib(&packet_data).map_err(ProtocolError::Io)?;
            compression = GCompressionTypeV6::CompressionZlib;
        }
        let total_length = self.header_format.header_length + packet_data.len();
        let checksum = self.to_client_checksum.load(Ordering::Acquire);
        let header = GProtocolV6PacketHeader {
            compression,
            checksum,
            length: total_length,
            id: packet.id(),
        };
        let header_bytes = self.header_format.create_header(&header)?;
        if checksum == 255 {
            self.to_client_checksum.store(0, Ordering::Release);
        } else {
            self.to_client_checksum.fetch_add(1, Ordering::Release);
        }
        let mut send_buffer = header_bytes;
        send_buffer.extend(packet_data);
        let mut writer = self.writer.lock().await;
        writer.write_bytes(&send_buffer).await?;
        writer.flush().await?;
        Ok(())
    }

    /// Send the handshake packet.
    pub async fn send_handshake(&self) -> Result<(), ProtocolError> {
        self.writer
            .lock()
            .await
            .write_bytes(V6_PROTOCOL_STRING.as_bytes())
            .await?;
        Ok(())
    }
}

impl<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send> crate::net::protocol::Protocol
    for GProtocolV6<R, W>
{
    async fn read(&self) -> Result<Arc<dyn GPacket>, ProtocolError> {
        // Keep trying to read until we have a packet
        loop {
            {
                let queue = self.packet_queue.lock().await;
                if !queue.is_empty() {
                    let packet = queue.front().cloned();
                    drop(queue);

                    if let Some(packet) = packet {
                        // Remove the packet from the queue now that we have a copy
                        self.packet_queue.lock().await.pop_front();
                        return Ok(packet);
                    }
                }
            }

            // No packets in queue, try to read from stream
            match self.read_from_stream().await {
                Ok(_) => {
                    // Successfully read from stream, check queue again in next iteration
                }
                Err(e) => {
                    // If it's a connection error or serious issue, return it
                    // Otherwise, we might want to retry or handle specific errors
                    if !matches!(e, ProtocolError::EmptyPacketQueue) {
                        return Err(e);
                    }
                    // For EmptyPacketQueue, we'll just loop and try again
                }
            }

            // Optional: Add a small delay to prevent tight looping
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    }

    async fn write(&self, packet: &(dyn GPacket + Send)) -> Result<(), ProtocolError> {
        self.send_packet(packet).await
    }

    fn version(&self) -> u8 {
        6
    }
}
