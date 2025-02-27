use crate::io::{
    GUINT8_MAX, GUINT16_MAX, GUINT24_MAX, GUINT32_MAX, GUINT40_MAX, GraalCodec, GraalIoError,
};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};

/// A synchronous Graal reader.
pub struct SyncGraalReader<R: Read> {
    pub(crate) inner: BufReader<R>,
}

impl<R: Read> SyncGraalReader<R> {
    /// Constructs a reader from any type implementing `Read`.
    pub fn from_reader(inner: R) -> Self {
        Self {
            inner: BufReader::new(inner),
        }
    }

    /// Reads exactly `n` bytes.
    pub fn read_exact(&mut self, n: usize) -> Result<Vec<u8>, GraalIoError> {
        let mut buffer = vec![0u8; n];
        self.inner.read_exact(&mut buffer)?;
        Ok(buffer)
    }

    /// Reads until the specified delimiter is found (the delimiter is not included).
    pub fn read_until(&mut self, delimiter: u8) -> Result<Vec<u8>, GraalIoError> {
        let mut buffer = Vec::new();
        let bytes_read = self.inner.read_until(delimiter, &mut buffer)?;
        if bytes_read == 0 || buffer.last() != Some(&delimiter) {
            return Err(GraalIoError::ByteNotFound(delimiter));
        }
        buffer.pop(); // remove the delimiter
        Ok(buffer)
    }

    /// Reads a null-terminated string.
    pub fn read_string(&mut self) -> Result<(String, u32), GraalIoError> {
        let bytes = self.read_until(0)?;
        let s = bytes.iter().map(|&c| c as char).collect::<String>();
        Ok((s, bytes.len() as u32))
    }

    /// Reads a Graal-encoded string (with a length prefix).
    pub fn read_gstring(&mut self) -> Result<String, GraalIoError> {
        let length = self.read_gu8()? as usize;
        let bytes = self.read_exact(length)?;
        Ok(bytes.iter().map(|&c| c as char).collect::<String>())
    }

    /// Reads a single byte.
    pub fn read_u8(&mut self) -> Result<u8, GraalIoError> {
        let mut buf = [0u8; 1];
        self.inner.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    /// Reads a big-endian unsigned 16-bit integer.
    pub fn read_u16(&mut self) -> Result<u16, GraalIoError> {
        let mut buf = [0u8; 2];
        self.inner.read_exact(&mut buf)?;
        Ok(u16::from_be_bytes(buf))
    }

    /// Reads a big-endian unsigned 32-bit integer.
    pub fn read_u32(&mut self) -> Result<u32, GraalIoError> {
        let mut buf = [0u8; 4];
        self.inner.read_exact(&mut buf)?;
        Ok(u32::from_be_bytes(buf))
    }

    /// Reads a Graal-encoded unsigned integer (with `byte_count` bytes).
    pub fn read_gu(&mut self, byte_count: usize) -> Result<u64, GraalIoError> {
        let bytes = self.read_exact(byte_count)?;
        Ok(GraalCodec::decode_bits(&bytes))
    }

    /// Reads a Graal-encoded unsigned integer with 1 byte.
    pub fn read_gu8(&mut self) -> Result<u64, GraalIoError> {
        self.read_gu(1)
    }
    /// Reads a Graal-encoded unsigned integer with 2 bytes.
    pub fn read_gu16(&mut self) -> Result<u64, GraalIoError> {
        self.read_gu(2)
    }
    /// Reads a Graal-encoded unsigned integer with 3 bytes.
    pub fn read_gu24(&mut self) -> Result<u64, GraalIoError> {
        self.read_gu(3)
    }
    /// Reads a Graal-encoded unsigned integer with 4 bytes.
    pub fn read_gu32(&mut self) -> Result<u64, GraalIoError> {
        self.read_gu(4)
    }
    /// Reads a Graal-encoded unsigned integer with 5 bytes.
    pub fn read_gu40(&mut self) -> Result<u64, GraalIoError> {
        self.read_gu(5)
    }
}

/// A synchronous Graal writer.
pub struct SyncGraalWriter<W: Write> {
    inner: BufWriter<W>,
}

impl<W: Write> SyncGraalWriter<W> {
    /// Constructs the writer from any type implementing `Write`.
    pub fn from_writer(inner: W) -> Self {
        Self {
            inner: BufWriter::new(inner),
        }
    }

    /// Writes the given bytes.
    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), GraalIoError> {
        self.inner.write_all(bytes)?;
        Ok(())
    }

    /// Flushes the writer.
    pub fn flush(&mut self) -> Result<(), GraalIoError> {
        self.inner.flush()?;
        Ok(())
    }

    /// Writes a null-terminated string.
    pub fn write_string(&mut self, s: &str) -> Result<(), GraalIoError> {
        self.write_bytes(s.as_bytes())?;
        self.write_bytes(&[0])?;
        Ok(())
    }

    /// Writes a Graal-encoded string (with a length prefix).
    pub fn write_gstring(&mut self, s: &str) -> Result<(), GraalIoError> {
        let length = s.len();
        self.write_gu8(length as u64)?;
        self.write_bytes(s.as_bytes())?;
        Ok(())
    }

    /// Writes a single byte.
    pub fn write_u8(&mut self, v: u8) -> Result<(), GraalIoError> {
        self.write_bytes(&[v])
    }
    /// Writes a big-endian unsigned 16-bit integer.
    pub fn write_u16(&mut self, v: u16) -> Result<(), GraalIoError> {
        self.write_bytes(&v.to_be_bytes())
    }
    /// Writes a big-endian unsigned 32-bit integer.
    pub fn write_u32(&mut self, v: u32) -> Result<(), GraalIoError> {
        self.write_bytes(&v.to_be_bytes())
    }

    /// Writes a Graal-encoded unsigned integer (with `byte_count` bytes).
    pub fn write_gu(&mut self, byte_count: usize, v: u64) -> Result<(), GraalIoError> {
        let max = match byte_count {
            1 => GUINT8_MAX,
            2 => GUINT16_MAX,
            3 => GUINT24_MAX,
            4 => GUINT32_MAX,
            5 => GUINT40_MAX,
            _ => {
                return Err(GraalIoError::Other(format!(
                    "Unsupported byte count: {}",
                    byte_count
                )));
            }
        };
        if v > max {
            return Err(GraalIoError::ValueExceedsMaximum(v, max));
        }
        let buffer = GraalCodec::encode_bits(v, byte_count);
        self.write_bytes(&buffer)
    }

    /// Writes a Graal-encoded unsigned integer with 1 byte.
    pub fn write_gu8(&mut self, v: u64) -> Result<(), GraalIoError> {
        self.write_gu(1, v)
    }
    /// Writes a Graal-encoded unsigned integer with 2 bytes.
    pub fn write_gu16(&mut self, v: u64) -> Result<(), GraalIoError> {
        self.write_gu(2, v)
    }
    /// Writes a Graal-encoded unsigned integer with 3 bytes.
    pub fn write_gu24(&mut self, v: u64) -> Result<(), GraalIoError> {
        self.write_gu(3, v)
    }
    /// Writes a Graal-encoded unsigned integer with 4 bytes.
    pub fn write_gu32(&mut self, v: u64) -> Result<(), GraalIoError> {
        self.write_gu(4, v)
    }
    /// Writes a Graal-encoded unsigned integer with 5 bytes.
    pub fn write_gu40(&mut self, v: u64) -> Result<(), GraalIoError> {
        self.write_gu(5, v)
    }
}
