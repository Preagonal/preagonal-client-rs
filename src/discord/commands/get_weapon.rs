use std::sync::Arc;

use serenity::all::{
    CommandInteraction, CommandOptionType, CreateAttachment, CreateCommandOption,
    CreateInteractionResponseFollowup, ResolvedValue,
};
use serenity::builder::CreateCommand;

use crate::net::client::ClientError;
use crate::net::client::nc::NpcControlClientTrait;
use crate::net::client::rc::RemoteControlClient;

/// Sends a message to RC.
pub async fn run(
    interaction: &CommandInteraction,
    rc: Vec<Arc<RemoteControlClient>>,
) -> Result<CreateInteractionResponseFollowup, ClientError> {
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
        let mut response_data = CreateInteractionResponseFollowup::new();
        for client in rc.iter() {
            let response = client.nc_get_weapon(msg.clone()).await?;

            response_data = response_data.add_file(CreateAttachment::bytes(
                response.script,
                format!("{}.gs2", &response.script_name),
            ))
        }

        return Ok(response_data);
    }
    Ok(CreateInteractionResponseFollowup::new().content("No `weapon` parameter found."))
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
