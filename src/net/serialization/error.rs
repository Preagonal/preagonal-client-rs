use std::fmt::Display;

use serde::de;
use thiserror::Error;

use crate::io::GraalIoError;

/// An error type for Graal IO operations.
#[derive(Debug, Error)]
pub enum GraalSerializationError {
    /// An IO error occurred.
    #[error("IO error: {0}")]
    Io(#[from] GraalIoError),

    /// An error occurred while encoding or decoding a Graal value.
    #[error("Other: {0}")]
    Other(String),
}

impl serde::ser::Error for GraalSerializationError {
    fn custom<T: Display>(msg: T) -> Self {
        GraalSerializationError::Other(msg.to_string())
    }
}

impl de::Error for GraalSerializationError {
    fn custom<T: Display>(msg: T) -> Self {
        GraalSerializationError::Other(msg.to_string())
    }
}
