//! The serenity event handler: ready, slash commands, button presses, voice state updates.

use super::panel::{self, PanelRef};
use super::session::{self, JoinOutcome};
use super::{ConnectionState, Shared};
use serenity::all::{
    Command, CommandInteraction, CommandOptionType, ComponentInteraction, Context, CreateCommand,
    CreateCommandOption, CreateInteractionResponse, CreateInteractionResponseMessage,
    EditInteractionResponse, EventHandler, Interaction, InteractionContext, Ready, VoiceState,
};
use serenity::async_trait;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub(crate) struct Handler {
    pub(crate) shared: Arc<Shared>,
}

/// The slash commands. `/link` (change `owner-only-access`) also works in a direct message
/// to the bot; Discord takes interaction contexts only on global commands, so the guild's
/// copy (`global: false`) has none.
fn command_definitions(global: bool) -> Vec<CreateCommand> {
    let link = CreateCommand::new("link")
        .description("Link your Discord account to VoiceBro with the code its window shows")
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::String,
                "code",
                "The code shown in the VoiceBro window",
            )
            .required(true)
            .min_length(8)
            .max_length(12),
        );
    let link = if global {
        link.contexts(vec![InteractionContext::Guild, InteractionContext::BotDm])
    } else {
        link
    };
    vec![
        CreateCommand::new("join").description("Join your current voice channel"),
        CreateCommand::new("leave").description("Leave the voice channel"),
        CreateCommand::new("status").description("Show what VoiceBro is doing"),
        link,
    ]
}

/// The `/status` reply. While the owner-only mode is on (change `owner-only-access`) it
/// says so; the command itself stays open to everyone.
fn status_text(
    connected: bool,
    channel: Option<String>,
    assistant: &str,
    muted: bool,
    owner_only: bool,
) -> String {
    let mut text = format!(
        "Discord: {}. Voice channel: {}. Assistant: {assistant}. {}.",
        if connected {
            "connected"
        } else {
            "not connected"
        },
        channel.unwrap_or_else(|| "none".to_owned()),
        if muted { "muted" } else { "not muted" },
    );
    if owner_only {
        text.push_str("\nOnly the owner's voice reaches the assistant here.");
    }
    text
}

impl Handler {
    async fn handle_join(&self, ctx: &Context, command: &CommandInteraction) {
        let Some(guild) = command.guild_id else {
            let _ = command
                .create_response(&ctx.http, ephemeral("Use /join inside a server."))
                .await;
            return;
        };
        // Resolve the caller's voice channel from the cache without holding the guard
        // across an await.
        let channel = ctx.cache.guild(guild).and_then(|g| {
            g.voice_states
                .get(&command.user.id)
                .and_then(|s| s.channel_id)
        });
        let Some(channel) = channel else {
            let _ = command
                .create_response(
                    &ctx.http,
                    ephemeral("Join a voice channel first, then run /join."),
                )
                .await;
            return;
        };
        if command.defer(&ctx.http).await.is_err() {
            return;
        }
        // The session keeps the channel /join was typed in before the panel is posted, so
        // answers can go there even when the panel cannot.
        let outcome =
            session::prepare_and_join(&self.shared, guild, channel, command.channel_id).await;
        let text = outcome.reply_text();
        let response = match outcome {
            JoinOutcome::Joined { .. } => match session::panel_message(&self.shared, guild) {
                Some((embed, components)) => EditInteractionResponse::new()
                    .content(text)
                    .embed(embed)
                    .components(components),
                None => EditInteractionResponse::new().content(text),
            },
            JoinOutcome::Failed(error) => {
                self.shared.log(format!("join failed: {error}"));
                EditInteractionResponse::new().content(text)
            }
            JoinOutcome::NotReady | JoinOutcome::Refused(_) => {
                EditInteractionResponse::new().content(text)
            }
        };
        match command.edit_response(&ctx.http, response).await {
            Ok(message) => {
                if session::session_channel_name(&self.shared, guild).is_some() {
                    session::remember_panel(
                        &self.shared,
                        guild,
                        PanelRef {
                            channel: command.channel_id,
                            message: message.id,
                        },
                    );
                }
            }
            Err(error) => self.shared.log(format!("join reply failed: {error}")),
        }
    }

    async fn handle_leave(&self, ctx: &Context, command: &CommandInteraction) {
        let Some(guild) = command.guild_id else {
            let _ = command
                .create_response(&ctx.http, ephemeral("Use /leave inside a server."))
                .await;
            return;
        };
        if command.defer_ephemeral(&ctx.http).await.is_err() {
            return;
        }
        let left = session::leave_voice(&self.shared, guild).await;
        let text = if left {
            "Left the voice channel."
        } else {
            "I was not in a voice channel here."
        };
        let _ = command
            .edit_response(&ctx.http, EditInteractionResponse::new().content(text))
            .await;
    }

    async fn handle_status(&self, ctx: &Context, command: &CommandInteraction) {
        let connected = session::is_connected(&self.shared);
        let channel = command
            .guild_id
            .and_then(|guild| session::session_channel_name(&self.shared, guild));
        let assistant = command
            .guild_id
            .map(|guild| self.shared.host.assistant_state(guild.get()).label())
            .unwrap_or("off");
        let text = status_text(
            connected,
            channel,
            assistant,
            self.shared.host.is_muted(),
            self.shared.host.owner_filter() != crate::owner::FILTER_EVERYONE,
        );
        let _ = command.create_response(&ctx.http, ephemeral(&text)).await;
    }

    /// `/link <code>` (change `owner-only-access`): the sender's account is the one Discord
    /// put in the interaction, so nobody can link someone else. The reply is the sender's
    /// only; the typed code is never logged.
    async fn handle_link(&self, ctx: &Context, command: &CommandInteraction) {
        let code = command
            .data
            .options
            .iter()
            .find(|option| option.name == "code")
            .and_then(|option| option.value.as_str())
            .unwrap_or("");
        let user = &command.user;
        let name = user
            .global_name
            .as_deref()
            .and_then(super::speakers::clean_display_name)
            .or_else(|| super::speakers::clean_display_name(&user.name))
            .unwrap_or_else(|| user.id.get().to_string());
        let outcome = self.shared.host.complete_link(user.id.get(), &name, code);
        let _ = command
            .create_response(&ctx.http, ephemeral(&outcome.reply()))
            .await;
    }

    async fn handle_component(&self, ctx: &Context, component: &ComponentInteraction) {
        let Some(button_guild) = panel::parse_mute_button_id(&component.data.custom_id) else {
            return;
        };
        let Some(guild) = component.guild_id else {
            return;
        };
        if guild.get() != button_guild {
            return;
        }
        if component.defer(&ctx.http).await.is_err() {
            return;
        }
        let muted = self.shared.host.toggle_mute();
        self.shared.log(format!(
            "mute {} from the Discord panel",
            if muted { "on" } else { "off" }
        ));
        if let Some((embed, components)) = session::panel_message(&self.shared, guild) {
            let _ = component
                .edit_response(
                    &ctx.http,
                    EditInteractionResponse::new()
                        .embed(embed)
                        .components(components),
                )
                .await;
        }
        self.shared.panels_dirty.store(true, Ordering::Release);
    }
}

fn ephemeral(text: &str) -> CreateInteractionResponse {
    CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .content(text)
            .ephemeral(true),
    )
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        *self.shared.http.lock().expect("http lock") = Some(ctx.http.clone());
        *self.shared.cache.lock().expect("cache lock") = Some(ctx.cache.clone());
        *self.shared.songbird.lock().expect("songbird lock") = songbird::get(&ctx).await;
        self.shared.update(|status| {
            status.state = ConnectionState::Connected;
            status.user_name = Some(ready.user.name.clone());
            status.guild_count = ready.guilds.len();
            status.last_error = None;
        });
        self.shared
            .log(format!("Discord: connected as {}", ready.user.name));

        // Globally, so every server has them, and in the configured server too, where they
        // appear at once.
        match Command::set_global_commands(&ctx.http, command_definitions(true)).await {
            Ok(list) => self.shared.log(format!(
                "registered {} global slash commands (Discord may take up to an hour to show new ones)",
                list.len()
            )),
            Err(error) => self
                .shared
                .log(format!("could not register global slash commands: {error}")),
        }
        let guild_id = *self.shared.guild_id.lock().expect("guild id lock");
        if let Some(guild) = guild_id {
            match guild
                .set_commands(&ctx.http, command_definitions(false))
                .await
            {
                Ok(list) => self.shared.log(format!(
                    "registered {} slash commands in guild {}",
                    list.len(),
                    guild.get()
                )),
                Err(error) => self
                    .shared
                    .log(format!("could not register guild slash commands: {error}")),
            }
        }
        tokio::spawn(panel::refresher(Arc::clone(&self.shared)));
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Command(command) => match command.data.name.as_str() {
                "join" => self.handle_join(&ctx, &command).await,
                "leave" => self.handle_leave(&ctx, &command).await,
                "status" => self.handle_status(&ctx, &command).await,
                "link" => self.handle_link(&ctx, &command).await,
                _ => {}
            },
            Interaction::Component(component) => self.handle_component(&ctx, &component).await,
            _ => {}
        }
    }

    async fn voice_state_update(&self, _ctx: Context, old: Option<VoiceState>, new: VoiceState) {
        let guild = new
            .guild_id
            .or_else(|| old.as_ref().and_then(|o| o.guild_id));
        if let Some(guild) = guild {
            session::refresh_participants(&self.shared, guild).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Change `owner-only-access`, task 5.1: four commands; `/link` takes a code of 8 to
    /// 12 characters and, globally, works in servers and the bot's DMs; the other three
    /// are registered as before.
    #[test]
    fn link_is_registered_next_to_the_three_commands() {
        let global = serde_json::to_value(command_definitions(true)).unwrap();
        let names: Vec<&str> = global
            .as_array()
            .unwrap()
            .iter()
            .map(|command| command["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["join", "leave", "status", "link"]);
        let link = &global[3];
        assert_eq!(link["contexts"], serde_json::json!([0, 1]));
        let option = &link["options"][0];
        assert_eq!(option["name"], "code");
        assert_eq!(option["type"], 3, "a string");
        assert_eq!(option["required"], true);
        assert_eq!(
            (option["min_length"].clone(), option["max_length"].clone()),
            (8.into(), 12.into())
        );
        for command in &global.as_array().unwrap()[..3] {
            assert!(command.get("contexts").is_none(), "{command}");
            assert!(
                command
                    .get("options")
                    .is_none_or(|o| o.as_array().is_some_and(Vec::is_empty)),
                "{command}"
            );
        }
        let guild = serde_json::to_value(command_definitions(false)).unwrap();
        assert!(
            guild[3].get("contexts").is_none(),
            "no contexts on a guild command"
        );
        assert_eq!(guild[3]["options"], link["options"]);
    }

    /// Scenario "Status from another member": the line is there only while the mode is on.
    #[test]
    fn status_names_the_owner_only_mode() {
        let off = status_text(true, Some("General".to_owned()), "listening", false, false);
        assert_eq!(
            off,
            "Discord: connected. Voice channel: General. Assistant: listening. not muted."
        );
        let on = status_text(true, None, "waiting for the wake word", true, true);
        assert!(
            on.starts_with("Discord: connected. Voice channel: none."),
            "{on}"
        );
        assert!(
            on.ends_with("Only the owner's voice reaches the assistant here."),
            "{on}"
        );
    }
}
