#![deny(missing_docs)]

use std::time::Duration;

/// A struct that contains the configuration for the RemoteControl client.
pub struct RcConfig {
    /// The host of the server.
    pub host: String,
    /// The port of the server.
    pub port: u16,
    /// The login configuration.
    pub login: RcLoginConfig,
    /// The timeout for events that have a response.
    pub timeout: Duration,
    /// When we should automatically disconnect the NpcControl.
    pub nc_auto_disconnect: Duration,
}

/// A struct that contains the RcLoginConfig
pub struct RcLoginConfig {
    /// The username of the client.
    pub username: String,
    /// The password of the client.
    pub password: String,
    /// The identification of the client.
    pub identification: Vec<String>,
}
