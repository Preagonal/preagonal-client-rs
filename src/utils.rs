#![deny(missing_docs)]

use bzip2::read::BzDecoder;
use flate2::Compression;
use flate2::read::{ZlibDecoder, ZlibEncoder};
use std::io::prelude::*;

/// Decompress zlib data.
pub fn decompress_zlib(data: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut decoder = ZlibDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)?;
    Ok(decompressed)
}

/// Compress data using zlib.
pub fn compress_zlib(data: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(data, Compression::default());
    let mut compressed = Vec::new();
    encoder.read_to_end(&mut compressed)?;
    Ok(compressed)
}

/// Decompress bzip2 data.
pub fn decompress_bzip2(data: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut decoder = BzDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)?;
    Ok(decompressed)
}

/// Compress data using bzip2.
pub fn compress_bzip2(data: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut encoder = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
    encoder.write_all(data)?;
    encoder.finish()
}
