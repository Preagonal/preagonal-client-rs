use std::sync::Arc;

use serenity::all::{
    CommandInteraction, CommandOptionType, CreateCommandOption, CreateInteractionResponse,
    CreateInteractionResponseMessage, ResolvedValue,
};
use serenity::builder::CreateCommand;

use crate::net::client::ClientError;
use crate::net::client::nc::NpcControlClientTrait;
use crate::net::client::rc::RemoteControlClient;

/// Adds a weapon.
pub async fn run(
    interaction: &CommandInteraction,
    rc: Vec<Arc<RemoteControlClient>>,
) -> Result<CreateInteractionResponse, ClientError> {
    // Get the right option
    let options = interaction.data.options();
    let weapon = if let Some(option) = options.iter().find(|o| o.name == "weapon") {
        if let ResolvedValue::String(msg) = option.value {
            msg.to_string()
        } else {
            return Ok(CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content("Weapon parameter is not a valid string."),
            ));
        }
    } else {
        return Ok(CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new().content("No `weapon` parameter found."),
        ));
    };

    let script = if let Some(option) = options.iter().find(|o| o.name == "script") {
        if let ResolvedValue::Attachment(attachment) = option.value {
            &attachment.url
        } else {
            return Ok(CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content("Script parameter is not a valid attachment."),
            ));
        }
    } else {
        return Ok(CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new().content("No `script` parameter found."),
        ));
    };

    // Download the script using reqwest
    let script = match reqwest::get(script).await {
        Ok(response) => match response.bytes().await {
            Ok(bytes) => match String::from_utf8(bytes.to_vec()) {
                Ok(string) => string,
                Err(_) => {
                    return Ok(CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content("Failed to convert script to a UTF-8 string."),
                    ));
                }
            },
            Err(_) => {
                return Ok(CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content("Failed to download script bytes."),
                ));
            }
        },
        Err(_) => {
            return Ok(CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new().content("Failed to download script."),
            ));
        }
    };

    for client in rc.iter() {
        client
            .nc_add_weapon(
                weapon.clone(),
                "bcalarmclock.png".to_string(),
                script.clone(),
            )
            .await?;
    }

    Ok(CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new().content(format!("Weapon {} added. In order to get the weapon bytecode, ensure that:\n* The client has the weapon added\n* `//#CLIENTSIDE` is added to the beginning of the script.\n* The weapon has been changed, to trigger the client to get the new weapon.", weapon)),
    ))
}

/// Registers the command.
pub fn register() -> CreateCommand {
    CreateCommand::new("add_weapon")
        .description("Add a weapon")
        .add_option(
            CreateCommandOption::new(CommandOptionType::String, "weapon", "The weapon name")
                .required(true),
        )
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::Attachment,
                "script",
                "The script to compile",
            )
            .required(true),
        )
}
