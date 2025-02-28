use std::{sync::Arc, time::Duration};

use config::LoginConfig;
use discord::commands::sendtorc;
use net::{
    client::{
        nc::{NpcControl, config::NcConfig},
        rc::{
            RemoteControl,
            config::{RcConfig, RcLoginConfig},
        },
    },
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
    rc: Arc<RemoteControl>,
    guild_id: GuildId,
    channel_id: ChannelId,
}

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            let content = match command.data.name.as_str() {
                "sendtorc" => sendtorc::run(&command),
                _ => None,
            };

            if let Some(packet) = content {
                self.rc
                    .send_packet(packet)
                    .await
                    .expect("Error sending packet");

                // Respond to the interaction.
                let response = CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new().content("Message sent to RC"),
                );

                if let Err(why) = command.create_response(&ctx.http, response).await {
                    log::error!("Error responding to command: {:?}", why);
                }
                return;
            }

            log::warn!("Unknown command: {}", command.data.name);
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        log::info!("Discord client connected as {}", ready.user.name);

        // Set available commands.
        if let Err(why) = self
            .guild_id
            .set_commands(&ctx.http, vec![sendtorc::register()])
            .await
        {
            log::error!("Error setting commands: {:?}", why);
        } else {
            log::info!("Successfully set commands");
        }

        // Capture needed fields to avoid lifetime issues.
        let channel_id = self.channel_id; // ChannelId is Copy.
        let http = ctx.http.clone();

        // register event handler for RcChat
        self.rc
            .register_event_handler(
                PacketId::FromServer(FromServerPacketId::RcChat),
                move |_, event| {
                    let channel_id = channel_id.clone();
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
            let rc_config = RcConfig {
                host: server.host,
                port: server.port,
                login: RcLoginConfig {
                    username: login.auth.account_name,
                    password: login.auth.password,
                    identification: login.identification,
                },
                timeout: Duration::from_secs(15),
            };

            // Create RemoteControl with provided configuration.
            let rc = RemoteControl::connect(rc_config)
                .await
                .expect("Error connecting to RemoteControl");

            async fn handle_signature_event(rc: Arc<RemoteControl>, login: LoginConfig) {
                // Signature tells us that we're connected to the server.
                log::info!("Connected to the RC server.");

                // Now connect to the NPC server.
                let npc_server_address = rc.query_nc_addr().await;

                // unwrap npc_server_address
                let npc_server_address =
                    npc_server_address.expect("Error querying NPC server address");

                // TODO: This jank formatting will go away once we implement deserialization for packets.
                let mut data = npc_server_address
                    .data()
                    .iter()
                    .map(|&byte| byte as char)
                    .collect::<String>();
                data.remove(0);
                data.remove(0);
                let data: Vec<&str> = data.split(',').collect();
                let npc_host = data[0];
                let npc_port = data[1].parse::<u16>().expect("Error parsing port");

                // Connect to the NPC server.
                let nc_config = NcConfig {
                    host: npc_host.to_string(),
                    port: npc_port,
                    login: net::client::nc::config::NcLoginConfig {
                        username: login.auth.account_name.clone(),
                        password: login.auth.password.clone(),
                        identification: login.identification.clone(),
                    },
                    timeout: Duration::from_secs(15),
                };

                let nc = NpcControl::connect(nc_config)
                    .await
                    .expect("Error connecting to NpcControl");

                nc.login().await.expect("Error logging in");

                // Attempt to get weapon
                let weapon = nc
                    .add_weapon(
                        "test".to_string(),
                        "bcalarmclock.png".to_string(),
                        "function onCreated() {\n    echo(\"hi\");\n}".to_string(),
                    )
                    .await;
                log::info!("Weapon: {:?}", weapon);
            }

            rc.register_event_handler(
                PacketId::FromServer(FromServerPacketId::Signature),
                move |rc, _| Box::pin(handle_signature_event(rc, server.login.clone())),
            )
            .await;

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

            rc.wait_shutdown().await;
        });
    }

    server_tasks.join_all().await;
}
