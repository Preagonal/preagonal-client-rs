use std::{sync::Arc, time::Duration};

use consts::{NC_PROTOCOL_VERSION, RC_PROTOCOL_VERSION};
use discord::commands::{add_weapon, get_weapon, sendtorc};
use net::{
    client::{GClientConfig, GClientLoginConfig, rc::RemoteControlClient},
    packet::{PacketId, from_server::FromServerPacketId},
};
use serenity::{
    Client,
    all::{
        ChannelId, CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage,
        GuildId, Interaction, Ready,
    },
    async_trait,
    prelude::*,
};
use tokio::task::JoinSet;

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

/// We derive Clone so we can easily capture fields.
#[derive(Clone)]
struct Handler {
    rc: Arc<RemoteControlClient>,
    guild_id: GuildId,
    channel_id: ChannelId,
}

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            let response = match command.data.name.as_str() {
                "send_to_rc" => sendtorc::run(&command, self.rc.clone()).await,
                "get_weapon" => get_weapon::run(&command, self.rc.clone()).await,
                "add_weapon" => add_weapon::run(&command, self.rc.clone()).await,
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
        self.rc
            .client
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
}

#[tokio::main]
async fn main() {
    // Initialize logging.
    log4rs::init_file("logging_config.yaml", Default::default())
        .expect("Failed to initialize logging.");

    // console_subscriber::init();

    // Load configuration.
    let config = config::get_config();

    // Spawn TCP/network tasks for each server.
    let mut server_tasks = JoinSet::new();
    for server in &config.servers {
        // Clone the server config so we can move it into the task.
        let server = server.clone();
        server_tasks.spawn(async move {
            let login = server.login.clone();

            log::info!(
                "Connecting to server: {}:{} with login: {}",
                server.host,
                server.port,
                login.auth.account_name
            );

            // Create RcConfig
            let rc_config = GClientConfig {
                host: server.host,
                rc_protocol_version: RC_PROTOCOL_VERSION.to_string(),
                nc_protocol_version: NC_PROTOCOL_VERSION.to_string(),
                port: server.port,
                login: GClientLoginConfig {
                    username: login.auth.account_name,
                    password: login.auth.password,
                    identification: login.identification,
                },
                timeout: Duration::from_secs(5),
                nc_auto_disconnect: Duration::from_secs(60),
            };

            // Create RemoteControl with provided configuration.
            let rc = RemoteControlClient::connect(rc_config)
                .await
                .expect("Error connecting to server");

            rc.login().await.expect("Error logging in");

            // == Discord ==
            let guild_id = server.discord.server_id;
            let channel_id = server.discord.channel_id;

            // Set up the Discord client with our custom event handler.
            let intents = GatewayIntents::GUILD_MESSAGES;
            let token = server.discord.token.clone();
            let handler = Handler {
                rc: rc.clone(),
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

            rc.client.wait_for_tasks().await;
        });
    }

    server_tasks.join_all().await;
}
