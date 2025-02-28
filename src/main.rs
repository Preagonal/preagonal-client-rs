use std::sync::Arc;

use discord::commands::sendtorc;
use net::{
    packet::{
        GPacket, PacketId::FromServer, from_client::rc_login::RcLogin,
        from_server::FromServerPacketId,
    },
    protocol::{Protocol, proto_v5::GProtocolV5},
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
use tokio::sync::{Mutex, mpsc};
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

struct PacketEvent {
    packet: Box<dyn GPacket + Send>,
}

/// We derive Clone so we can easily capture fields.
#[derive(Clone)]
struct Handler {
    from_server_rx: Arc<Mutex<mpsc::Receiver<PacketEvent>>>,
    from_client_tx: mpsc::Sender<PacketEvent>,
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

            if let Some(content) = content {
                // Send the response using from_client_tx.
                if let Err(why) = self
                    .from_client_tx
                    .send(PacketEvent { packet: content })
                    .await
                {
                    log::error!("Error sending packet: {:?}", why);
                }
            }

            // Respond to the interaction.
            let response = CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new().content("Message sent to RC"),
            );
            if let Err(why) = command.create_response(&ctx.http, response).await {
                log::error!("Error responding to command: {:?}", why);
            }
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
        let packet_rx = Arc::clone(&self.from_server_rx);
        let channel_id = self.channel_id; // ChannelId is Copy.
        let http = ctx.http.clone();

        tokio::spawn(async move {
            let mut rx = packet_rx.lock().await;
            while let Some(event) = rx.recv().await {
                match event.packet.id() {
                    FromServer(packet_id) => {
                        if let FromServerPacketId::RcChat = packet_id {
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
                    }
                    _ => {
                        log::debug!("Received non-chat packet");
                    }
                }
            }
        });
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
            // Create channels for packet events.
            let (from_server_tx, from_server_rx) = mpsc::channel::<PacketEvent>(32);
            let (from_client_tx, mut from_client_rx) = mpsc::channel::<PacketEvent>(32);

            let guild_id = server.discord.server_id;
            let channel_id = server.discord.channel_id;
            let login = server.login.clone();

            // Set up the Discord client with our custom event handler.
            let intents = GatewayIntents::GUILD_MESSAGES;
            let token = server.discord.token.clone();
            let handler = Handler {
                from_server_rx: Arc::new(Mutex::new(from_server_rx)),
                from_client_tx: from_client_tx.clone(),
                guild_id,
                channel_id,
            };

            let mut client = Client::builder(token, intents)
                .event_handler(handler)
                .await
                .expect("Error creating Discord client");

            // Run the Discord client in its own task.
            tokio::spawn(async move {
                if let Err(why) = client.start().await {
                    log::error!("Discord client error: {:?}", why);
                }
            });

            let packet_tx = from_server_tx.clone();
            log::info!(
                "Connecting to server: {}:{} with login: {}",
                server.host,
                server.port,
                login.auth.account_name
            );

            // Connect to the TCP server.
            let stream = tokio::net::TcpStream::connect(format!("{}:{}", server.host, server.port))
                .await
                .expect("Failed to connect to server");
            let (reader, writer) = stream.into_split();
            let reader = tokio::io::BufReader::new(reader);
            let protocol = GProtocolV5::new(reader, writer);
            let encryption_key: u8 = rand::random::<u8>() & 0x7f;
            let login_packet = RcLogin::new(
                encryption_key,
                login.version,
                login.auth.account_name,
                login.auth.password,
                login.identification,
            );

            // Wrap the protocol in an Arc<Mutex<_>> so it can be shared safely.
            let protocol = Arc::new(Mutex::new(protocol));
            {
                let mut proto = protocol.lock().await;
                proto
                    .send_packet(&login_packet)
                    .await
                    .expect("Failed to send login packet");
                proto.set_encryption_key(encryption_key);
            }

            // Spawn a task to handle outgoing packets.
            let protocol_for_sending = Arc::clone(&protocol);
            tokio::spawn(async move {
                while let Some(event) = from_client_rx.recv().await {
                    let mut proto = protocol_for_sending.lock().await;
                    if let Err(e) = proto.send_packet(event.packet.as_ref()).await {
                        log::error!("Failed to send packet: {:?}", e);
                    }
                }
                log::error!("from_client_rx channel closed, stopping TCP send task");
            });

            // Start the read loop.
            loop {
                let packet_result = {
                    let mut proto = protocol.lock().await;
                    proto.read().await
                };
                match packet_result {
                    Ok(packet) => {
                        if packet_tx.send(PacketEvent { packet }).await.is_err() {
                            log::error!("Event handler dropped, stopping TCP read loop");
                            break;
                        }
                    }
                    Err(e) => {
                        log::error!("Error reading packet: {:?}", e);
                        break;
                    }
                }
            }
        });
    }

    server_tasks.join_all().await;
}
