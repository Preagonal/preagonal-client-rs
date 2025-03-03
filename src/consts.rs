#![deny(missing_docs)]

/// The host of the server.
pub const SERVER_HOST: &str = "198.27.86.142";

/// The port of the server.
pub const SERVER_PORT: u16 = 14801;

/// The version of the RC Protocol.
pub const RC_PROTOCOL_VERSION: &str = "GSERV025";

/// The version of the NC Protocol.
pub const NC_PROTOCOL_VERSION: &str = "NCL21075";

/// The version of the Game Protocol.
pub const GAME_PROTOCOL_VERSION: &str = "G3D2204D";

/// V6 Protocol String
pub const V6_PROTOCOL_STRING: &str = "GNP1905C";

/// Encryption parse key for server -> client
pub const GAME_CLIENT_PRIVATE_KEY: &str =
    include_str!("../config/keys/game_client_private_key.pem");
