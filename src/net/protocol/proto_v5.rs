#![deny(missing_docs)]

use std::{collections::VecDeque, sync::Arc};

use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    io::{
        io_async::{AsyncGraalReader, AsyncGraalWriter},
        io_vec::{InMemoryGraalReaderExt, IntoSyncGraalReaderRef, IntoSyncGraalWriterRef},
    },
    net::packet::{GPacket, GPacketBuilder, PacketId, from_server::FromServerPacketId},
    utils::{compress_zlib, decompress_bzip2, decompress_zlib},
};

use super::{Protocol, ProtocolError};

/// The encryption start value for the v5 protocol.
pub const V5_ENCRYPTION_START: u32 = 0x4A80B38;

/// The compression start value for the v5 protocol.
#[repr(u8)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum GCompressionTypeV5 {
    /// No compression.
    CompressionNone = 2,
    /// Zlib compression.
    CompressionZlib = 4,
    /// Bzip2 compression.
    CompressionBzip2 = 6,
}

impl From<GCompressionTypeV5> for u8 {
    fn from(packet: GCompressionTypeV5) -> u8 {
        packet as u8
    }
}

// Implement `TryFrom<u8>` for the enum.
impl TryFrom<u8> for GCompressionTypeV5 {
    type Error = ProtocolError;

    fn try_from(byte: u8) -> Result<Self, Self::Error> {
        match byte {
            2 => Ok(GCompressionTypeV5::CompressionNone),
            4 => Ok(GCompressionTypeV5::CompressionZlib),
            6 => Ok(GCompressionTypeV5::CompressionBzip2),
            _ => Err(ProtocolError::InvalidCompression(byte)),
        }
    }
}

/// Struct representing the v5 protocol.
pub struct GProtocolV5<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send> {
    reader: AsyncGraalReader<R>,
    writer: AsyncGraalWriter<W>,
    encryption_out_state: u32,
    encryption_in_state: u32,
    packet_queue: VecDeque<Arc<dyn GPacket>>,
    encryption_key: Option<u8>,
}

impl<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send> GProtocolV5<R, W> {
    /// Create a new v5 protocol.
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader: AsyncGraalReader::from_reader(reader),
            writer: AsyncGraalWriter::from_writer(writer),
            encryption_out_state: V5_ENCRYPTION_START,
            encryption_in_state: V5_ENCRYPTION_START,
            packet_queue: VecDeque::new(),
            encryption_key: None,
        }
    }

    fn get_iterator_limit(&self, compression_type: GCompressionTypeV5) -> u32 {
        match compression_type {
            GCompressionTypeV5::CompressionNone => 0xc, // 12 in hex.
            _ => 0x4,                                   // 4 in hex.
        }
    }

    /// Set the encryption key.
    pub fn set_encryption_key(&mut self, key: u8) {
        self.encryption_key = Some(key);
    }

    /// Process a buffer (for encryption or decryption).
    /// If `is_outgoing` is true, update the out-state; otherwise update the in-state.
    fn process(
        &mut self,
        compression_type: GCompressionTypeV5,
        is_outgoing: bool,
        buf: &[u8],
    ) -> Vec<u8> {
        if self.encryption_key.is_none() {
            return buf.to_vec();
        }
        let mut new_buf = buf.to_vec();
        let mut limit = self.get_iterator_limit(compression_type);

        for (i, elem) in new_buf.iter_mut().enumerate() {
            if i % 4 == 0 {
                if limit == 0 {
                    break;
                }
                limit -= 1;
                if let Some(key) = self.encryption_key {
                    if is_outgoing {
                        self.encryption_out_state = self
                            .encryption_out_state
                            .wrapping_mul(0x8088405)
                            .wrapping_add(key as u32);
                    } else {
                        self.encryption_in_state = self
                            .encryption_in_state
                            .wrapping_mul(0x8088405)
                            .wrapping_add(key as u32);
                    }
                }
            }
            let state = if is_outgoing {
                self.encryption_out_state
            } else {
                self.encryption_in_state
            };
            let shift = ((i % 4) * 8) as u32;
            let mask = (state >> shift) as u8;
            *elem ^= mask;
        }
        new_buf
    }

    /// Encrypt a buffer.
    fn encrypt(&mut self, buf: &[u8], compression: GCompressionTypeV5) -> Vec<u8> {
        self.process(compression, true, buf)
    }

    /// Decrypt a buffer.
    fn decrypt(&mut self, buf: &[u8], compression: GCompressionTypeV5) -> Vec<u8> {
        self.process(compression, false, buf)
    }

    /// Read raw packet data from the input stream,
    /// decrypt, decompress, and parse it into one or more GPacket(s).
    pub async fn read_from_stream(&mut self) -> Result<(), ProtocolError> {
        // Read the first 2 bytes (packet length).
        let len = self.reader.read_u16().await?;
        let packet_data = self.reader.read_exact(len.into()).await?;

        // The first byte indicates the compression type.
        let compression_type = GCompressionTypeV5::try_from(packet_data[0])?;

        // Decrypt the remaining bytes.
        let decrypted_packet = self.decrypt(&packet_data[1..], compression_type);

        // Decompress according to the compression type.
        let decompressed_packet = match compression_type {
            GCompressionTypeV5::CompressionNone => decrypted_packet,
            GCompressionTypeV5::CompressionZlib => {
                decompress_zlib(&decrypted_packet).map_err(|e| {
                    log::error!("Zlib decompression error: {:?}", e);
                    ProtocolError::Io(e.into())
                })?
            }
            GCompressionTypeV5::CompressionBzip2 => {
                decompress_bzip2(&decrypted_packet).map_err(|e| {
                    log::error!("Bzip2 decompression error: {:?}", e);
                    ProtocolError::Io(e.into())
                })?
            }
        };

        // Wrap decompressed_packet in a cursor.
        let mut pack_data = decompressed_packet.into_sync_graal_reader();

        while pack_data.can_read()? {
            let packet_type = pack_data.read_gu8()?;
            // Read until newline.
            let payload: Vec<u8> = pack_data.read_until(0xa)?;
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
        // Encrypt the compressed payload.
        Ok(self.encrypt(&compressed_payload, GCompressionTypeV5::CompressionZlib))
    }

    /// Send a packet by writing its data (with header, compression, and encryption)
    /// to the output stream.
    pub async fn send_packet(
        &mut self,
        packet: &(dyn GPacket + Send),
    ) -> Result<(), ProtocolError> {
        // Create new GraalReader with a new buffer.
        let mut vec = Vec::new();
        {
            let mut payload_stream = vec.into_sync_graal_writer();
            payload_stream.write_gu8(packet.id().into())?;
            payload_stream.write_bytes(&packet.data())?;
            payload_stream.write_bytes(&[0xA])?;
            payload_stream.flush()?;
        }

        // get all the bytes from the buffer.
        let prepared_buffer = self.prepare_send(&vec)?;

        let length: u16 = if self.encryption_key.is_some() {
            (prepared_buffer.len() + 1) as u16
        } else {
            prepared_buffer.len() as u16
        };

        let mut send_vec = Vec::new();
        {
            let mut send_stream = send_vec.into_sync_graal_writer();
            send_stream.write_u16(length)?;
            if self.encryption_key.is_some() {
                send_stream.write_u8(GCompressionTypeV5::CompressionZlib.into())?;
            }
            send_stream.write_bytes(&prepared_buffer)?;
            send_stream.flush()?;
        }

        self.writer.write_bytes(&send_vec).await?;
        self.writer.flush().await?;
        Ok(())
    }
}

impl<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send> Protocol for GProtocolV5<R, W> {
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
        5
    }
}
