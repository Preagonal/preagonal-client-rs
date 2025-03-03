use std::sync::Arc;

use serenity::all::{
    CommandInteraction, CommandOptionType, CreateCommandOption, CreateEmbed,
    CreateInteractionResponse, CreateInteractionResponseMessage, ResolvedValue,
};
use serenity::builder::CreateCommand;

use crate::net::client::ClientError;
use crate::net::client::nc::NpcControlClientTrait;
use crate::net::client::rc::RemoteControlClient;

/// Sends a message to RC.
pub async fn run(
    interaction: &CommandInteraction,
    rc: Vec<Arc<RemoteControlClient>>,
) -> Result<CreateInteractionResponse, ClientError> {
    // Get the right option
    let options = interaction.data.options();
    let weapon = options.iter().find(|o| o.name == "weapon").and_then(|o| {
        if let ResolvedValue::String(msg) = o.value {
            Some(msg.to_string())
        } else {
            None
        }
    });

    if let Some(msg) = weapon {
        let mut response_data: Vec<String> = Vec::new();
        for client in rc.iter() {
            let response = client.nc_get_weapon(msg.clone()).await?;
            // get response.data serialize as utf8
            let res = response.data();
            let res = String::from_utf8_lossy(res.as_slice());
            response_data.push(res.to_string());
        }
        log::info!("Weapon response: {:?}", response_data);
        let mut message = CreateInteractionResponseMessage::new();
        for data in &response_data {
            message = message.add_embed(CreateEmbed::new().title(&msg).description(data));
        }
        return Ok(CreateInteractionResponse::Message(message));
    }
    Ok(CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new().content("No `weapon` parameter found."),
    ))
}

/// Registers the command.
pub fn register() -> CreateCommand {
    CreateCommand::new("get_weapon")
        .description("Gets a weapon")
        .add_option(
            CreateCommandOption::new(CommandOptionType::String, "weapon", "The weapon name")
                .required(true),
        )
}
