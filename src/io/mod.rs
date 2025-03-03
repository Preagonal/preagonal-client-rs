#![deny(missing_docs)]

use thiserror::Error;

/// A module defining asynchronous IO operations.
pub mod io_async;
/// A module defining synchronous IO operations.
pub mod io_sync;
/// A module defining how a vector can be used as a reader or writer.
pub mod io_vec;

/// The maximum value for a graal-encoded gu8.
pub const GUINT8_MAX: u64 = 0xDF;
/// The maximum value for a graal-encoded gu16.
pub const GUINT16_MAX: u64 = 0x705F;
/// The maximum value for a graal-encoded gu24.
pub const GUINT24_MAX: u64 = 0x38305F;
/// The maximum value for a graal-encoded gu32.
pub const GUINT32_MAX: u64 = 0x1C18305F;
/// The maximum value for a graal-encoded gu40.
pub const GUINT40_MAX: u64 = 0xFFFFFFFF;

/// An error type for Graal IO operations.
#[derive(Debug, Error)]
pub enum GraalIoError {
    /// The value exceeds the maximum for a Graal-encoded integer.
    #[error(
        "Value exceeds maximum for Graal-encoded integer. Value was {0}, but cannot exceed {1}."
    )]
    ValueExceedsMaximum(u64, u64),

    /// An IO error occurred.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// An error occurred while encoding or decoding a Graal value.
    #[error("Other: {0}")]
    Other(String),
}

/// A helper for encoding and decoding Graal values.
pub struct GraalCodec;

impl GraalCodec {
    /// Decodes a slice of bytes using the Graal encoding.
    pub fn decode_bits(slice: &[u8]) -> u64 {
        let mut value = 0;
        for (i, &byte) in slice.iter().enumerate() {
            let chunk = (byte - 32) as u64;
            let shift = 7 * (slice.len() - 1 - i);
            value += chunk << shift;
        }
        value
    }

    /// Encodes a value into a Graal-encoded byte vector of length `byte_count`.
    pub fn encode_bits(mut value: u64, byte_count: usize) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(byte_count);
        for i in 0..byte_count {
            let shift = 7 * (byte_count - 1 - i);
            let chunk = (value >> shift) & 0x7F;
            buffer.push((chunk as u8) + 32);
            value -= chunk << shift;
        }
        buffer
    }
}
