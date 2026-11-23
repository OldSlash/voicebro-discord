//! The serenity event handler: ready, slash commands, button presses, voice state updates.

use super::panel::{self, PanelAction, PanelButton, PanelRef};
use super::session::{self, JoinOutcome};
use super::{ConnectionState, ModeChange, Shared};
use crate::settings::{AccessMode, AssistantMode};
use serenity::all::{
    ButtonStyle, Command, CommandInteraction, CommandOptionType, ComponentInteraction, Context,
    CreateActionRow, CreateButton, CreateCommand, CreateCommandOption, CreateInteractionResponse,
    CreateInteractionResponseFollowup, CreateInteractionResponseMessage, EditInteractionResponse,
    EventHandler, GuildId, Interaction, InteractionContext, Message, MessageId, MessageType, Ready,
    VoiceState,
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
    // Change `voice-consent`: the three consent commands work in servers and in a direct
    // message to the bot, like `/link`.
    // The description names no company: the engine, and with it the recipient, can change
    // (change `claude-and-opencode-engines`); `/optin`'s text names it.
    let optin = CreateCommand::new("optin").description(
        "Let this bot's AI assistant hear you: after the wake phrase your speech goes to it",
    );
    let optout = CreateCommand::new("optout")
        .description("Stop VoiceBro's assistant hearing you and delete your consent");
    let privacy = CreateCommand::new("privacy").description("Show this bot's privacy policy");
    let anywhere = |command: CreateCommand| {
        if global {
            command.contexts(vec![InteractionContext::Guild, InteractionContext::BotDm])
        } else {
            command
        }
    };
    vec![
        CreateCommand::new("join").description("Join your current voice channel"),
        CreateCommand::new("leave").description("Leave the voice channel"),
        CreateCommand::new("status").description("Show what VoiceBro is doing"),
        anywhere(link),
        anywhere(optin),
        anywhere(optout),
        anywhere(privacy),
    ]
}

/// The custom id prefix of the "I agree" button; the consent text's version follows.
const AGREE_PREFIX: &str = "vb:optin:agree:";
/// The custom id of the "Cancel" button under the consent text.
const CANCEL_ID: &str = "vb:optin:cancel";

/// A press of one of the two buttons under the consent text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConsentButton {
    Agree(u32),
    Cancel,
}

fn parse_consent_button(custom_id: &str) -> Option<ConsentButton> {
    if custom_id == CANCEL_ID {
        return Some(ConsentButton::Cancel);
    }
    custom_id
        .strip_prefix(AGREE_PREFIX)
        .and_then(|version| version.parse().ok())
        .map(ConsentButton::Agree)
}

const NO_POLICY: &str = "The operator of this bot has not published a privacy policy yet, so it cannot take your consent. Ask them to add one.";

/// What `/optin` replies (design D5).
#[derive(Debug, Clone, PartialEq, Eq)]
enum OptinReply {
    /// No buttons: no policy, or a record for the current text already.
    Note(String),
    /// The consent text with "I agree" (for this version) and "Cancel".
    Offer { text: String, version: u32 },
}

fn optin_reply(
    url: &str,
    existing: Option<&crate::consent::ConsentRecord>,
    provider: &str,
) -> OptinReply {
    if url.is_empty() {
        return OptinReply::Note(NO_POLICY.to_owned());
    }
    let version = crate::consent::CONSENT_TEXT_VERSION;
    if let Some(record) = existing.filter(|record| record.text_version == version) {
        return OptinReply::Note(format!(
            "You already agreed on {} (UTC). Run /optout to withdraw.",
            utc_minutes(&record.agreed_at)
        ));
    }
    OptinReply::Offer {
        text: crate::consent::consent_text(version, provider, url).unwrap_or_default(),
        version,
    }
}

/// `2026-11-18T10:00:00.123+00:00` as `2026-11-18 10:00`.
fn utc_minutes(rfc3339: &str) -> String {
    let text = rfc3339.replace('T', " ");
    text.get(..16).map(str::to_owned).unwrap_or(text)
}

/// The reply to "I agree" or "Cancel" in place of the consent text.
fn button_reply(outcome: Option<crate::consent::AgreeOutcome>, access: AccessMode) -> String {
    use crate::consent::AgreeOutcome;
    match outcome {
        None => "Nothing changed.".to_owned(),
        Some(AgreeOutcome::OldText) => {
            "The consent text has changed since this message. Run /optin again to see the new one."
                .to_owned()
        }
        Some(AgreeOutcome::AlreadyAgreed { agreed_at }) => format!(
            "You already agreed on {} (UTC). Run /optout to withdraw.",
            utc_minutes(&agreed_at)
        ),
        Some(AgreeOutcome::Recorded) => {
            let base = "Done. Your voice reaches the assistant after the activation phrase";
            match access {
                AccessMode::Agreed => format!("{base}. Run /optout to stop."),
                AccessMode::Everyone => format!(
                    "{base} once the operator limits the bot to people who agreed; right now this bot hears everyone in the channel. Run /optout to withdraw."
                ),
                AccessMode::Owner => format!(
                    "{base} once the operator limits the bot to people who agreed; right now it hears only its owner. Run /optout to withdraw."
                ),
            }
        }
    }
}

fn optout_reply(outcome: &crate::consent::WithdrawOutcome) -> String {
    match outcome {
        crate::consent::WithdrawOutcome::Removed { .. } => "Done. Your consent is deleted and your voice no longer reaches the assistant while this bot hears only people who agreed. What you said before was already sent and cannot be called back.".to_owned(),
        crate::consent::WithdrawOutcome::NothingToRemove => {
            "You had not agreed, so there was nothing to remove.".to_owned()
        }
    }
}

fn privacy_reply(url: &str) -> String {
    if url.is_empty() {
        "The operator of this bot has not published a privacy policy.".to_owned()
    } else {
        format!("This bot's privacy policy: {url}")
    }
}

/// A user's name as `/link` and the consent record take it.
fn display_name(user: &serenity::all::User) -> String {
    user.global_name
        .as_deref()
        .and_then(super::speakers::clean_display_name)
        .or_else(|| super::speakers::clean_display_name(&user.name))
        .unwrap_or_else(|| user.id.get().to_string())
}

/// The line `/join` and `/status` add for a limited access mode (changes `owner-only-access`
/// and `voice-consent`); none in the `everyone` mode.
fn access_line(access: AccessMode) -> Option<&'static str> {
    match access {
        AccessMode::Everyone => None,
        AccessMode::Agreed => {
            Some("Only the owner and people who ran /optin are heard here. See /privacy.")
        }
        AccessMode::Owner => Some("Only the owner's voice reaches the assistant here."),
    }
}

/// Who pressed one of the panel's buttons, against the linked owner (change
/// `discord-panel-controls`, design D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Presser {
    Owner,
    NotOwner,
    NoOwnerLinked,
}

fn presser(owner: Option<u64>, user: u64) -> Presser {
    match owner {
        None => Presser::NoOwnerLinked,
        Some(owner) if owner == user => Presser::Owner,
        Some(_) => Presser::NotOwner,
    }
}

const NOT_THE_OWNER: &str = "These buttons are for this bot's owner only.";
const NO_OWNER_LINKED: &str = "No owner is linked to this bot yet, so its buttons work for nobody. The operator links their Discord account in VoiceBro → Config → Access.";
/// The same for the headless app (change `cross-platform-headless`), which has no window.
const NO_OWNER_LINKED_TERMINAL: &str = "No owner is linked to this bot yet, so its buttons work for nobody. The operator links their Discord account with voicebro-server --link-owner.";

/// The reply only the presser sees when the press is refused; `None` for the owner.
fn press_refusal(presser: Presser, terminal: bool) -> Option<&'static str> {
    match presser {
        Presser::Owner => None,
        Presser::NotOwner => Some(NOT_THE_OWNER),
        Presser::NoOwnerLinked if terminal => Some(NO_OWNER_LINKED_TERMINAL),
        Presser::NoOwnerLinked => Some(NO_OWNER_LINKED),
    }
}

/// Whether a press came from the panel of the bot's current voice session (design D3).
fn is_current_panel(current: Option<MessageId>, pressed: MessageId) -> bool {
    current == Some(pressed)
}

/// An old panel after the owner's press: its embed stays (an update leaves out what it does
/// not name), its buttons go, and it says that `/join` posts a new one.
fn old_panel_update() -> CreateInteractionResponse {
    CreateInteractionResponse::UpdateMessage(
        CreateInteractionResponseMessage::new()
            .content(panel::OLD_PANEL_NOTE)
            .components(Vec::new()),
    )
}

/// The owner's reply to Leave channel (design D4): the channel left, or that there was none.
fn leave_reply(left: Option<&str>) -> String {
    match left {
        Some(channel) => format!("Left {channel}."),
        None => "I was not in a voice channel here.".to_owned(),
    }
}

const ENGINE_SWITCHED: &str =
    "Switched to the realtime voice. The engine is now Codex: the voice call works only with Codex.";

/// What only the owner sees after a mode button (design D5): the engine switch, or why
/// nothing changed; `None` for a plain switch, which the panel itself shows.
fn mode_follow_up(result: &Result<ModeChange, String>) -> Option<String> {
    match result {
        Ok(ModeChange {
            engine_switched: true,
        }) => Some(ENGINE_SWITCHED.to_owned()),
        Ok(_) => None,
        Err(reason) => Some(reason.clone()),
    }
}

/// The `/status` reply. A limited access mode adds its line; the command itself stays open
/// to everyone.
fn status_text(
    connected: bool,
    channel: Option<String>,
    assistant: &str,
    mode: &str,
    muted: bool,
    access: AccessMode,
) -> String {
    let mut text = format!(
        "Discord: {}. Voice channel: {}. Assistant: {assistant}. Mode: {mode}. {}.",
        if connected {
            "connected"
        } else {
            "not connected"
        },
        channel.unwrap_or_else(|| "none".to_owned()),
        if muted { "muted" } else { "not muted" },
    );
    if let Some(line) = access_line(access) {
        text.push('\n');
        text.push_str(line);
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
        let mut text = outcome.reply_text();
        // A limited access mode is named under a successful join (change `voice-consent`).
        if matches!(outcome, JoinOutcome::Joined { .. }) {
            if let Some(line) = access_line(self.shared.host.access_mode()) {
                text.push('\n');
                text.push_str(line);
            }
        }
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
        let state = command
            .guild_id
            .map(|guild| self.shared.host.assistant_state(guild.get()).label())
            .unwrap_or("off");
        // Change `claude-and-opencode-engines`: the reply names the engine in use.
        let assistant = format!("{}, {state}", self.shared.host.engine_name());
        let text = status_text(
            connected,
            channel,
            &assistant,
            self.shared.host.assistant_mode(),
            self.shared.host.is_muted(),
            self.shared.host.access_mode(),
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
        let name = display_name(user);
        let outcome = self.shared.host.complete_link(user.id.get(), &name, code);
        let reply = if self.shared.events.terminal_guidance() {
            outcome.terminal_reply()
        } else {
            outcome.reply()
        };
        let _ = command.create_response(&ctx.http, ephemeral(&reply)).await;
    }

    /// `/optin` (change `voice-consent`): the consent text with two buttons, only for the
    /// sender; nothing is recorded until "I agree".
    async fn handle_optin(&self, ctx: &Context, command: &CommandInteraction) {
        let host = &self.shared.host;
        let reply = optin_reply(
            &host.privacy_url(),
            host.consent_record(command.user.id.get()).as_ref(),
            &host.consent_provider(),
        );
        let response = match reply {
            OptinReply::Note(text) => ephemeral(&text),
            OptinReply::Offer { text, version } => CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(text)
                    .ephemeral(true)
                    .components(vec![CreateActionRow::Buttons(vec![
                        CreateButton::new(format!("{AGREE_PREFIX}{version}"))
                            .label("I agree")
                            .style(ButtonStyle::Success),
                        CreateButton::new(CANCEL_ID)
                            .label("Cancel")
                            .style(ButtonStyle::Secondary),
                    ])]),
            ),
        };
        let _ = command.create_response(&ctx.http, response).await;
    }

    /// `/optout`: the sender's record is deleted; applies from the next frame.
    async fn handle_optout(&self, ctx: &Context, command: &CommandInteraction) {
        let outcome = self.shared.host.withdraw(command.user.id.get());
        let _ = command
            .create_response(&ctx.http, ephemeral(&optout_reply(&outcome)))
            .await;
    }

    async fn handle_privacy(&self, ctx: &Context, command: &CommandInteraction) {
        let text = privacy_reply(&self.shared.host.privacy_url());
        let _ = command.create_response(&ctx.http, ephemeral(&text)).await;
    }

    /// "I agree" or "Cancel" under the consent text. The message is ephemeral, so only the
    /// person who ran `/optin` sees and presses it: the presser is the one who agrees.
    async fn handle_consent_button(
        &self,
        ctx: &Context,
        component: &ComponentInteraction,
        button: ConsentButton,
    ) {
        let host = &self.shared.host;
        let outcome = match button {
            ConsentButton::Cancel => None,
            ConsentButton::Agree(version) => Some(host.agree(
                component.user.id.get(),
                &display_name(&component.user),
                version,
            )),
        };
        let text = button_reply(outcome, host.access_mode());
        let _ = component
            .create_response(
                &ctx.http,
                CreateInteractionResponse::UpdateMessage(
                    CreateInteractionResponseMessage::new()
                        .content(text)
                        .components(Vec::new()),
                ),
            )
            .await;
    }

    async fn handle_component(&self, ctx: &Context, component: &ComponentInteraction) {
        if let Some(button) = parse_consent_button(&component.data.custom_id) {
            self.handle_consent_button(ctx, component, button).await;
            return;
        }
        let Some(button) = PanelButton::parse(&component.data.custom_id) else {
            return;
        };
        let Some(guild) = component.guild_id else {
            return;
        };
        if guild.get() != button.guild {
            return;
        }
        self.handle_panel_button(ctx, component, guild, button.action)
            .await;
    }

    /// Every press on the panel (change `discord-panel-controls`, designs D2 and D3): the
    /// owner check first, then whether the panel is the current one, then the action.
    async fn handle_panel_button(
        &self,
        ctx: &Context,
        component: &ComponentInteraction,
        guild: GuildId,
        action: PanelAction,
    ) {
        let who = presser(self.shared.host.owner_id(), component.user.id.get());
        if let Some(text) = press_refusal(who, self.shared.events.terminal_guidance()) {
            self.shared.log(match who {
                Presser::NoOwnerLinked => {
                    "a press on the Discord panel was refused: no owner is linked"
                }
                _ => "a press on the Discord panel was refused: not the owner",
            });
            let _ = component.create_response(&ctx.http, ephemeral(text)).await;
            return;
        }
        let current = session::panel_message_id(&self.shared, guild);
        if !is_current_panel(current, component.message.id) {
            let _ = component
                .create_response(&ctx.http, old_panel_update())
                .await;
            return;
        }
        match action {
            PanelAction::Mute => self.panel_mute(ctx, component, guild).await,
            PanelAction::Leave => self.panel_leave(ctx, component, guild).await,
            PanelAction::Mode(mode) => self.panel_mode(ctx, component, guild, mode).await,
        }
    }

    /// Leave channel (design D4): as `/leave`, which also deletes the panel; the reply is
    /// the owner's only and does not depend on the deleted message.
    async fn panel_leave(&self, ctx: &Context, component: &ComponentInteraction, guild: GuildId) {
        if component.defer_ephemeral(&ctx.http).await.is_err() {
            return;
        }
        let channel = session::session_channel_name(&self.shared, guild);
        let left = session::leave_voice(&self.shared, guild).await;
        let channel = left.then(|| channel.unwrap_or_else(|| "the voice channel".to_owned()));
        if let Some(name) = &channel {
            self.shared
                .log(format!("left {name} from the Discord panel"));
        }
        let _ = component
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new().content(leave_reply(channel.as_deref())),
            )
            .await;
    }

    /// A mode button (design D5): the save may take its time behind a deferred update; the
    /// panel shows the new mode, and only the owner sees an engine switch or a refusal.
    async fn panel_mode(
        &self,
        ctx: &Context,
        component: &ComponentInteraction,
        guild: GuildId,
        mode: AssistantMode,
    ) {
        if component.defer(&ctx.http).await.is_err() {
            return;
        }
        let result = self.shared.host.change_mode(mode).await;
        match &result {
            Ok(change) => {
                self.shared.log(format!(
                    "assistant mode {} from the Discord panel{}",
                    mode.name(),
                    if change.engine_switched {
                        ", the engine switched to Codex"
                    } else {
                        ""
                    }
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
            }
            Err(reason) => self.shared.log(format!(
                "mode change from the Discord panel refused: {reason}"
            )),
        }
        if let Some(text) = mode_follow_up(&result) {
            let _ = component
                .create_followup(
                    &ctx.http,
                    CreateInteractionResponseFollowup::new()
                        .content(text)
                        .ephemeral(true),
                )
                .await;
        }
    }

    /// Mute assistant: the app-wide mute, as the window's button and the hotkey.
    async fn panel_mute(&self, ctx: &Context, component: &ComponentInteraction, guild: GuildId) {
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
            status.user_id = Some(ready.user.id.get().to_string());
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
                "optin" => self.handle_optin(&ctx, &command).await,
                "optout" => self.handle_optout(&ctx, &command).await,
                "privacy" => self.handle_privacy(&ctx, &command).await,
                _ => {}
            },
            Interaction::Component(component) => self.handle_component(&ctx, &component).await,
            _ => {}
        }
    }

    /// A pasted link code (change `owner-link-paste`, designs D2 and D3). Every other
    /// message is dropped on the first checks, before its text is looked at.
    async fn message(&self, ctx: Context, message: Message) {
        let bot = ctx.cache.current_user().id;
        let facts = super::link_message::MessageFacts {
            from_bot: message.author.bot,
            webhook: message.webhook_id.is_some(),
            regular: matches!(
                message.kind,
                MessageType::Regular | MessageType::InlineReply
            ),
            direct: message.guild_id.is_none(),
            mentions_bot: message.mentions_user_id(bot),
        };
        if !super::link_message::is_candidate(&facts) {
            return;
        }
        let Some(attempt) = super::link_message::link_attempt(&message.content, bot.get()) else {
            return;
        };
        let name = display_name(&message.author);
        let outcome = super::link_message::complete(
            self.shared.host.as_ref(),
            &self.shared.events,
            message.author.id.get(),
            &name,
            message.guild_id.map(|guild| guild.get()),
            &attempt,
        );
        let reply = if self.shared.events.terminal_guidance() {
            outcome.terminal_reply()
        } else {
            outcome.reply()
        };
        if let Err(error) = message
            .channel_id
            .send_message(&ctx.http, super::link_message::answer(&reply))
            .await
        {
            self.shared.log(format!(
                "owner link: could not answer in {}: {error}",
                message.channel_id.get()
            ));
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

    /// Changes `owner-only-access` and `voice-consent`: seven commands; `/link` takes a code
    /// of 8 to 12 characters; globally `/link`, `/optin`, `/optout` and `/privacy` work in
    /// servers and the bot's DMs; the first three are registered as before.
    #[test]
    fn link_is_registered_next_to_the_three_commands() {
        let global = serde_json::to_value(command_definitions(true)).unwrap();
        let names: Vec<&str> = global
            .as_array()
            .unwrap()
            .iter()
            .map(|command| command["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            ["join", "leave", "status", "link", "optin", "optout", "privacy"]
        );
        for command in &global.as_array().unwrap()[3..] {
            assert_eq!(command["contexts"], serde_json::json!([0, 1]), "{command}");
        }
        for command in &global.as_array().unwrap()[4..] {
            assert!(
                command
                    .get("options")
                    .is_none_or(|o| o.as_array().is_some_and(Vec::is_empty)),
                "{command}"
            );
        }
        let optin = global[4]["description"].as_str().unwrap();
        assert!(optin.contains("hear you"), "{optin}");
        // Change `claude-and-opencode-engines`: no company named, within Discord's 100.
        assert!(optin.contains("AI assistant"), "{optin}");
        for company in ["OpenAI", "Anthropic", "Codex", "Claude"] {
            assert!(!optin.contains(company), "{optin}");
        }
        assert!(optin.chars().count() <= 100, "{optin}");
        assert!(global[5]["description"]
            .as_str()
            .unwrap()
            .contains("delete your consent"));
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
        for command in guild.as_array().unwrap() {
            assert!(
                command.get("contexts").is_none(),
                "no contexts on a guild command: {command}"
            );
        }
        assert_eq!(guild[3]["options"], link["options"]);
    }

    fn record(version: u32) -> crate::consent::ConsentRecord {
        crate::consent::ConsentRecord {
            discord_user_id: 7,
            discord_name: "Alex".to_owned(),
            agreed_at: "2026-11-18T10:05:00.123+00:00".to_owned(),
            text_version: version,
            assistant: "codex".to_owned(),
        }
    }

    /// Requirements "`/optin` needs a privacy policy", "`/optin` shows the consent text" and
    /// "Agreeing again changes nothing".
    #[test]
    fn optin_replies_with_the_text_a_note_or_the_time() {
        let provider = crate::consent::provider_name("codex");
        let provider = provider.as_str();
        let OptinReply::Note(note) = optin_reply("", None, provider) else {
            panic!("no policy: a note");
        };
        assert!(note.contains("not published a privacy policy"), "{note}");

        let version = crate::consent::CONSENT_TEXT_VERSION;
        let OptinReply::Offer {
            text,
            version: offered,
        } = optin_reply("https://example.org/p", None, provider)
        else {
            panic!("an offer");
        };
        assert_eq!(offered, version);
        assert!(
            text.contains("OpenAI") && text.contains("https://example.org/p"),
            "{text}"
        );
        assert_eq!(
            parse_consent_button(&format!("{AGREE_PREFIX}{offered}")),
            Some(ConsentButton::Agree(version))
        );
        assert_eq!(parse_consent_button(CANCEL_ID), Some(ConsentButton::Cancel));
        assert_eq!(parse_consent_button("vb:mute:1"), None);
        assert_eq!(format!("{AGREE_PREFIX}1"), "vb:optin:agree:1");

        let current = record(version);
        let OptinReply::Note(again) =
            optin_reply("https://example.org/p", Some(&current), provider)
        else {
            panic!("already agreed: a note");
        };
        assert!(
            again.contains("2026-11-18 10:05") && again.contains("/optout"),
            "{again}"
        );
        let old = record(version - 1);
        assert!(matches!(
            optin_reply("https://example.org/p", Some(&old), provider),
            OptinReply::Offer { .. }
        ));
    }

    /// Requirement "\"I agree\" records the person": the confirmation names the mode unless
    /// it is `agreed`; an older text and Cancel record nothing.
    #[test]
    fn the_buttons_reply_by_outcome_and_mode() {
        use crate::consent::AgreeOutcome;
        let agreed = button_reply(Some(AgreeOutcome::Recorded), AccessMode::Agreed);
        assert!(
            agreed.starts_with("Done.") && !agreed.contains("right now"),
            "{agreed}"
        );
        let everyone = button_reply(Some(AgreeOutcome::Recorded), AccessMode::Everyone);
        assert!(
            everyone.contains("hears everyone in the channel"),
            "{everyone}"
        );
        let owner = button_reply(Some(AgreeOutcome::Recorded), AccessMode::Owner);
        assert!(owner.contains("only its owner"), "{owner}");
        assert!(
            button_reply(Some(AgreeOutcome::OldText), AccessMode::Agreed).contains("/optin again")
        );
        assert_eq!(button_reply(None, AccessMode::Agreed), "Nothing changed.");
    }

    /// Requirements "`/optout` deletes the record" and "`/privacy` shows the policy".
    #[test]
    fn optout_and_privacy_reply() {
        use crate::consent::WithdrawOutcome;
        let removed = optout_reply(&WithdrawOutcome::Removed {
            name: "Alex".to_owned(),
        });
        assert!(removed.contains("already sent"), "{removed}");
        assert!(optout_reply(&WithdrawOutcome::NothingToRemove).contains("nothing to remove"));
        assert_eq!(
            privacy_reply("https://example.org/p"),
            "This bot's privacy policy: https://example.org/p"
        );
        assert!(privacy_reply("").contains("has not published"));
    }

    /// Scenarios "Status from another member" and "Status in the everyone mode": a line for
    /// `owner` and for `agreed`, none for `everyone`.
    #[test]
    fn status_names_the_access_mode() {
        let off = status_text(
            true,
            Some("General".to_owned()),
            "listening",
            "Realtime voice",
            false,
            AccessMode::Everyone,
        );
        assert_eq!(
            off,
            "Discord: connected. Voice channel: General. Assistant: listening. Mode: Realtime voice. not muted."
        );
        let on = status_text(
            true,
            None,
            "waiting for the wake word",
            "Whisper mode",
            true,
            AccessMode::Owner,
        );
        assert!(on.contains("Mode: Whisper mode."), "{on}");
        assert!(
            on.starts_with("Discord: connected. Voice channel: none."),
            "{on}"
        );
        assert!(
            on.ends_with("Only the owner's voice reaches the assistant here."),
            "{on}"
        );
        let agreed = status_text(
            true,
            None,
            "listening",
            "Realtime voice",
            false,
            AccessMode::Agreed,
        );
        assert!(
            agreed.ends_with(
                "Only the owner and people who ran /optin are heard here. See /privacy."
            ),
            "{agreed}"
        );
        assert!(!off.contains("/optin"));
    }

    /// Change `discord-panel-controls`, requirement "Only the linked owner presses the panel's
    /// buttons": the owner passes in every case, anyone else is refused, and with nobody
    /// linked everyone is, with the note on how to link (the window's or the terminal's).
    #[test]
    fn only_the_linked_owner_passes_the_press_check() {
        assert_eq!(presser(Some(7), 7), Presser::Owner);
        assert_eq!(presser(Some(7), 8), Presser::NotOwner);
        assert_eq!(presser(None, 7), Presser::NoOwnerLinked);
        assert_eq!(press_refusal(Presser::Owner, false), None);
        assert_eq!(press_refusal(Presser::Owner, true), None);
        assert_eq!(
            press_refusal(Presser::NotOwner, false),
            Some("These buttons are for this bot's owner only.")
        );
        assert_eq!(
            press_refusal(Presser::NotOwner, true),
            press_refusal(Presser::NotOwner, false)
        );
        let window = press_refusal(Presser::NoOwnerLinked, false).unwrap();
        assert!(
            window.starts_with("No owner is linked to this bot yet")
                && window.contains("VoiceBro → Config → Access"),
            "{window}"
        );
        let terminal = press_refusal(Presser::NoOwnerLinked, true).unwrap();
        assert!(
            terminal.contains("voicebro-server --link-owner"),
            "{terminal}"
        );
    }

    /// Design D3: only the current session's panel counts; an old one's update keeps the
    /// embed (it names none), takes every button away and says how to get a new panel.
    #[test]
    fn an_old_panel_loses_its_buttons() {
        let current = MessageId::new(10);
        assert!(is_current_panel(Some(current), MessageId::new(10)));
        assert!(!is_current_panel(Some(current), MessageId::new(11)));
        assert!(!is_current_panel(None, MessageId::new(10)));
        let update = serde_json::to_value(old_panel_update()).unwrap();
        assert_eq!(update["type"], 7, "an update of the pressed message");
        assert_eq!(update["data"]["components"], serde_json::json!([]));
        assert_eq!(
            update["data"]["content"],
            "This panel is from an earlier session. /join posts a new one."
        );
        assert!(update["data"].get("embeds").is_none(), "{update}");
    }

    /// Design D4 and D5: the replies only the owner sees.
    #[test]
    fn the_leave_and_mode_replies() {
        assert_eq!(leave_reply(Some("General")), "Left General.");
        assert_eq!(leave_reply(None), "I was not in a voice channel here.");
        assert_eq!(
            mode_follow_up(&Ok(ModeChange {
                engine_switched: false
            })),
            None
        );
        let switched = mode_follow_up(&Ok(ModeChange {
            engine_switched: true,
        }))
        .unwrap();
        assert_eq!(
            switched,
            "Switched to the realtime voice. The engine is now Codex: the voice call works only with Codex."
        );
        let refused = mode_follow_up(&Err("Not switched: the disk is full".to_owned()));
        assert_eq!(refused.as_deref(), Some("Not switched: the disk is full"));
    }
}
