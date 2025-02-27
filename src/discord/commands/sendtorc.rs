use serenity::all::{CommandInteraction, CommandOptionType, CreateCommandOption, ResolvedValue};
use serenity::builder::CreateCommand;

use crate::net::packet::GPacket;
use crate::net::packet::from_client::rc_chat::RcChat;

/// Sends a message to RC.
pub fn run(interaction: &CommandInteraction) -> Option<Box<dyn GPacket + Send>> {
    // Get the user who ran the interaction
    let nick = interaction.user.name.clone();

    // Get the right option
    let options = interaction.data.options();
    let message = options.iter().find(|o| o.name == "message").and_then(|o| {
        if let ResolvedValue::String(msg) = o.value {
            Some(msg.to_string())
        } else {
            None
        }
    });

    message
        .map(|msg| Box::new(RcChat::new(format!("{}: {}", nick, msg))) as Box<dyn GPacket + Send>)
}

/// Registers the command.
pub fn register() -> CreateCommand {
    CreateCommand::new("sendtorc")
        .description("Sends a message to RC")
        .add_option(
            CreateCommandOption::new(CommandOptionType::String, "message", "The message to send")
                .required(true),
        )
}
