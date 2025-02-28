#![deny(missing_docs)]

use std::{collections::VecDeque, sync::Arc};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    io::{
        io_async::{AsyncGraalReader, AsyncGraalWriter},
        io_vec::{InMemoryGraalReaderExt, IntoSyncGraalReaderRef, IntoSyncGraalWriterRef},
    },
    net::packet::{GPacket, GPacketBuilder, PacketId, from_server::FromServerPacketId},
    utils::{compress_zlib, decompress_zlib},
    // Assume Protocol and ProtocolError are defined in your crate.
};

use super::{Protocol, ProtocolError};

/// The encryption start value for the v4 protocol.
/// (This is the same value as used in v5.)
pub const V4_ENCRYPTION_START: u32 = 0x4A80B38;

/// Struct representing the v4 protocol.
pub struct GProtocolV4<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send> {
    reader: AsyncGraalReader<R>,
    writer: AsyncGraalWriter<W>,
    packet_queue: VecDeque<Arc<dyn GPacket>>,
}

impl<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send> GProtocolV4<R, W> {
    /// Create a new v4 protocol.
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader: AsyncGraalReader::from_reader(reader),
            writer: AsyncGraalWriter::from_writer(writer),
            packet_queue: VecDeque::new(),
        }
    }

    /// Read raw packet data from the input stream,
    /// decompress it using zlib, and parse it into one or more `GPacket`s.
    pub async fn read_from_stream(&mut self) -> Result<(), ProtocolError> {
        // Read the first 2 bytes (packet length).
        let len = self.reader.read_u16().await?;
        let packet_data = self.reader.read_exact(len.into()).await?;

        // Decompress the packet data using zlib.
        let decompressed_packet = decompress_zlib(&packet_data).map_err(|e| {
            log::error!("Zlib decompression error: {:?}", e);
            ProtocolError::Io(e)
        })?;

        // Wrap the decompressed packet data in a synchronous reader.
        let mut pack_data = decompressed_packet.into_sync_graal_reader();

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

            self.packet_queue.push_back(packet);
        }
        Ok(())
    }

    /// Compress and encrypt data before sending.
    fn prepare_send(&mut self, data: &[u8]) -> Result<Vec<u8>, ProtocolError> {
        // Compress the payload using zlib.
        let compressed_payload = compress_zlib(data)?;
        // Encrypt the compressed payload (removing one byte).
        Ok(compressed_payload)
    }

    /// Send a packet by writing its data (with header, compression, and encryption)
    /// to the output stream.
    pub async fn send_packet(&mut self, packet: &dyn GPacket) -> Result<(), ProtocolError> {
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

        self.writer.write_bytes(&send_vec).await?;
        self.writer.flush().await?;
        Ok(())
    }
}

impl<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send> Protocol for GProtocolV4<R, W> {
    async fn read(&mut self) -> Result<Arc<dyn GPacket>, ProtocolError> {
        if self.packet_queue.is_empty() {
            self.read_from_stream().await?;
        }
        self.packet_queue
            .pop_front()
            .ok_or(ProtocolError::EmptyPacketQueue)
    }

    async fn write(&mut self, packet: &(dyn GPacket + Send)) -> Result<(), ProtocolError> {
        self.send_packet(packet).await
    }

    fn version(&self) -> u8 {
        4
    }
}
