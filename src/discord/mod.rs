#![deny(missing_docs)]

use std::sync::Arc;

use commands::{add_weapon, get_weapon, sendtorc};
use serenity::{
    all::{
        ChannelId, Context, CreateAttachment, CreateInteractionResponse,
        CreateInteractionResponseFollowup, CreateInteractionResponseMessage, CreateMessage,
        EventHandler, GuildId, Interaction, Ready,
    },
    async_trait,
};

use crate::net::{
    client::{GClientTrait, game::GameClient, rc::RemoteControlClient},
    packet::{
        PacketId,
        from_server::{FromServerPacketId, weapon_script::NpcWeaponScript},
    },
};

/// This module contains all the discord related slash commands.
pub mod commands;

/// The PreagonalBotEventHandler struct is the event handler for the bot.
pub struct PreagonalBotEventHandler {
    rc_clients: Vec<Arc<RemoteControlClient>>,
    game_clients: Vec<Arc<GameClient>>,
    guild_id: GuildId,
    channel_id: ChannelId,
}

#[async_trait]
impl EventHandler for PreagonalBotEventHandler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            // Defer the response
            let err = command
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Defer(
                        CreateInteractionResponseMessage::new()
                            .content("Waiting for command completion..."),
                    ),
                )
                .await;

            if let Err(why) = err {
                log::error!("Error creating interaction response: {:?}", why);
                return;
            }

            let response = match command.data.name.as_str() {
                "send_to_rc" => sendtorc::run(&command, self.rc_clients.clone()).await,
                "get_weapon" => get_weapon::run(&command, self.rc_clients.clone()).await,
                "add_weapon" => add_weapon::run(&command, self.rc_clients.clone()).await,
                _ => {
                    log::warn!("No command found: {}", command.data.name);
                    Ok(CreateInteractionResponseFollowup::new()
                        .content(format!("Command `{}` not found.", command.data.name)))
                }
            };

            if let Err(why) = response {
                log::error!("Error running command: {:?}", why);
                command
                    .create_followup(
                        &ctx.http,
                        CreateInteractionResponseFollowup::new()
                            .content(format!("Error running command:\n```{:?}```", why)),
                    )
                    .await
                    .expect("Error sending error response");
                return;
            }

            if let Err(why) = command.create_followup(&ctx.http, response.unwrap()).await {
                log::error!("Error creating interaction response: {:?}", why);
            }
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
                .await
                .expect("Error registering event handler");
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
                .await
                .expect("Error registering event handler");
        }
    }
}

impl PreagonalBotEventHandler {
    /// Create a new PreagonalBotEventHandler.
    pub fn new(
        rc_clients: Vec<Arc<RemoteControlClient>>,
        game_clients: Vec<Arc<GameClient>>,
        guild_id: GuildId,
        channel_id: ChannelId,
    ) -> Self {
        Self {
            rc_clients,
            game_clients,
            guild_id,
            channel_id,
        }
    }
}
