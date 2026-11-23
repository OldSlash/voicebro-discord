//! `discord-panel` (change `discord-panel-controls`, task 3.1): posts the panel as `/join`
//! builds it, for each assistant mode, unmuted and muted, to the test channel's text chat;
//! reads each message back and checks its embed and its two rows of buttons (the labels, the
//! custom ids, Leave channel's danger style, the current mode marked and disabled, the other
//! modes pressable); then deletes the messages. It shows that Discord accepts and stores the
//! components, not that presses work: a press needs a person in Discord.

use crate::discord::env;
use crate::Outcome;
use anyhow::Context as _;
use serenity::all::{
    ActionRowComponent, ButtonKind, ButtonStyle, ChannelId, CreateMessage, Http, Message,
};
use voicebro_core::discord::{mode_button_label, sample_panel};
use voicebro_core::settings::AssistantMode;

/// One stored button: its label, custom id, style and whether it is disabled.
struct Stored {
    label: String,
    custom_id: String,
    style: Option<ButtonStyle>,
    disabled: bool,
}

fn rows(message: &Message) -> Vec<Vec<Stored>> {
    message
        .components
        .iter()
        .map(|row| {
            row.components
                .iter()
                .filter_map(|component| match component {
                    ActionRowComponent::Button(button) => Some(Stored {
                        label: button.label.clone().unwrap_or_default(),
                        custom_id: match &button.data {
                            ButtonKind::NonLink { custom_id, .. } => custom_id.clone(),
                            _ => String::new(),
                        },
                        style: match &button.data {
                            ButtonKind::NonLink { style, .. } => Some(*style),
                            _ => None,
                        },
                        disabled: button.disabled,
                    }),
                    _ => None,
                })
                .collect()
        })
        .collect()
}

/// What is wrong with a stored panel of `mode` and `muted` in `guild`.
fn check(message: &Message, guild: u64, mode: AssistantMode, muted: bool) -> Vec<String> {
    let mut problems = Vec::new();
    if message.embeds.len() != 1 {
        problems.push(format!("{} embeds, not 1", message.embeds.len()));
    }
    let rows = rows(message);
    if rows.len() != 2 {
        problems.push(format!("{} rows, not 2", rows.len()));
        return problems;
    }
    let first: Vec<(&str, &str)> = rows[0]
        .iter()
        .map(|button| (button.label.as_str(), button.custom_id.as_str()))
        .collect();
    let mute = if muted {
        "Unmute assistant"
    } else {
        "Mute assistant"
    };
    let mute_id = format!("voicebro:mute:{guild}");
    let leave_id = format!("voicebro:leave:{guild}");
    if first
        != [
            (mute, mute_id.as_str()),
            ("Leave channel", leave_id.as_str()),
        ]
    {
        problems.push(format!("first row {first:?}"));
    }
    if rows[0].get(1).and_then(|button| button.style) != Some(ButtonStyle::Danger) {
        problems.push("Leave channel is not the danger style".to_owned());
    }
    if rows[0].iter().any(|button| button.disabled) {
        problems.push("a button of the first row is disabled".to_owned());
    }
    if rows[1].len() != AssistantMode::ALL.len() {
        problems.push(format!("{} mode buttons", rows[1].len()));
        return problems;
    }
    for (button, candidate) in rows[1].iter().zip(AssistantMode::ALL) {
        let current = candidate == mode;
        let id = format!("voicebro:mode:{}:{guild}", candidate.name());
        if button.label != mode_button_label(candidate) || button.custom_id != id {
            problems.push(format!(
                "mode button {:?} {:?}",
                button.label, button.custom_id
            ));
        }
        if button.disabled != current {
            problems.push(format!(
                "{} is {}",
                button.label,
                if button.disabled {
                    "disabled"
                } else {
                    "pressable"
                }
            ));
        }
        let style = if current {
            ButtonStyle::Primary
        } else {
            ButtonStyle::Secondary
        };
        if button.style != Some(style) {
            problems.push(format!("{} has style {:?}", button.label, button.style));
        }
    }
    problems
}

pub fn run(_args: &[String]) -> anyhow::Result<Outcome> {
    let env = env()?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        let http = Http::new(&env.token);
        let bot = http.get_current_user().await.context("the bot's user")?;
        let channel: ChannelId = env.channel;
        let guild = env.guild;
        let mut details = vec![format!("bot: {} ({})", bot.name, bot.id.get())];
        let mut pass = true;
        let mut posted = Vec::new();
        for mode in AssistantMode::ALL {
            for muted in [false, true] {
                let (embed, components) = sample_panel(guild, mode, muted);
                let sent = channel
                    .send_message(
                        &http,
                        CreateMessage::new()
                            .content(format!(
                                "probe discord-panel: {} mode, {}",
                                mode.name(),
                                if muted { "muted" } else { "not muted" }
                            ))
                            .embed(embed)
                            .components(components),
                    )
                    .await;
                let message = match sent {
                    Ok(message) => message,
                    Err(error) => {
                        details.push(format!(
                            "{} mode, muted {muted}: refused: {error}",
                            mode.name()
                        ));
                        pass = false;
                        continue;
                    }
                };
                posted.push(message.id);
                // Read back: what Discord stored, not what it echoed.
                let stored = match channel.message(&http, message.id).await {
                    Ok(stored) => stored,
                    Err(error) => {
                        details.push(format!(
                            "could not read message {} back: {error}",
                            message.id
                        ));
                        pass = false;
                        continue;
                    }
                };
                let problems = check(&stored, guild.get(), mode, muted);
                details.push(format!(
                    "{} mode, {}: {}",
                    mode.name(),
                    if muted { "muted" } else { "not muted" },
                    if problems.is_empty() {
                        "two rows, labels, ids and the marked mode as built".to_owned()
                    } else {
                        problems.join("; ")
                    }
                ));
                pass &= problems.is_empty();
            }
        }
        let mut deleted = 0;
        for id in &posted {
            match channel.delete_message(&http, *id).await {
                Ok(()) => deleted += 1,
                Err(error) => details.push(format!("could not delete message {id}: {error}")),
            }
        }
        details.push(format!("{deleted} of {} messages deleted", posted.len()));
        pass &= deleted == posted.len() && posted.len() == AssistantMode::ALL.len() * 2;
        Ok(Outcome { pass, details })
    })
}
