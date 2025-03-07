use std::sync::Arc;

use serenity::all::{
    CommandInteraction, CommandOptionType, CreateCommandOption, CreateInteractionResponseFollowup,
    ResolvedValue,
};
use serenity::builder::CreateCommand;

use crate::net::client::rc::RemoteControlClient;
use crate::net::client::{ClientError, GClientTrait};
use crate::net::packet::from_client::rc_chat::RcChat;

/// Sends a message to RC.
pub async fn run(
    interaction: &CommandInteraction,
    rc: Vec<Arc<RemoteControlClient>>,
) -> Result<CreateInteractionResponseFollowup, ClientError> {
    // Get the user who ran the interaction
    let nick = &interaction.user.name;

    // Get the right option
    let options = interaction.data.options();
    let message = options.iter().find(|o| o.name == "message").and_then(|o| {
        if let ResolvedValue::String(msg) = o.value {
            Some(msg.to_string())
        } else {
            None
        }
    });

    if let Some(msg) = message {
        for client in rc.iter() {
            let chat_packet = RcChat::new(format!("{}: {}", nick, msg));
            client.send_packet(Arc::new(chat_packet)).await?;
        }
        return Ok(
            CreateInteractionResponseFollowup::new().content("Message sent to RemoteControl.")
        );
    }
    Ok(CreateInteractionResponseFollowup::new().content("No `message` parameter found."))
}

/// Registers the command.
pub fn register() -> CreateCommand {
    CreateCommand::new("send_to_rc")
        .description("Sends a message to RC")
        .add_option(
            CreateCommandOption::new(CommandOptionType::String, "message", "The message to send")
                .required(true),
        )
}
