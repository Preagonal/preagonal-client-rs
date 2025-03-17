use std::{sync::Arc, time::Duration};

use config::{ClientConfig, ClientType};
use discord::PreagonalBotEventHandler;
use net::client::{ClientError, GClientTrait, game::GameClient, rc::RemoteControlClient};

use serenity::{Client, prelude::*};

/// The config module contains the configuration for the bot.
pub mod config;
/// The consts module contains constant values.
pub mod consts;
/// The discord module contains discord utilities.
pub mod discord;
/// The io module contains IO utilities.
pub mod io;
/// The net module contains networking utilities.
pub mod net;
/// The utils module contains utility functions.
pub mod utils;

#[tokio::main]
async fn main() {
    // Initialize logging.
    log4rs::init_file("logging_config.yaml", Default::default())
        .expect("Failed to initialize logging.");

    // console_subscriber::init();

    // Load configuration.
    let config = config::get_config();

    let mut rc_clients: Vec<Arc<RemoteControlClient>> = Vec::new();
    let mut game_clients: Vec<Arc<GameClient>> = Vec::new();

    for server in &config.clients {
        match server.client_type {
            ClientType::Game(_) => {
                let client = create_game_client(server).await;
                game_clients.push(client.expect("Error creating game client"));
            }
            ClientType::RemoteControl(_) => {
                let rc = create_rc_client(server).await;
                rc_clients.push(rc.expect("Error creating remote control client"));
            }
            ClientType::NpcControl(_) => {
                log::warn!("Unsupported client type: NpcControl");
            }
        }
        // sleep for 5 seconds to avoid spamming the server
        // TODO: Find a better way?
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // == Discord ==
    let guild_id = config.discord.server_id;
    let channel_id = config.discord.channel_id;

    // Set up the Discord client with our custom event handler.
    let intents = GatewayIntents::GUILD_MESSAGES;
    let token = config.discord.token.clone();
    let handler = PreagonalBotEventHandler::new(
        rc_clients.clone(),
        game_clients.clone(),
        guild_id,
        channel_id,
    );

    let mut client = Client::builder(token, intents)
        .event_handler(handler)
        .await
        .expect("Error creating Discord client");

    // Run the Discord client in its own task.
    tokio::spawn(async move {
        client.start().await.expect("Error running Discord client");
    });

    // Now, sleep forever.
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

/// Create a GameClient
pub async fn create_game_client(client: &ClientConfig) -> Result<Arc<GameClient>, ClientError> {
    let login = client.login.clone();

    log::info!(
        "Connecting to server: {}:{} with login: {}",
        client.host,
        client.port,
        login.auth.account_name
    );

    let client = GameClient::new(client)?;

    client.connect().await.expect("Error connecting to server");

    // Register a disconnect handler that automatically reconnects.
    client
        .register_disconnect_handler({
            let client = Arc::clone(&client);
            move || {
                let client = Arc::clone(&client);
                async move {
                    log::warn!("Disconnected from server, reconnecting after 5 seconds...");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    client.connect().await.expect("Error reconnecting");
                }
            }
        })
        .await
        .expect("Error registering disconnect handler");

    client.login().await.expect("Error logging in");

    Ok(client)
}

/// Create a RemoteControlClient
/// Create a RemoteControlClient
pub async fn create_rc_client(
    client: &ClientConfig,
) -> Result<Arc<RemoteControlClient>, ClientError> {
    let login = client.login.clone();

    log::info!(
        "Connecting to server: {}:{} with login: {}",
        client.host,
        client.port,
        login.auth.account_name
    );

    let rc = RemoteControlClient::new(client).await?;
    rc.connect().await.expect("Error connecting to server");

    rc.register_disconnect_handler({
        let rc = Arc::clone(&rc);
        move || {
            let rc = Arc::clone(&rc);
            async move {
                log::warn!("Disconnected from server, reconnecting after 5 seconds...");

                // Clear the internal client reference before reconnecting
                {
                    log::debug!("Attempting to clear client reference");
                    let mut client_guard = rc.client.write().await;
                    *client_guard = None;
                    log::debug!("Cleared client reference");
                }

                // Now try to reconnect
                match rc.connect().await {
                    Ok(_) => match rc.login().await {
                        Ok(_) => log::info!("Successfully reconnected and logged in"),
                        Err(e) => log::error!("Failed to login after reconnect: {:?}", e),
                    },
                    Err(e) => log::error!("Failed to reconnect: {:?}", e),
                }
            }
        }
    })
    .await
    .expect("Error registering disconnect handler");

    rc.login().await.expect("Error logging in");

    Ok(rc)
}
