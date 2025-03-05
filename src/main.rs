use std::{sync::Arc, time::Duration};

use config::{ClientConfig, ClientType};
use discord::commands::{add_weapon, get_weapon, sendtorc};
use net::{
    client::{GClientTrait, game::GameClient, rc::RemoteControlClient},
    packet::{
        PacketId,
        from_server::{FromServerPacketId, weapon_script::NpcWeaponScript},
    },
};

use serenity::{
    Client,
    all::{
        ChannelId, CreateAttachment, CreateInteractionResponse, CreateInteractionResponseMessage,
        CreateMessage, GuildId, Interaction, Ready,
    },
    async_trait,
    prelude::*,
};

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

struct Handler {
    rc_clients: Vec<Arc<RemoteControlClient>>,
    game_clients: Vec<Arc<GameClient>>,
    guild_id: GuildId,
    channel_id: ChannelId,
}

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            let response = match command.data.name.as_str() {
                "send_to_rc" => sendtorc::run(&command, self.rc_clients.clone()).await,
                "get_weapon" => get_weapon::run(&command, self.rc_clients.clone()).await,
                "add_weapon" => add_weapon::run(&command, self.rc_clients.clone()).await,
                _ => {
                    log::warn!("No command found: {}", command.data.name);
                    Ok(CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new().content("No command found"),
                    ))
                }
            };

            if let Err(why) = response {
                log::error!("Error running command: {:?}", why);
                command
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content(format!("Error running command: {:?}", why)),
                        ),
                    )
                    .await
                    .expect("Error sending error response");
                return;
            }

            if let Err(why) = command.create_response(&ctx.http, response.unwrap()).await {
                log::error!("Error responding to Discord command: {:?}", why);
            }
            return;
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        log::info!("Discord client connected as {}", ready.user.name);

        // Set available commands.
        if let Err(why) = self
            .guild_id
            .set_commands(
                &ctx.http,
                vec![
                    sendtorc::register(),
                    get_weapon::register(),
                    add_weapon::register(),
                ],
            )
            .await
        {
            log::error!("Error setting Discord commands: {:?}", why);
        } else {
            log::info!("Successfully set Discord commands");
        }

        // Capture needed fields to avoid lifetime issues.
        let channel_id = self.channel_id; // ChannelId is Copy.
        let http = ctx.http.clone();

        // register event handler for RcChat
        for client in self.rc_clients.iter() {
            let http = http.clone(); // Clone http for each iteration
            client
                .register_event_handler(
                    PacketId::FromServer(FromServerPacketId::RcChat),
                    move |event| {
                        let channel_id = channel_id;
                        let http = http.clone();
                        async move {
                            let chat_message = event
                                .packet
                                .data()
                                .iter()
                                .map(|&byte| byte as char)
                                .collect::<String>();
                            if let Err(why) = channel_id
                                .send_message(&http, CreateMessage::new().content(chat_message))
                                .await
                            {
                                log::error!("Error sending message: {:?}", why);
                            }
                        }
                    },
                )
                .await;
        }

        // register event handler for NpcWeaponScript
        for client in self.game_clients.iter() {
            let http = http.clone(); // Clone http for each iteration
            client
                .register_event_handler(
                    PacketId::FromServer(FromServerPacketId::NpcWeaponScript),
                    move |event| {
                        let channel_id = channel_id;
                        let http = http.clone();
                        async move {
                            let packet = NpcWeaponScript::try_from(event.packet)
                                .expect("Error parsing weapon script packet");
                            log::info!("Received weapon script: {:?}", packet);

                            // Upload the weapon script to Discord as an attachment.
                            let script = packet.weapon_bytecode;
                            let script_name = packet.script_name;
                            let script_attachment = CreateMessage::new()
                                .content(format!("Weapon script: **{}**", script_name))
                                .add_file(CreateAttachment::bytes(
                                    script,
                                    format!("{}.gs2bc", script_name),
                                ));

                            // Create a message in the channel.
                            if let Err(why) =
                                channel_id.send_message(&http, script_attachment).await
                            {
                                log::error!("Error sending message: {:?}", why);
                            }
                        }
                    },
                )
                .await;
        }
    }
}

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
                game_clients.push(client);
            }
            ClientType::RemoteControl(_) => {
                let rc = create_rc_client(server).await;
                rc_clients.push(rc);
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
    let handler = Handler {
        rc_clients: rc_clients.clone(),
        game_clients: game_clients.clone(),
        guild_id,
        channel_id,
    };

    let mut client = Client::builder(token, intents)
        .event_handler(handler)
        .await
        .expect("Error creating Discord client");

    // Run the Discord client in its own task.
    tokio::spawn(async move {
        client.start().await.expect("Error running Discord client");
    });

    // Now, wait on all clients to finish.
    for client in rc_clients {
        // TODO: This waits for each client to finish before moving on to the next one.
        // We should wait for all clients to finish at the same time.
        client.wait_for_tasks().await;
    }

    for client in game_clients {
        // TODO: This waits for each client to finish before moving on to the next one.
        // We should wait for all clients to finish at the same time.
        client.wait_for_tasks().await;
    }
}

/// Create a GameClient
pub async fn create_game_client(client: &ClientConfig) -> Arc<GameClient> {
    let login = client.login.clone();

    log::info!(
        "Connecting to server: {}:{} with login: {}",
        client.host,
        client.port,
        login.auth.account_name
    );

    let client = GameClient::connect(client)
        .await
        .expect("Error connecting to server");

    client.login().await.expect("Error logging in");

    client
}

/// Create a RemoteControlClient
pub async fn create_rc_client(client: &ClientConfig) -> Arc<RemoteControlClient> {
    let login = client.login.clone();

    log::info!(
        "Connecting to server: {}:{} with login: {}",
        client.host,
        client.port,
        login.auth.account_name
    );

    let rc = RemoteControlClient::connect(client)
        .await
        .expect("Error connecting to server");

    rc.login().await.expect("Error logging in");

    rc
}
