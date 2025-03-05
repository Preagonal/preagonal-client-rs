#![deny(missing_docs)]

use rsa::{RsaPrivateKey, pkcs8::DecodePrivateKey};
use serde::{Deserialize, Deserializer};
use serenity::all::{ChannelId, GuildId};
use std::{sync::OnceLock, time::Duration};

use crate::net::protocol::proto_v6_header::GProtocolV6HeaderFormat;

static CONFIG: OnceLock<Config> = OnceLock::new();

/// RcClient, or GameClient
#[derive(Debug, Deserialize, Clone)]
pub enum ClientType {
    /// The RemoteControl client.
    RemoteControl(RemoteControlConfig),
    /// The Game client.
    Game(GameConfig),
    /// The NpcControl client.
    NpcControl(NpcControlConfig),
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
    /// The system ids to use
    #[serde(default)]
    pub identification: Vec<String>,
}

/// A struct that contains the configuration for the NpcControlClient.
#[derive(Debug, Clone, Deserialize)]
pub struct NpcControlConfig {
    /// The protocol version of the NC client.
    pub version: String,
}

/// A struct that contains the configuration for the RemoteControlClient.
#[derive(Debug, Clone, Deserialize)]
pub struct RemoteControlConfig {
    /// The protocol version of the NC client.
    pub version: String,
    /// When we should automatically disconnect the NpcControl after a period of inactivity.
    #[serde(default = "default_nc_auto_disconnect")]
    pub nc_auto_disconnect: Duration,
    /// The NPC control configuration.
    #[serde(default)]
    pub npc_control: Option<NpcControlConfig>,
}

/// A struct that contains the configuration for the GameClient.
#[derive(Debug, Clone, Deserialize)]
pub struct GameConfig {
    /// The protocol version of the game client.
    pub version: String,
    /// Encryption keys.
    pub encryption_keys: EncryptionKeys,
    /// The header format.
    #[serde(deserialize_with = "deserialize_header_format")]
    pub header_format: GProtocolV6HeaderFormat,
}

/// Encryption keys provided by the caller.
#[derive(Debug, Clone, Deserialize)]
pub struct EncryptionKeys {
    /// The private key.
    #[serde(deserialize_with = "deserialize_private_key")]
    pub rsa_private_key: Box<RsaPrivateKey>,
    // TODO: The public key.
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
    /// The timeout for events that have a response.
    #[serde(default = "default_timeout")]
    pub timeout: Duration,
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

/// Get the default timeout
fn default_timeout() -> Duration {
    Duration::from_secs(5)
}

/// Get the default NC auto disconnect
fn default_nc_auto_disconnect() -> Duration {
    Duration::from_secs(60)
}

/// Deserialize a header format
fn deserialize_header_format<'de, D>(deserializer: D) -> Result<GProtocolV6HeaderFormat, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    GProtocolV6HeaderFormat::try_from(s.as_str()).map_err(serde::de::Error::custom)
}

/// Deserialize a private key, in PEM format
fn deserialize_private_key<'de, D>(deserializer: D) -> Result<Box<RsaPrivateKey>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let key = RsaPrivateKey::from_pkcs8_pem(&s).map_err(serde::de::Error::custom)?;
    Ok(Box::new(key))
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
