//! The serenity event handler: ready, slash commands, button presses, voice state updates.

use super::panel::{self, PanelRef};
use super::session::{self, JoinOutcome};
use super::{ConnectionState, Shared};
use serenity::all::{
    Command, CommandInteraction, ComponentInteraction, Context, CreateCommand,
    CreateInteractionResponse, CreateInteractionResponseMessage, EditInteractionResponse,
    EventHandler, Interaction, Ready, VoiceState,
};
use serenity::async_trait;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub(crate) struct Handler {
    pub(crate) shared: Arc<Shared>,
}

fn command_definitions() -> Vec<CreateCommand> {
    vec![
        CreateCommand::new("join").description("Join your current voice channel"),
        CreateCommand::new("leave").description("Leave the voice channel"),
        CreateCommand::new("status").description("Show what VoiceBro is doing"),
    ]
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
        let outcome = session::join_voice(&self.shared, guild, channel).await;
        let response = match outcome {
            JoinOutcome::Joined { channel_name } => {
                match session::panel_message(&self.shared, guild) {
                    Some((embed, components)) => EditInteractionResponse::new()
                        .content(format!("Joined {channel_name}."))
                        .embed(embed)
                        .components(components),
                    None => {
                        EditInteractionResponse::new().content(format!("Joined {channel_name}."))
                    }
                }
            }
            JoinOutcome::NotReady => EditInteractionResponse::new()
                .content("Voice is not ready yet; try again in a moment."),
            JoinOutcome::Failed(error) => {
                self.shared.log(format!("join failed: {error}"));
                EditInteractionResponse::new().content(format!("Could not join: {error}"))
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
        let muted = if self.shared.host.is_muted() {
            "muted"
        } else {
            "not muted"
        };
        let text = format!(
            "Discord: {}. Voice channel: {}. Assistant: {assistant}. {muted}.",
            if connected {
                "connected"
            } else {
                "not connected"
            },
            channel.unwrap_or_else(|| "none".to_owned()),
        );
        let _ = command.create_response(&ctx.http, ephemeral(&text)).await;
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

        let guild_id = *self.shared.guild_id.lock().expect("guild id lock");
        let commands = command_definitions();
        match guild_id {
            Some(guild) => match guild.set_commands(&ctx.http, commands).await {
                Ok(list) => self.shared.log(format!(
                    "registered {} slash commands in guild {}",
                    list.len(),
                    guild.get()
                )),
                Err(error) => self
                    .shared
                    .log(format!("could not register guild slash commands: {error}")),
            },
            None => match Command::set_global_commands(&ctx.http, commands).await {
                Ok(list) => self.shared.log(format!(
                    "registered {} global slash commands (Discord may take up to an hour to show them)",
                    list.len()
                )),
                Err(error) => self
                    .shared
                    .log(format!("could not register global slash commands: {error}")),
            },
        }
        tokio::spawn(panel::refresher(Arc::clone(&self.shared)));
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Command(command) => match command.data.name.as_str() {
                "join" => self.handle_join(&ctx, &command).await,
                "leave" => self.handle_leave(&ctx, &command).await,
                "status" => self.handle_status(&ctx, &command).await,
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
