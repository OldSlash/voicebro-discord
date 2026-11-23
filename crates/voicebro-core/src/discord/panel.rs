//! The member panel: one message per guild with the assistant's state and its buttons (named
//! for no engine since change `claude-and-opencode-engines`): Mute assistant and Leave
//! channel in the first row, the three assistant modes in the second (change
//! `discord-panel-controls`, design D1). Edits are coalesced to one per second by the
//! refresher task.

use super::{AssistantState, Shared};
use crate::settings::AssistantMode;
use serenity::all::{
    ButtonStyle, ChannelId, CreateActionRow, CreateButton, CreateEmbed, EditMessage, GuildId, Http,
    MessageId,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

const MUTE_PREFIX: &str = "voicebro:mute:";
const LEAVE_PREFIX: &str = "voicebro:leave:";
const MODE_PREFIX: &str = "voicebro:mode:";

/// What a panel button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelAction {
    Mute,
    Leave,
    Mode(AssistantMode),
}

/// One of the panel's buttons: the guild of the panel and what it does. Its custom id is
/// `voicebro:mute:<guild>` (as before the change, so an older panel's Mute still parses),
/// `voicebro:leave:<guild>` or `voicebro:mode:<realtime|whisper|text>:<guild>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelButton {
    pub guild: u64,
    pub action: PanelAction,
}

impl PanelButton {
    pub fn custom_id(self) -> String {
        match self.action {
            PanelAction::Mute => format!("{MUTE_PREFIX}{}", self.guild),
            PanelAction::Leave => format!("{LEAVE_PREFIX}{}", self.guild),
            PanelAction::Mode(mode) => format!("{MODE_PREFIX}{}:{}", mode.name(), self.guild),
        }
    }

    /// The button a custom id names, or `None` for any other id.
    pub fn parse(custom_id: &str) -> Option<PanelButton> {
        let guild = |text: &str| text.parse::<u64>().ok();
        if let Some(rest) = custom_id.strip_prefix(MUTE_PREFIX) {
            return guild(rest).map(|guild| PanelButton {
                guild,
                action: PanelAction::Mute,
            });
        }
        if let Some(rest) = custom_id.strip_prefix(LEAVE_PREFIX) {
            return guild(rest).map(|guild| PanelButton {
                guild,
                action: PanelAction::Leave,
            });
        }
        let (mode, rest) = custom_id.strip_prefix(MODE_PREFIX)?.split_once(':')?;
        let mode = AssistantMode::ALL
            .into_iter()
            .find(|candidate| candidate.name() == mode)?;
        guild(rest).map(|guild| PanelButton {
            guild,
            action: PanelAction::Mode(mode),
        })
    }
}

pub fn mute_button_id(guild: GuildId) -> String {
    PanelButton {
        guild: guild.get(),
        action: PanelAction::Mute,
    }
    .custom_id()
}

/// The guild a Mute button belongs to, or `None` for any other custom id.
pub fn parse_mute_button_id(custom_id: &str) -> Option<u64> {
    PanelButton::parse(custom_id)
        .filter(|button| button.action == PanelAction::Mute)
        .map(|button| button.guild)
}

/// A mode's button label.
pub fn mode_button_label(mode: AssistantMode) -> &'static str {
    match mode {
        AssistantMode::Realtime => "Realtime voice",
        AssistantMode::Whisper => "Whisper mode",
        AssistantMode::Text => "Text answers",
    }
}

/// What an old panel says once the owner pressed one of its buttons (design D3).
pub const OLD_PANEL_NOTE: &str = "This panel is from an earlier session. /join posts a new one.";

#[derive(Debug, Clone, Copy)]
pub struct PanelRef {
    pub channel: ChannelId,
    pub message: MessageId,
}

pub struct PanelView<'a> {
    pub channel_name: &'a str,
    pub participants: &'a [String],
    pub assistant: AssistantState,
    /// The assistant mode (change `whisper-mode`).
    pub mode: &'static str,
    /// The engine in use (change `claude-and-opencode-engines`).
    pub engine: &'static str,
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
        .field("Engine", view.engine, true)
        .field("Assistant", view.assistant.label(), true)
        .field("Mode", view.mode, true)
        .field("Voice channel", view.channel_name, false)
        .field("In the channel", people, false)
}

/// The two rows of buttons (design D1): Mute assistant (Unmute assistant while muted) and
/// Leave channel; then the three modes, the current one marked and not pressable.
pub fn components(guild: GuildId, muted: bool, mode: AssistantMode) -> Vec<CreateActionRow> {
    let id = |action| {
        PanelButton {
            guild: guild.get(),
            action,
        }
        .custom_id()
    };
    let mute = CreateButton::new(id(PanelAction::Mute))
        .label(if muted {
            "Unmute assistant"
        } else {
            "Mute assistant"
        })
        .style(if muted {
            ButtonStyle::Danger
        } else {
            ButtonStyle::Secondary
        });
    let leave = CreateButton::new(id(PanelAction::Leave))
        .label("Leave channel")
        .style(ButtonStyle::Danger);
    let modes = AssistantMode::ALL
        .into_iter()
        .map(|candidate| {
            let current = candidate == mode;
            CreateButton::new(id(PanelAction::Mode(candidate)))
                .label(mode_button_label(candidate))
                .style(if current {
                    ButtonStyle::Primary
                } else {
                    ButtonStyle::Secondary
                })
                .disabled(current)
        })
        .collect();
    vec![
        CreateActionRow::Buttons(vec![mute, leave]),
        CreateActionRow::Buttons(modes),
    ]
}

/// The panel as `/join` posts it for a sample state, for the `discord-panel` probe: the
/// embed and the buttons of `mode` and `muted`.
pub fn sample_panel(
    guild: GuildId,
    mode: AssistantMode,
    muted: bool,
) -> (CreateEmbed, Vec<CreateActionRow>) {
    let participants = ["Alex".to_owned(), "Sam".to_owned()];
    let view = PanelView {
        channel_name: "General",
        participants: &participants,
        assistant: if muted {
            AssistantState::Muted
        } else {
            AssistantState::WaitingForWakeWord
        },
        mode: super::mode_label(mode),
        engine: "Codex",
        muted,
    };
    (embed(&view), components(guild, muted, mode))
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
        mode: shared.host.assistant_mode(),
        engine: shared.host.engine_name(),
        muted: shared.host.is_muted(),
    };
    let edit = EditMessage::new()
        .embed(embed(&view))
        .components(components(guild, view.muted, shared.host.mode()));
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
        assert_eq!(id, "voicebro:mute:964597969980620820");
        assert_eq!(parse_mute_button_id(&id), Some(964597969980620820));
        assert_eq!(parse_mute_button_id("other:mute:1"), None);
        assert_eq!(parse_mute_button_id("voicebro:mute:abc"), None);
        assert_eq!(parse_mute_button_id("voicebro:leave:1"), None);
    }

    /// Design D1: every button's id goes back to the same button; an id of an older build's
    /// panel (`voicebro:mute:<guild>`) still parses; anything else is not a panel button.
    #[test]
    fn panel_button_ids_round_trip_and_others_are_none() {
        let guild = 964597969980620820;
        let actions = [
            PanelAction::Mute,
            PanelAction::Leave,
            PanelAction::Mode(AssistantMode::Realtime),
            PanelAction::Mode(AssistantMode::Whisper),
            PanelAction::Mode(AssistantMode::Text),
        ];
        for action in actions {
            let button = PanelButton { guild, action };
            assert_eq!(PanelButton::parse(&button.custom_id()), Some(button));
        }
        assert_eq!(
            PanelButton {
                guild,
                action: PanelAction::Mode(AssistantMode::Whisper)
            }
            .custom_id(),
            "voicebro:mode:whisper:964597969980620820"
        );
        assert_eq!(
            PanelButton::parse("voicebro:mute:42"),
            Some(PanelButton {
                guild: 42,
                action: PanelAction::Mute
            })
        );
        for foreign in [
            "",
            "vb:optin:cancel",
            "vb:optin:agree:1",
            "voicebro:mute:",
            "voicebro:mute:abc",
            "voicebro:leave:-1",
            "voicebro:mode:whisper",
            "voicebro:mode:whisper:",
            "voicebro:mode:loud:42",
            "voicebro:mode:42",
            "voicebro:join:42",
        ] {
            assert_eq!(PanelButton::parse(foreign), None, "{foreign}");
        }
    }

    /// Design D1: two rows; the first holds Mute (Unmute while muted) and Leave channel, the
    /// second the three modes with only the current one primary and disabled.
    #[test]
    fn the_components_hold_two_rows_for_every_mode_and_mute_state() {
        let guild = GuildId::new(7);
        for mode in AssistantMode::ALL {
            for muted in [false, true] {
                let rows = serde_json::to_value(components(guild, muted, mode)).unwrap();
                let rows = rows.as_array().unwrap();
                assert_eq!(rows.len(), 2);
                let first = rows[0]["components"].as_array().unwrap();
                assert_eq!(first.len(), 2);
                assert_eq!(
                    first[0]["label"],
                    if muted {
                        "Unmute assistant"
                    } else {
                        "Mute assistant"
                    }
                );
                assert_eq!(first[0]["custom_id"], "voicebro:mute:7");
                assert_eq!(first[1]["label"], "Leave channel");
                assert_eq!(first[1]["custom_id"], "voicebro:leave:7");
                assert_eq!(first[1]["style"], 4, "Leave channel is the danger style");
                let second = rows[1]["components"].as_array().unwrap();
                let labels: Vec<&str> = second
                    .iter()
                    .map(|button| button["label"].as_str().unwrap())
                    .collect();
                assert_eq!(labels, ["Realtime voice", "Whisper mode", "Text answers"]);
                for (button, candidate) in second.iter().zip(AssistantMode::ALL) {
                    let current = candidate == mode;
                    assert_eq!(
                        button["custom_id"],
                        format!("voicebro:mode:{}:7", candidate.name())
                    );
                    assert_eq!(button["disabled"], current, "{mode:?} {candidate:?}");
                    assert_eq!(
                        button["style"],
                        if current { 1 } else { 2 },
                        "primary for the current mode, secondary for the others"
                    );
                }
            }
        }
    }
}
