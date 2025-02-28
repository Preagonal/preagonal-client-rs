#![deny(missing_docs)]

use std::time::Duration;

/// A struct that contains the configuration for the NpcControl client.
pub struct NcConfig {
    /// The host of the server.
    pub host: String,
    /// The port of the server.
    pub port: u16,
    /// The login configuration.
    pub login: NcLoginConfig,
    /// The timeout for the connection.
    pub timeout: Duration,
}

/// A struct that contains the NcLoginConfig
pub struct NcLoginConfig {
    /// The username of the client.
    pub username: String,
    /// The password of the client.
    pub password: String,
    /// The identification of the client.
    pub identification: Vec<String>,
}
