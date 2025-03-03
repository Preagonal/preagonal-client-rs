#![deny(missing_docs)]

use serde::{Deserialize, Deserializer};
use serenity::all::{ChannelId, GuildId};
use std::sync::OnceLock;

static CONFIG: OnceLock<Config> = OnceLock::new();

/// RcClient, or GameClient
#[derive(Debug, Deserialize, Clone)]
pub enum ClientType {
    /// The RemoteControl client.
    RemoteControl,
    /// The Game client.
    Game,
}

/// Get the configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    /// The server to connect to.
    #[serde(default)]
    pub clients: Vec<ClientConfig>,
    /// The discord configuration
    pub discord: DiscordConfig,
}

/// The configuration for a single login
#[derive(Debug, Deserialize, Clone)]
pub struct LoginConfig {
    /// The nickname to use
    #[serde(default)]
    pub nick: Option<String>,
    #[serde(default)]
    /// The system ids to use
    pub identification: Vec<String>,
    /// The auth to use
    pub auth: LoginAuth,
}

/// The login auth
#[derive(Debug, Deserialize, Clone)]
pub struct LoginAuth {
    /// The username
    pub account_name: String,
    /// The password
    pub password: String,
}

/// The server configuration
#[derive(Debug, Deserialize, Clone)]
pub struct ClientConfig {
    /// The host to connect to
    pub host: String,
    /// The port to connect to
    pub port: u16,
    /// The type of client to use
    pub client_type: ClientType,
    /// The login to use for this server
    pub login: LoginConfig,
}

/// The discord configuration
#[derive(Debug, Deserialize, Clone)]
pub struct DiscordConfig {
    /// The token to use
    pub token: String,
    /// The channel to use
    #[serde(deserialize_with = "deserialize_channel_id")]
    pub channel_id: ChannelId,
    /// The guild to use
    #[serde(deserialize_with = "deserialize_guild_id")]
    pub server_id: GuildId,
}

/// Deserialize a channel id
fn deserialize_channel_id<'de, D>(deserializer: D) -> Result<ChannelId, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse().map_err(serde::de::Error::custom)
}

/// Deserialize a guild id
fn deserialize_guild_id<'de, D>(deserializer: D) -> Result<GuildId, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse().map_err(serde::de::Error::custom)
}

/// Get or init config from `client.toml`
// TODO(@ropguy): Return Result instead of panic
pub fn get_config() -> &'static Config {
    CONFIG.get_or_init(|| {
        let config =
            std::fs::read_to_string("config/client.toml").expect("Unable to read client.toml");
        toml::from_str(&config).expect("Unable to parse client.toml")
    })
}
