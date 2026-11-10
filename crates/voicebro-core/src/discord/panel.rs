//! The member panel: one message per guild with the assistant's state and a Mute GPT
//! button. Edits are coalesced to one per second by the refresher task.

use super::{AssistantState, Shared};
use serenity::all::{
    ButtonStyle, ChannelId, CreateActionRow, CreateButton, CreateEmbed, EditMessage, GuildId, Http,
    MessageId,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

const PREFIX: &str = "voicebro:mute:";

pub fn mute_button_id(guild: GuildId) -> String {
    format!("{PREFIX}{}", guild.get())
}

/// The guild a Mute button belongs to, or `None` for a foreign custom id.
pub fn parse_mute_button_id(custom_id: &str) -> Option<u64> {
    custom_id.strip_prefix(PREFIX)?.parse().ok()
}

#[derive(Debug, Clone, Copy)]
pub struct PanelRef {
    pub channel: ChannelId,
    pub message: MessageId,
}

pub struct PanelView<'a> {
    pub channel_name: &'a str,
    pub participants: &'a [String],
    pub assistant: AssistantState,
    pub muted: bool,
}

pub fn embed(view: &PanelView<'_>) -> CreateEmbed {
    let people = if view.participants.is_empty() {
        "nobody else".to_owned()
    } else {
        view.participants.join(", ")
    };
    CreateEmbed::new()
        .title("VoiceBro")
        .field("Engine", "Codex", true)
        .field("Assistant", view.assistant.label(), true)
        .field("Voice channel", view.channel_name, false)
        .field("In the channel", people, false)
}

pub fn components(guild: GuildId, muted: bool) -> Vec<CreateActionRow> {
    let button = CreateButton::new(mute_button_id(guild))
        .label(if muted { "Unmute GPT" } else { "Mute GPT" })
        .style(if muted {
            ButtonStyle::Danger
        } else {
            ButtonStyle::Secondary
        });
    vec![CreateActionRow::Buttons(vec![button])]
}

/// Edits one panel with the current state; a deleted message is not an error.
async fn refresh_one(shared: &Shared, http: &Http, guild: GuildId) {
    let (panel, channel_name, participants) = {
        let sessions = shared.sessions.lock().expect("sessions lock");
        let Some(session) = sessions.get(&guild.get()) else {
            return;
        };
        let Some(panel) = session.panel else {
            return;
        };
        (
            panel,
            session.channel_name.clone(),
            session.participants.clone(),
        )
    };
    let view = PanelView {
        channel_name: &channel_name,
        participants: &participants,
        assistant: shared.host.assistant_state(guild.get()),
        muted: shared.host.is_muted(),
    };
    let edit = EditMessage::new()
        .embed(embed(&view))
        .components(components(guild, view.muted));
    if let Err(error) = panel.channel.edit_message(http, panel.message, edit).await {
        log::debug!("panel edit for guild {} failed: {error}", guild.get());
    }
}

/// Runs for the life of the connection: whenever the state is marked dirty, edits every
/// panel, at most once per second.
pub(crate) async fn refresher(shared: Arc<Shared>) {
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    loop {
        ticker.tick().await;
        let Some(http) = shared.http() else {
            continue;
        };
        if !shared.panels_dirty.swap(false, Ordering::AcqRel) {
            continue;
        }
        let guilds: Vec<u64> = shared
            .sessions
            .lock()
            .expect("sessions lock")
            .keys()
            .copied()
            .collect();
        for guild in guilds {
            refresh_one(&shared, &http, GuildId::new(guild)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mute_button_ids_are_guild_scoped() {
        let guild = GuildId::new(964597969980620820);
        let id = mute_button_id(guild);
        assert_eq!(parse_mute_button_id(&id), Some(964597969980620820));
        assert_eq!(parse_mute_button_id("other:mute:1"), None);
        assert_eq!(parse_mute_button_id("voicebro:mute:abc"), None);
    }

    #[test]
    fn components_change_label_with_mute() {
        let guild = GuildId::new(1);
        let unmuted = serde_json::to_value(components(guild, false)).unwrap();
        let muted = serde_json::to_value(components(guild, true)).unwrap();
        assert_eq!(unmuted[0]["components"][0]["label"], "Mute GPT");
        assert_eq!(muted[0]["components"][0]["label"], "Unmute GPT");
    }
}
