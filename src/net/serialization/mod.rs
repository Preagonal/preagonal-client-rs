#![deny(missing_docs)]

use serde::{Deserialize, Serialize};

/// This module defines the error types for serialization.
pub mod error;
/// This module handles the serialization of packets.
pub mod serialize;

/// A GString is a string that is serialized as a length-preceded string.
#[derive(Debug, Serialize, Deserialize)]
pub struct GString(pub String);

/// Allow conversion from GString to String.
impl From<GString> for String {
    fn from(g: GString) -> Self {
        g.0
    }
}

/// Allow conversion from String to GString.
impl From<String> for GString {
    fn from(s: String) -> Self {
        GString(s)
    }
}

/// Allow conversion from &str to GString.
impl From<&str> for GString {
    fn from(s: &str) -> Self {
        GString(s.to_string())
    }
}

/// A GScript is a string that repalces \r with "" and \n with "\xa7".
#[derive(Debug, Serialize, Deserialize)]
pub struct GScript(pub String);

/// Allow conversion from GScript to String.
impl From<GScript> for String {
    fn from(g: GScript) -> Self {
        g.0
    }
}

/// Allow conversion from String to GScript.
impl From<String> for GScript {
    fn from(s: String) -> Self {
        GScript(s)
    }
}

/// Allow conversion from &str to GScript.
impl From<&str> for GScript {
    fn from(s: &str) -> Self {
        GScript(s.to_string())
    }
}
