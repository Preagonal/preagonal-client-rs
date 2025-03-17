#![deny(missing_docs)]

use std::{collections::VecDeque, sync::Arc};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Mutex;

use crate::io::io_vec::InMemoryGraalReaderExt;
use crate::{
    io::{
        io_async::{AsyncGraalReader, AsyncGraalWriter},
        io_vec::{IntoSyncGraalReaderRef, IntoSyncGraalWriterRef},
    },
    net::packet::{GPacket, GPacketBuilder, PacketId, from_server::FromServerPacketId},
    utils::{compress_zlib, decompress_zlib},
};

use super::{Protocol, ProtocolError};

/// Struct representing the v4 protocol, now with separate locks for reading, writing,
/// and for the packet queue.
pub struct GProtocolV4<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send> {
    /// Reader is guarded by its own mutex.
    reader: Mutex<AsyncGraalReader<R>>,
    /// Writer is guarded by its own mutex.
    writer: Mutex<AsyncGraalWriter<W>>,
    /// Packet queue, filled by the read loop.
    packet_queue: Mutex<VecDeque<Arc<dyn GPacket>>>,
}

impl<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send> GProtocolV4<R, W> {
    /// Create a new v4 protocol.
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader: Mutex::new(AsyncGraalReader::from_reader(reader)),
            writer: Mutex::new(AsyncGraalWriter::from_writer(writer)),
            packet_queue: Mutex::new(VecDeque::new()),
        }
    }

    /// Read raw packet data from the input stream, decompress it using zlib,
    /// and parse it into one or more `GPacket`s.
    pub async fn read_from_stream(&self) -> Result<(), ProtocolError> {
        // Acquire the reader lock.
        let mut reader = self.reader.lock().await;
        // Read the first 2 bytes (packet length).
        let len = reader.read_u16().await?;
        let packet_data = reader.read_exact(len.into()).await?;
        // Release the reader lock as early as possible.
        drop(reader);

        // Decompress the packet data using zlib.
        let decompressed_packet = decompress_zlib(&packet_data).map_err(|e| {
            log::error!("Zlib decompression error: {:?}", e);
            ProtocolError::Io(e)
        })?;

        // Wrap the decompressed packet data in a synchronous reader.
        let mut pack_data = decompressed_packet.into_sync_graal_reader();
        // Lock the packet queue for writing.
        let mut queue = self.packet_queue.lock().await;
        while pack_data.can_read()? {
            // Read a packet: first byte is the packet type.
            let packet_type = pack_data.read_gu8()?;
            // Read until the newline (0x0A) delimiter.
            let payload: Vec<u8> = pack_data.read_until(0xA)?;
            let packet = GPacketBuilder::new(PacketId::FromServer(FromServerPacketId::try_from(
                packet_type,
            )?))
            .with_data(payload)
            .build();

            queue.push_back(packet);
        }
        Ok(())
    }

    /// Compress (and encrypt if needed) data before sending.
    fn prepare_send(&self, data: &[u8]) -> Result<Vec<u8>, ProtocolError> {
        // Compress the payload using zlib.
        let compressed_payload = compress_zlib(data)?;
        // Here you can apply encryption if needed.
        Ok(compressed_payload)
    }

    /// Shutdown the protocol.
    pub async fn shutdown(&self) -> Result<(), ProtocolError> {
        // Lock only the writer for sending.
        let mut writer = self.writer.lock().await;
        writer.shutdown().await?;
        Ok(())
    }

    /// Send a packet by writing its data (with header, compression, and encryption)
    /// to the output stream.
    pub async fn send_packet(&self, packet: &dyn GPacket) -> Result<(), ProtocolError> {
        // Build the packet payload: packet type followed by data and a newline.
        let mut vec = Vec::new();
        {
            let mut payload_stream = vec.into_sync_graal_writer();
            payload_stream.write_gu8(packet.id().into())?;
            payload_stream.write_bytes(&packet.data())?;
            payload_stream.write_bytes(&[0xA])?;
            payload_stream.flush()?;
        }

        // Prepare (compress then encrypt) the payload.
        let prepared_buffer = self.prepare_send(&vec)?;
        let length = prepared_buffer.len() as u16;

        let mut send_vec = Vec::new();
        {
            let mut send_stream = send_vec.into_sync_graal_writer();
            send_stream.write_u16(length)?;
            send_stream.write_bytes(&prepared_buffer)?;
            send_stream.flush()?;
        }

        // Lock only the writer for sending.
        let mut writer = self.writer.lock().await;
        writer.write_bytes(&send_vec).await?;
        writer.flush().await?;
        Ok(())
    }
}

impl<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send> Protocol for GProtocolV4<R, W> {
    /// Read a packet. If the packet queue is empty, fill it by reading from the stream.
    async fn read(&self) -> Result<Arc<dyn GPacket>, ProtocolError> {
        {
            let queue = self.packet_queue.lock().await;
            if queue.is_empty() {
                drop(queue);
                self.read_from_stream().await?;
            }
        }
        let mut queue = self.packet_queue.lock().await;
        queue.pop_front().ok_or(ProtocolError::EmptyPacketQueue)
    }

    /// Write a packet.
    async fn write(&self, packet: &(dyn GPacket + Send)) -> Result<(), ProtocolError> {
        self.send_packet(packet).await
    }

    /// Write a packet.
    async fn shutdown(&self) -> Result<(), ProtocolError> {
        self.shutdown().await
    }

    /// Return the protocol version.
    fn version(&self) -> u8 {
        4
    }
}
