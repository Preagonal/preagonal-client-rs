use std::sync::Arc;

use serenity::all::{
    CommandInteraction, CommandOptionType, CreateCommandOption, CreateEmbed,
    CreateInteractionResponse, CreateInteractionResponseMessage, ResolvedValue,
};
use serenity::builder::CreateCommand;

use crate::net::client::ClientError;
use crate::net::client::rc::RemoteControlClient;
use crate::net::packet::from_client::nc_weapon_get::NcWeaponGet;
use crate::net::packet::from_server::FromServerPacketId;

/// Sends a message to RC.
pub async fn run(
    interaction: &CommandInteraction,
    rc: Arc<RemoteControlClient>,
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
        let weapon_packet = NcWeaponGet::new(msg);
        let response = rc
            .get_npc_control()
            .await
            .expect("Failed to get npc control")
            .client
            .send_and_receive(Arc::new(weapon_packet), FromServerPacketId::NcWeaponGet)
            .await?;
        // get response.data serialize as utf8
        let response_data = response.data();
        let response_data = String::from_utf8_lossy(response_data.as_slice());
        log::info!("Weapon response: {}", response_data);
        return Ok(CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new().add_embed(
                CreateEmbed::new()
                    .title("Weapon")
                    .description(response_data),
            ),
        ));
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
