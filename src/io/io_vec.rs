use super::{
    GraalIoError,
    io_sync::{SyncGraalReader, SyncGraalWriter},
};
use std::io::{BufRead, Cursor};

/// Extension trait for converting a read-only slice reference into a SyncGraalReader.
pub trait IntoSyncGraalReaderRef<'a> {
    /// Converts the value into a SyncGraalReader.
    fn into_sync_graal_reader(self) -> SyncGraalReader<Cursor<&'a [u8]>>;
}

impl<'a> IntoSyncGraalReaderRef<'a> for &'a [u8] {
    fn into_sync_graal_reader(self) -> SyncGraalReader<Cursor<&'a [u8]>> {
        SyncGraalReader::from_reader(Cursor::new(self))
    }
}

/// Extension trait for converting a mutable vector reference into a SyncGraalWriter.
/// The returned writer wraps a Cursor over a mutable reference so that any writes
/// will directly modify the caller’s vector without cloning.
pub trait IntoSyncGraalWriterRef<'a> {
    /// Converts the value into a SyncGraalWriter.
    fn into_sync_graal_writer(self) -> SyncGraalWriter<Cursor<&'a mut Vec<u8>>>;
}

impl<'a> IntoSyncGraalWriterRef<'a> for &'a mut Vec<u8> {
    fn into_sync_graal_writer(self) -> SyncGraalWriter<Cursor<&'a mut Vec<u8>>> {
        SyncGraalWriter::from_writer(Cursor::new(self))
    }
}

/// Extension trait for converting a vector into a SyncGraalWriter.
/// The returned writer wraps a Cursor over a mutable reference so that any writes
/// will directly modify the caller’s vector without cloning.
pub trait IntoSyncGraalReader {
    /// Converts the value into a SyncGraalReader.
    fn into_sync_graal_reader(self) -> SyncGraalReader<Cursor<Vec<u8>>>;
}

impl IntoSyncGraalReader for Vec<u8> {
    fn into_sync_graal_reader(self) -> SyncGraalReader<Cursor<Vec<u8>>> {
        SyncGraalReader::from_reader(Cursor::new(self))
    }
}

impl IntoSyncGraalReader for &[u8] {
    fn into_sync_graal_reader(self) -> SyncGraalReader<Cursor<Vec<u8>>> {
        SyncGraalReader::from_reader(Cursor::new(self.to_vec()))
    }
}

/// This trait allows conversion to the SyncGraalWriter type.
pub trait IntoSyncGraalWriter {
    /// Converts the value into a SyncGraalWriter.
    fn into_sync_graal_writer(self) -> SyncGraalWriter<Cursor<Vec<u8>>>;
}

impl IntoSyncGraalWriter for Vec<u8> {
    fn into_sync_graal_writer(self) -> SyncGraalWriter<Cursor<Vec<u8>>> {
        SyncGraalWriter::from_writer(Cursor::new(self))
    }
}

impl IntoSyncGraalWriter for &mut Vec<u8> {
    fn into_sync_graal_writer(self) -> SyncGraalWriter<Cursor<Vec<u8>>> {
        // This version consumes the data by taking it out of the reference.
        let vec = std::mem::take(self);
        SyncGraalWriter::from_writer(Cursor::new(vec))
    }
}

/// Extension trait that adds can_read for in‑memory readers.
pub trait InMemoryGraalReaderExt {
    /// Returns whether there are bytes available to be read.
    /// Note: This makes sense for in‑memory buffers, where the underlying data is fixed.
    fn can_read(&mut self) -> Result<bool, GraalIoError>;
}

impl InMemoryGraalReaderExt for SyncGraalReader<Cursor<Vec<u8>>> {
    fn can_read(&mut self) -> Result<bool, GraalIoError> {
        // Using fill_buf() on the BufReader gives us a slice of buffered data.
        let buf = self.inner.fill_buf()?;
        Ok(!buf.is_empty())
    }
}

impl InMemoryGraalReaderExt for SyncGraalReader<Cursor<&[u8]>> {
    fn can_read(&mut self) -> Result<bool, GraalIoError> {
        // Using fill_buf() on the BufReader gives us a slice of buffered data.
        let buf = self.inner.fill_buf()?;
        Ok(!buf.is_empty())
    }
}
