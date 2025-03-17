use crate::io::{
    GUINT8_MAX, GUINT16_MAX, GUINT24_MAX, GUINT32_MAX, GUINT40_MAX, GraalCodec, GraalIoError,
};
use tokio::io::{
    AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter,
};

/// An asynchronous Graal reader.
pub struct AsyncGraalReader<R: AsyncRead + Unpin> {
    pub(crate) inner: BufReader<R>,
}

impl<R: AsyncRead + Unpin> AsyncGraalReader<R> {
    /// Constructs the reader from any async reader.
    pub fn from_reader(inner: R) -> Self {
        Self {
            inner: BufReader::new(inner),
        }
    }

    /// Reads exactly `n` bytes.
    pub async fn read_exact(&mut self, n: usize) -> Result<Vec<u8>, GraalIoError> {
        let mut buffer = vec![0u8; n];
        self.inner.read_exact(&mut buffer).await?;
        Ok(buffer)
    }

    /// Read until EOF
    pub async fn read_to_end(&mut self) -> Result<Vec<u8>, GraalIoError> {
        let mut buffer = Vec::new();
        self.inner.read_to_end(&mut buffer).await?;
        Ok(buffer)
    }

    /// Return a Vec<Vec<u8>> based on the delimiter, until EOF.
    pub async fn split_vec(&mut self, delimiter: u8) -> Result<Vec<Vec<u8>>, GraalIoError> {
        let mut result = Vec::new();
        loop {
            let bytes = self.read_until(delimiter).await?;
            if bytes.is_empty() {
                break;
            }
            result.push(bytes);
        }
        Ok(result)
    }

    /// Reads until the specified delimiter is found (excluding the delimiter). If the delimiter is not found, the buffer will contain all bytes read until EOF.
    pub async fn read_until(&mut self, delimiter: u8) -> Result<Vec<u8>, GraalIoError> {
        let mut buffer = Vec::new();
        let bytes_read = self.inner.read_until(delimiter, &mut buffer).await?;
        if bytes_read == 0 || buffer.last() != Some(&delimiter) {
            return Ok(buffer);
        }
        buffer.pop(); // remove the delimiter
        Ok(buffer)
    }

    /// Reads a null-terminated string.
    pub async fn read_string(&mut self) -> Result<(String, u32), GraalIoError> {
        let bytes = self.read_until(0).await?;
        let s = bytes.iter().map(|&c| c as char).collect::<String>();
        Ok((s, bytes.len() as u32))
    }

    /// Reads a Graal-encoded string (with a length prefix).
    pub async fn read_gstring(&mut self) -> Result<String, GraalIoError> {
        let length = self.read_gu8().await? as usize;
        let bytes = self.read_exact(length).await?;
        Ok(bytes.iter().map(|&c| c as char).collect::<String>())
    }

    /// Reads a single byte.
    pub async fn read_u8(&mut self) -> Result<u8, GraalIoError> {
        let mut buf = [0u8; 1];
        self.inner.read_exact(&mut buf).await?;
        Ok(buf[0])
    }

    /// Reads a big-endian unsigned 16-bit integer.
    pub async fn read_u16(&mut self) -> Result<u16, GraalIoError> {
        let mut buf = [0u8; 2];
        self.inner.read_exact(&mut buf).await?;
        Ok(u16::from_be_bytes(buf))
    }

    /// Reads a big-endian unsigned 32-bit integer.
    pub async fn read_u32(&mut self) -> Result<u32, GraalIoError> {
        let mut buf = [0u8; 4];
        self.inner.read_exact(&mut buf).await?;
        Ok(u32::from_be_bytes(buf))
    }

    /// Reads a Graal-encoded unsigned integer (with `byte_count` bytes).
    pub async fn read_gu(&mut self, byte_count: usize) -> Result<u64, GraalIoError> {
        let bytes = self.read_exact(byte_count).await?;
        Ok(GraalCodec::decode_bits(&bytes))
    }

    /// Reads a Graal-encoded unsigned 8-bit integer.
    pub async fn read_gu8(&mut self) -> Result<u64, GraalIoError> {
        self.read_gu(1).await
    }
    /// Reads a Graal-encoded unsigned 16-bit integer.
    pub async fn read_gu16(&mut self) -> Result<u64, GraalIoError> {
        self.read_gu(2).await
    }
    /// Reads a Graal-encoded unsigned 24-bit integer.
    pub async fn read_gu24(&mut self) -> Result<u64, GraalIoError> {
        self.read_gu(3).await
    }
    /// Reads a Graal-encoded unsigned 32-bit integer.
    pub async fn read_gu32(&mut self) -> Result<u64, GraalIoError> {
        self.read_gu(4).await
    }
    /// Reads a Graal-encoded unsigned 40-bit integer.
    pub async fn read_gu40(&mut self) -> Result<u64, GraalIoError> {
        self.read_gu(5).await
    }
}

/// An asynchronous Graal writer.
pub struct AsyncGraalWriter<W: AsyncWrite + Unpin> {
    inner: BufWriter<W>,
}

impl<W: AsyncWrite + Unpin> AsyncGraalWriter<W> {
    /// Constructs the writer from any async writer.
    pub fn from_writer(inner: W) -> Self {
        Self {
            inner: BufWriter::new(inner),
        }
    }

    /// Writes the given bytes.
    pub async fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), GraalIoError> {
        self.inner.write_all(bytes).await?;
        Ok(())
    }

    /// Shutdown the writer.
    pub async fn shutdown(&mut self) -> Result<(), GraalIoError> {
        self.inner.shutdown().await?;
        Ok(())
    }

    /// Flushes the writer.
    pub async fn flush(&mut self) -> Result<(), GraalIoError> {
        self.inner.flush().await?;
        Ok(())
    }

    /// Writes a null-terminated string.
    pub async fn write_string(&mut self, s: &str) -> Result<(), GraalIoError> {
        self.write_bytes(s.as_bytes()).await?;
        self.write_bytes(&[0]).await?;
        Ok(())
    }

    /// Writes a Graal-encoded string (with a length prefix).
    pub async fn write_gstring(&mut self, s: &str) -> Result<(), GraalIoError> {
        let length = s.len();
        self.write_gu8(length as u64).await?;
        self.write_bytes(s.as_bytes()).await?;
        Ok(())
    }

    /// Writes a single byte.
    pub async fn write_u8(&mut self, v: u8) -> Result<(), GraalIoError> {
        self.write_bytes(&[v]).await
    }
    /// Writes a big-endian unsigned 16-bit integer.
    pub async fn write_u16(&mut self, v: u16) -> Result<(), GraalIoError> {
        self.write_bytes(&v.to_be_bytes()).await
    }
    /// Writes a big-endian unsigned 32-bit integer.
    pub async fn write_u32(&mut self, v: u32) -> Result<(), GraalIoError> {
        self.write_bytes(&v.to_be_bytes()).await
    }

    /// Writes a Graal-encoded unsigned integer (with `byte_count` bytes).
    pub async fn write_gu(&mut self, byte_count: usize, v: u64) -> Result<(), GraalIoError> {
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
        self.write_bytes(&buffer).await
    }

    /// Writes a Graal-encoded unsigned 8-bit integer.
    pub async fn write_gu8(&mut self, v: u64) -> Result<(), GraalIoError> {
        self.write_gu(1, v).await
    }
    /// Writes a Graal-encoded unsigned 16-bit integer.
    pub async fn write_gu16(&mut self, v: u64) -> Result<(), GraalIoError> {
        self.write_gu(2, v).await
    }
    /// Writes a Graal-encoded unsigned 24-bit integer.
    pub async fn write_gu24(&mut self, v: u64) -> Result<(), GraalIoError> {
        self.write_gu(3, v).await
    }
    /// Writes a Graal-encoded unsigned 32-bit integer.
    pub async fn write_gu32(&mut self, v: u64) -> Result<(), GraalIoError> {
        self.write_gu(4, v).await
    }
    /// Writes a Graal-encoded unsigned 40-bit integer.
    pub async fn write_gu40(&mut self, v: u64) -> Result<(), GraalIoError> {
        self.write_gu(5, v).await
    }
}
