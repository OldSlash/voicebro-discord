//! `discord-post` (change `post-assistant-answers`, design D11): posts a sample agent answer
//! to the test channel through the app's own formatting, splitting and message builder, then
//! checks what Discord stored: every message within 2000 characters, every code block
//! closed in its own message, nobody mentioned, and a preview under the link.
//!
//! The test channel is a voice channel; the answer goes to its built-in text chat, as the
//! `voice_channel` destination does. The sample mentions `@everyone` and the bot itself, so
//! a mistake would ping the test server, not one person.

use crate::discord::env;
use crate::Outcome;
use anyhow::Context as _;
use serenity::all::{ChannelId, Http, Message};
use std::time::{Duration, Instant};
use voicebro_core::discord::answers::{
    answer_message, format_answer, split_for_discord, MESSAGE_LIMIT,
};

/// The link whose preview the probe waits for: a page with a picture in its preview.
const LINK: &str = "https://en.wikipedia.org/wiki/Bangkok";
const TASK: &str =
    "find the weather in Bangkok for the next three days and a script that checks it";

/// The built-in sample, about 5 800 characters: a paragraph with the link and the mentions,
/// a table, a 2 500-character Rust block and closing paragraphs, long enough for four
/// messages, the second and third holding parts of the code block. `bot` is the bot's own
/// user id, for the user mention.
fn sample(bot: u64) -> String {
    // Not inside parentheses: Discord gives no preview to a link there (checked on
    // 2026-11-16, `(source: [name](url))` stayed without one).
    let mut text = format!(
        "**Bangkok, the next three days**, from [Wikipedia on Bangkok]({LINK}). @everyone, and <@{bot}>, this line mentions people only as text.\n\n"
    );
    text.push_str("| Day | High | Low | Sky | Rain |\n|---|---|---|---|---|\n");
    for (day, high, low, sky, rain) in [
        ("Monday", 34, 27, "sun and clouds", "20 %"),
        ("Tuesday", 33, 26, "thunderstorms in the afternoon", "60 %"),
        ("Wednesday", 32, 26, "showers", "70 %"),
    ] {
        text.push_str(&format!(
            "| {day} | {high} °C | {low} °C | {sky} | {rain} |\n"
        ));
    }
    text.push('\n');
    for index in 1..=3 {
        text.push_str(&format!(
            "{index}. The forecast for the afternoon changes quickly in this season: a dry morning can turn into a heavy shower within an hour, so the numbers above are the most likely values, not promises. Carry an umbrella and check again before you leave.\n"
        ));
    }
    text.push_str(
        "\nA small script that reads the forecast and prints a line per day:\n\n```rust\n",
    );
    let mut code = String::new();
    let mut line = 0;
    while code.len() < 2_480 {
        line += 1;
        code.push_str(&format!(
            "    let day_{line:02} = forecast.day({line}).expect(\"day {line}\"); // one line per day\n"
        ));
    }
    text.push_str("fn main() {\n");
    text.push_str(&code);
    text.push_str("}\n```\n\n");
    for index in 1..=8 {
        text.push_str(&format!(
            "Note {index}: the high and low are for the city centre; the airports and the river banks are often a degree cooler, and the humidity makes 33 °C feel closer to 40 °C. Shade, water and a slower pace help more than anything else.\n\n"
        ));
    }
    text.push_str("That is all for the three days.");
    text
}

/// A code fence that changes whether a block is open.
fn toggles_fence(line: &str) -> bool {
    line.trim_start()
        .strip_prefix("```")
        .is_some_and(|rest| !rest.contains("```"))
}

/// Checks one stored message; returns the problems found.
fn check(message: &Message, sent: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let length = message.content.encode_utf16().count();
    if length > MESSAGE_LIMIT {
        problems.push(format!("{length} characters, over {MESSAGE_LIMIT}"));
    }
    if message.content != sent {
        problems.push("Discord stored other text than was sent".to_owned());
    }
    let fences = message
        .content
        .lines()
        .filter(|line| toggles_fence(line))
        .count();
    if fences % 2 != 0 {
        problems.push(format!("{fences} code fences: a block is left open"));
    }
    if message.mention_everyone {
        problems.push("mention_everyone is true".to_owned());
    }
    if !message.mentions.is_empty() {
        problems.push(format!("{} users mentioned", message.mentions.len()));
    }
    if !message.mention_roles.is_empty() {
        problems.push(format!("{} roles mentioned", message.mention_roles.len()));
    }
    problems
}

pub fn post(args: &[String]) -> anyhow::Result<Outcome> {
    let env = env()?;
    let file = args
        .iter()
        .position(|arg| arg == "--file")
        .map(|index| {
            args.get(index + 1)
                .ok_or_else(|| anyhow::anyhow!("--file needs a path"))
        })
        .transpose()?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        let http = Http::new(&env.token);
        let bot = http.get_current_user().await.context("the bot's user")?;
        let (task, answer) = match file {
            Some(path) => (
                format!("probe discord-post: {path}"),
                std::fs::read_to_string(path).with_context(|| format!("read {path}"))?,
            ),
            None => (TASK.to_owned(), sample(bot.id.get())),
        };
        let text = format_answer(Some(&task), &answer);
        let parts = split_for_discord(&text, MESSAGE_LIMIT);
        let mut details = vec![
            format!("bot: {} ({})", bot.name, bot.id.get()),
            format!(
                "answer: {} characters; formatted: {} characters in {} messages",
                answer.chars().count(),
                text.chars().count(),
                parts.len()
            ),
        ];
        let channel: ChannelId = env.channel;
        let mut pass = true;
        let mut posted: Vec<Message> = Vec::new();
        for (index, part) in parts.iter().enumerate() {
            let message = match channel.send_message(&http, answer_message(part)).await {
                Ok(message) => message,
                Err(error) => {
                    details.push(format!("message {}: refused: {error}", index + 1));
                    return Ok(Outcome::fail(details));
                }
            };
            let problems = check(&message, part);
            details.push(format!(
                "message {}: {} characters, https://discord.com/channels/{}/{}/{}{}",
                index + 1,
                message.content.encode_utf16().count(),
                env.guild.get(),
                channel.get(),
                message.id.get(),
                if problems.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", problems.join("; "))
                }
            ));
            pass &= problems.is_empty();
            posted.push(message);
        }

        // The preview: Discord adds the link's embed shortly after the post.
        let Some(linked) = posted.iter().find(|message| message.content.contains(LINK)) else {
            details.push("no message holds the link; preview not checked".to_owned());
            return Ok(Outcome {
                pass: pass && file.is_some(),
                details,
            });
        };
        let started = Instant::now();
        let mut preview = None;
        while started.elapsed() < Duration::from_secs(15) {
            tokio::time::sleep(Duration::from_millis(1_500)).await;
            match channel.message(&http, linked.id).await {
                Ok(message) if !message.embeds.is_empty() => {
                    preview = Some(message.embeds[0].clone());
                    break;
                }
                Ok(_) => {}
                Err(error) => {
                    details.push(format!("could not read the message back for its preview: {error}"));
                    break;
                }
            }
        }
        match preview {
            // Discord sends most link previews as a `components` embed now (a text block and
            // a media gallery, seen on 2026-11-16), which serenity 0.12's `Embed` does not
            // model: its title and picture are not readable here.
            Some(embed) => details.push(format!(
                "preview after {:.1} s: kind {}, {}",
                started.elapsed().as_secs_f32(),
                embed.kind.as_deref().unwrap_or("unknown"),
                match (embed.kind.as_deref(), embed.thumbnail.is_some() || embed.image.is_some()) {
                    (Some("components"), _) => {
                        "its title and picture are in components serenity does not read; look at the message".to_owned()
                    }
                    (_, picture) => format!(
                        "title {:?}, {} picture",
                        embed.title.unwrap_or_default(),
                        if picture { "with a" } else { "no" }
                    ),
                }
            )),
            None => {
                details.push("no preview under the link within 15 s".to_owned());
                pass = false;
            }
        }
        Ok(Outcome { pass, details })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_splits_into_four_closed_messages() {
        let text = format_answer(Some(TASK), &sample(1568533518601031690));
        let parts = split_for_discord(&text, MESSAGE_LIMIT);
        assert_eq!(
            parts.len(),
            4,
            "{:?}",
            parts.iter().map(|p| p.len()).collect::<Vec<_>>()
        );
        for part in &parts {
            assert!(part.encode_utf16().count() <= MESSAGE_LIMIT);
            assert_eq!(
                part.lines().filter(|line| toggles_fence(line)).count() % 2,
                0
            );
        }
        assert!(parts[0].contains(LINK) && parts[0].contains("@everyone"));
    }
}
