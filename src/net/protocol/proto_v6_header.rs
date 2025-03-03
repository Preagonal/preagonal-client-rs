use crate::net::{
    packet::{PacketId, from_client::FromClientPacketId, from_server::FromServerPacketId},
    protocol::ProtocolError,
};
use std::convert::TryFrom;

/// A header format for protocol v6. The header format is defined by a protocol string
/// (which can only contain the characters 'E', 'I', 'L', and 'T').
#[derive(Debug, Clone)]
pub struct GProtocolV6HeaderFormat {
    /// The length of the header.
    pub header_length: usize,
    /// The index of the compression type.
    pub compression_index: usize,
    /// The size of the compression type.
    pub compression_size: usize,
    /// The index of the checksum.
    pub checksum_index: usize,
    /// The size of the checksum.
    pub checksum_size: usize,
    /// The index of the length.
    pub length_index: usize,
    /// The size of the length.
    pub length_size: usize,
    /// The index of the packet type.
    pub type_index: usize,
    /// The size of the packet type.
    pub type_size: usize,
}

impl TryFrom<&str> for GProtocolV6HeaderFormat {
    type Error = ProtocolError;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        if s.chars().any(|c| !matches!(c, 'E' | 'I' | 'L' | 'T')) {
            return Err(ProtocolError::InvalidHeaderFormat(s.to_string()));
        }
        let header_length = s.len();
        let compression_index = s
            .find('E')
            .ok_or_else(|| ProtocolError::InvalidHeaderFormat(s.to_string()))?;
        let compression_size = s.chars().filter(|&c| c == 'E').count();
        let checksum_index = s
            .find('I')
            .ok_or_else(|| ProtocolError::InvalidHeaderFormat(s.to_string()))?;
        let checksum_size = s.chars().filter(|&c| c == 'I').count();
        let length_index = s
            .find('L')
            .ok_or_else(|| ProtocolError::InvalidHeaderFormat(s.to_string()))?;
        let length_size = s.chars().filter(|&c| c == 'L').count();
        let type_index = s
            .find('T')
            .ok_or_else(|| ProtocolError::InvalidHeaderFormat(s.to_string()))?;
        let type_size = s.chars().filter(|&c| c == 'T').count();
        Ok(Self {
            header_length,
            compression_index,
            compression_size,
            checksum_index,
            checksum_size,
            length_index,
            length_size,
            type_index,
            type_size,
        })
    }
}

/// Supported compression types.
#[repr(u8)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum GCompressionTypeV6 {
    /// No compression.
    CompressionNone = 0,
    /// Zlib compression.
    CompressionZlib = 1,
    /// Bzip2 compression.
    CompressionBzip2 = 2,
}

/// Incoming vs outgoing packet types.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PacketDirection {
    /// Packets coming from the server.
    Incoming,
    /// Packets coming from the client.
    Outgoing,
}

impl TryFrom<u8> for GCompressionTypeV6 {
    type Error = ProtocolError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::try_from(value as usize)
    }
}

impl TryFrom<usize> for GCompressionTypeV6 {
    type Error = ProtocolError;
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(GCompressionTypeV6::CompressionNone),
            1 => Ok(GCompressionTypeV6::CompressionZlib),
            2 => Ok(GCompressionTypeV6::CompressionBzip2),
            _ => Err(ProtocolError::InvalidCompression(value as u8)),
        }
    }
}

impl From<GCompressionTypeV6> for u8 {
    fn from(comp: GCompressionTypeV6) -> Self {
        comp as u8
    }
}

impl From<GCompressionTypeV6> for usize {
    fn from(comp: GCompressionTypeV6) -> Self {
        comp as usize
    }
}

/// A parsed header for a single packet.
#[derive(Debug)]
pub struct GProtocolV6PacketHeader {
    /// The compression type.
    pub compression: GCompressionTypeV6,
    /// The checksum.
    pub checksum: u8,
    /// The length of the packet.
    pub length: usize,
    /// The packet ID.
    pub id: PacketId,
}

impl GProtocolV6HeaderFormat {
    /// Parse the header from a given byte slice.
    pub fn parse_header(
        &self,
        packet_direction: PacketDirection,
        buffer: &[u8],
    ) -> Result<GProtocolV6PacketHeader, ProtocolError> {
        if buffer.len() != self.header_length {
            return Err(ProtocolError::InvalidHeader(format!(
                "Invalid header length: {} != {}",
                buffer.len(),
                self.header_length
            )));
        }
        let compression = GCompressionTypeV6::try_from(Self::get_num_from_buffer(
            buffer,
            self.compression_index,
            self.compression_size,
        )?)?;
        let checksum = Self::get_num_from_buffer(buffer, self.checksum_index, self.checksum_size)?;
        let length = Self::get_num_from_buffer(buffer, self.length_index, self.length_size)?;
        let packet_id = Self::get_num_from_buffer(buffer, self.type_index, self.type_size)?;
        let packet_id = match packet_direction {
            PacketDirection::Incoming => {
                PacketId::FromServer(FromServerPacketId::try_from(packet_id)?)
            }
            PacketDirection::Outgoing => {
                PacketId::FromClient(FromClientPacketId::try_from(packet_id)?)
            }
        };
        Ok(GProtocolV6PacketHeader {
            compression,
            checksum: checksum as u8,
            length,
            id: packet_id,
        })
    }

    /// Create a header buffer from a `NewProtocolPacketHeader`.
    pub fn create_header(
        &self,
        header: &GProtocolV6PacketHeader,
    ) -> Result<Vec<u8>, ProtocolError> {
        let mut buffer = vec![0u8; self.header_length];
        let compression_buf =
            Self::get_buffer_from_num(header.compression.into(), self.compression_size)?;
        let checksum_buf = Self::get_buffer_from_num(header.checksum as usize, self.checksum_size)?;
        let length_buf = Self::get_buffer_from_num(header.length, self.length_size)?;
        let type_buf = Self::get_buffer_from_num(header.id.into(), self.type_size)?;
        buffer[self.compression_index..self.compression_index + self.compression_size]
            .copy_from_slice(&compression_buf);
        buffer[self.checksum_index..self.checksum_index + self.checksum_size]
            .copy_from_slice(&checksum_buf);
        buffer[self.length_index..self.length_index + self.length_size]
            .copy_from_slice(&length_buf);
        buffer[self.type_index..self.type_index + self.type_size].copy_from_slice(&type_buf);
        Ok(buffer)
    }

    /// Helper: extract an integer from a buffer at a given index and size (big-endian).
    fn get_num_from_buffer(
        buffer: &[u8],
        index: usize,
        size: usize,
    ) -> Result<usize, ProtocolError> {
        if index + size > buffer.len() {
            return Err(ProtocolError::InvalidHeaderFormat(
                "Buffer too short".to_string(),
            ));
        }
        let mut value = usize::default();
        for i in 0..size {
            value = (value << 8) | buffer[index + i] as usize;
        }
        Ok(value)
    }

    /// Helper: produce a big-endian byte buffer for an integer.
    fn get_buffer_from_num(value: usize, size: usize) -> Result<Vec<u8>, ProtocolError> {
        let mut buffer = vec![0u8; size];
        for i in 0..size {
            buffer[size - 1 - i] = (value >> (8 * i)) as u8;
        }
        Ok(buffer)
    }
}
