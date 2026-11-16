//! The agent's answers in a Discord text chat (change `post-assistant-answers`): where an
//! answer goes, how it is formatted and split, how it is posted without pinging anyone,
//! and the text channels the window offers. See `docs/ARCHITECTURE.md` 4.2.

use super::{session, Shared};
use crate::assistant::AgentAnswer;
use serde::Serialize;
use serenity::all::{
    Cache, ChannelId, ChannelType, CreateAllowedMentions, CreateMessage, GuildId,
    PermissionOverwrite, PermissionOverwriteType, Permissions, RoleId, UserId,
};
use std::collections::HashMap;
use std::sync::Arc;

/// Discord's limit for a bot's message, in characters.
pub const MESSAGE_LIMIT: usize = 2000;
/// How much of the task the first line shows, in characters.
pub const TASK_LINE_LIMIT: usize = 150;

/// What the destination setting asks for; resolved to a channel when an answer arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerDestination {
    /// The built-in text chat of the voice channel the bot is in for the answer's server.
    VoiceChannel,
    /// The channel the current voice session's `/join` was typed in.
    CommandChannel,
    /// One chosen channel, for answers from its own server.
    Channel(u64),
}

/// How a post went, for the runtime's handler (log line, `answers` error).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostOutcome {
    /// Every part was posted. `channel` is the channel's name, `server` its server's.
    Posted {
        channel: String,
        server: String,
        messages: usize,
        characters: usize,
    },
    /// Nothing was sent, on purpose (the bot had left, another server). Logged, not an
    /// error; `repeat` when the same reason was already logged in this voice session.
    NotPosted { reason: String, repeat: bool },
    /// Discord refused a part, or the channel is gone; the remaining parts were not sent.
    /// `message` is Discord's reason.
    Refused {
        channel: String,
        server: String,
        message: String,
    },
}

/// The channels of one voice session that the destinations need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionChannels {
    pub(crate) voice: u64,
    pub(crate) command: u64,
}

/// Why an answer is not posted where its destination points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unresolved {
    /// The bot is no longer in a voice channel of the answer's server.
    Left,
    /// The chosen channel belongs to another server.
    OtherServer { channel_guild: u64 },
    /// The chosen channel is in no server the bot is in (deleted, or not visible).
    Gone,
}

/// Resolves a destination for an answer from `guild`: `session` is that server's voice
/// session, if the bot is still in one; `chosen_guild` the server the chosen channel was
/// found in, if any. Returns the channel to post in.
pub(crate) fn resolve(
    destination: AnswerDestination,
    guild: u64,
    session: Option<SessionChannels>,
    chosen_guild: Option<u64>,
) -> Result<u64, Unresolved> {
    match destination {
        AnswerDestination::VoiceChannel => session.map(|s| s.voice).ok_or(Unresolved::Left),
        AnswerDestination::CommandChannel => session.map(|s| s.command).ok_or(Unresolved::Left),
        AnswerDestination::Channel(channel) => match chosen_guild {
            Some(owner) if owner == guild => Ok(channel),
            Some(owner) => Err(Unresolved::OtherServer {
                channel_guild: owner,
            }),
            None => Err(Unresolved::Gone),
        },
    }
}

/// The text as posted: a small first line naming the task (Discord's `-#` subtext), then
/// the answer with its Markdown, its tables put into code blocks.
pub fn format_answer(task: Option<&str>, text: &str) -> String {
    let task = task
        .map(|task| task.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|task| !task.is_empty());
    let line = match task {
        Some(task) => format!("-# Codex: {}", cut(&task, TASK_LINE_LIMIT)),
        None => "-# Codex".to_owned(),
    };
    format!("{line}\n{}", wrap_tables(text.trim()))
}

/// `text` cut to `limit` characters, `…` in place of the rest.
fn cut(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let kept: String = text.chars().take(limit.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// A code fence line: its info string (the language), or `None` for another line.
fn fence_info(line: &str) -> Option<&str> {
    line.trim_start().strip_prefix("```").map(str::trim)
}

/// A fence line that changes whether a code block is open (```` ```code``` ```` on one line
/// opens and closes it at once).
fn toggles_fence(line: &str) -> bool {
    fence_info(line).is_some_and(|rest| !rest.contains("```"))
}

/// A table's separator row: cells of dashes with optional colons, at least one `|`.
fn is_table_separator(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.contains('|') || !trimmed.contains('-') {
        return false;
    }
    trimmed.trim_matches('|').split('|').all(|cell| {
        let cell = cell.trim();
        !cell.is_empty() && cell.trim_matches(':').chars().all(|c| c == '-')
    })
}

/// The cells of a table row, trimmed, without the outer pipes.
fn table_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    inner
        .split('|')
        .map(|cell| cell.trim().to_owned())
        .collect()
}

/// The table's rows with every column padded to its widest cell, so the columns line up in
/// a code block's fixed-width font. The second row is the separator.
fn aligned_table(rows: &[&str]) -> Vec<String> {
    let cells: Vec<Vec<String>> = rows.iter().map(|row| table_cells(row)).collect();
    let columns = cells.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![3usize; columns];
    for (index, row) in cells.iter().enumerate() {
        if index == 1 {
            continue;
        }
        for (column, cell) in row.iter().enumerate() {
            widths[column] = widths[column].max(cell.chars().count());
        }
    }
    cells
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let parts: Vec<String> = widths
                .iter()
                .enumerate()
                .map(|(column, width)| {
                    if index == 1 {
                        return "-".repeat(*width);
                    }
                    let cell = row.get(column).map(String::as_str).unwrap_or("");
                    let pad = width.saturating_sub(cell.chars().count());
                    format!("{cell}{}", " ".repeat(pad))
                })
                .collect();
            format!("| {} |", parts.join(" | "))
        })
        .collect()
}

/// Puts every Markdown table (a row with `|`, a separator row, then rows with `|`) outside
/// a code block into a code block of its own, its columns padded to line up. Discord does
/// not render tables. Everything else stays as written.
pub fn wrap_tables(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut in_fence = false;
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if toggles_fence(line) {
            in_fence = !in_fence;
        }
        let starts_table = !in_fence
            && line.contains('|')
            && lines
                .get(index + 1)
                .is_some_and(|next| is_table_separator(next));
        if !starts_table {
            out.push(line.to_owned());
            index += 1;
            continue;
        }
        let mut end = index + 2;
        while end < lines.len() && lines[end].contains('|') && !lines[end].trim().is_empty() {
            end += 1;
        }
        out.push("```".to_owned());
        out.extend(aligned_table(&lines[index..end]));
        out.push("```".to_owned());
        index = end;
    }
    out.join("\n")
}

/// Length as Discord counts a message, taken as UTF-16 units: never less than the number of
/// characters, so a part within the limit this way is within it either way.
fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The largest byte index whose prefix of `text` fits in `budget` units.
fn fit(text: &str, budget: usize) -> usize {
    let mut used = 0;
    for (index, c) in text.char_indices() {
        used += c.len_utf16();
        if used > budget {
            return index;
        }
    }
    text.len()
}

/// Where a part may end, and whether a code block is open there.
#[derive(Debug, Clone, Copy)]
struct Cut {
    at: usize,
    in_fence: bool,
}

/// The last cut of each kind up to a byte limit, outside (`[0]`) and inside (`[1]`) a code
/// block, and the code block open after each scanned line.
struct Scan {
    blank: [Option<Cut>; 2],
    line: [Option<Cut>; 2],
    space: [Option<Cut>; 2],
    /// (byte index where the line ends, info string of the block open after it)
    fences: Vec<(usize, Option<String>)>,
}

impl Scan {
    /// The code block open at byte `at`: the state after every line that ends at or before
    /// it, starting from `open`.
    fn fence_at(&self, at: usize, open: Option<&str>) -> Option<String> {
        let mut current = open.map(str::to_owned);
        for (end, fence) in &self.fences {
            if *end > at {
                break;
            }
            current.clone_from(fence);
        }
        current
    }
}

/// Scans `rest` up to byte `limit`; `open` is the code block `rest` starts inside, if any.
fn scan(rest: &str, limit: usize, open: Option<&str>) -> Scan {
    let mut found = Scan {
        blank: [None; 2],
        line: [None; 2],
        space: [None; 2],
        fences: Vec::new(),
    };
    let mut fence: Option<String> = open.map(str::to_owned);
    let lines: Vec<&str> = rest.split('\n').collect();
    let mut start = 0;
    for (number, line) in lines.iter().enumerate() {
        if start > limit {
            break;
        }
        let end = start + line.len();
        let inside_line = fence.is_some();
        if fence_info(line).is_none() {
            for (offset, c) in line.char_indices() {
                let at = start + offset;
                if c == ' ' && at > 0 && at <= limit {
                    found.space[usize::from(inside_line)] = Some(Cut {
                        at,
                        in_fence: inside_line,
                    });
                }
            }
        }
        let opens = toggles_fence(line) && fence.is_none();
        if toggles_fence(line) {
            fence = match fence {
                Some(_) => None,
                None => fence_info(line).map(str::to_owned),
            };
        }
        found.fences.push((end, fence.clone()));
        if let Some(next) = lines.get(number + 1) {
            let inside = fence.is_some();
            // No cut right after a block opens or right before it closes: either would
            // leave an empty block.
            let closes_next = inside && toggles_fence(next);
            if end > 0 && end <= limit && !opens && !closes_next {
                let cut = Cut {
                    at: end,
                    in_fence: inside,
                };
                found.line[usize::from(inside)] = Some(cut);
                if next.trim().is_empty() && !line.trim().is_empty() {
                    found.blank[usize::from(inside)] = Some(cut);
                }
            }
        }
        start = end + 1;
    }
    found
}

/// The rest after an outside cut: the space or the line breaks and blank lines that ended
/// the part are not carried over.
fn after_cut(next: &str) -> &str {
    if let Some(rest) = next.strip_prefix(' ') {
        return rest;
    }
    let mut rest = next;
    loop {
        rest = rest.trim_start_matches(['\n', '\r']);
        match rest.split_once('\n') {
            Some((first, more)) if first.trim().is_empty() => rest = more,
            None if rest.trim().is_empty() => return "",
            _ => return rest,
        }
    }
}

/// Splits a formatted answer into messages of at most `limit` characters, in order.
///
/// A cut falls at a blank line, else a line break, else a space, else at the limit, outside
/// code blocks first; of each kind the last one before the limit, and one in the second
/// half of the message is preferred to a better kind earlier. Only a code block longer than
/// one message is cut: that part ends with a closing fence and the next one opens the block
/// again with its language.
pub fn split_for_discord(text: &str, limit: usize) -> Vec<String> {
    const CLOSE: &str = "\n```";
    let mut parts = Vec::new();
    let mut rest = text.trim_end();
    let mut open: Option<String> = None;
    while !rest.is_empty() {
        let prefix = open
            .as_ref()
            .map(|info| format!("```{info}\n"))
            .unwrap_or_default();
        if units(&prefix) + units(rest) <= limit {
            parts.push(format!("{prefix}{rest}"));
            break;
        }
        let budget = limit.saturating_sub(units(&prefix));
        let outside_limit = fit(rest, budget);
        let inside_limit = fit(rest, budget.saturating_sub(units(CLOSE)));
        let found = scan(rest, outside_limit, open.as_deref());
        let half = outside_limit / 2;
        let fits = |cut: Option<Cut>| cut.filter(|cut| !cut.in_fence || cut.at <= inside_limit);
        let late = |cut: Option<Cut>| fits(cut).filter(|cut| cut.at >= half);
        let choice = late(found.blank[0])
            .or(late(found.line[0]))
            .or(late(found.space[0]))
            .or(late(found.line[1]))
            .or(late(found.space[1]))
            .or(fits(found.blank[0]))
            .or(fits(found.line[0]))
            .or(fits(found.space[0]))
            .or(fits(found.line[1]))
            .or(fits(found.space[1]));
        let cut = choice.unwrap_or_else(|| {
            // No line break or space to cut at: cut at the limit.
            let inside = found.fence_at(outside_limit, open.as_deref()).is_some();
            let at = if inside { inside_limit } else { outside_limit };
            let at = at.max(rest.chars().next().map_or(1, char::len_utf8));
            Cut {
                at,
                in_fence: found.fence_at(at, open.as_deref()).is_some(),
            }
        });
        let fence = if cut.in_fence {
            found.fence_at(cut.at, open.as_deref())
        } else {
            None
        };
        let body = rest[..cut.at].trim_end();
        parts.push(match fence {
            Some(_) => format!("{prefix}{body}{CLOSE}"),
            None => format!("{prefix}{body}"),
        });
        let next = &rest[cut.at..];
        rest = match fence {
            Some(_) => next.strip_prefix('\n').unwrap_or(next),
            None => after_cut(next),
        };
        open = fence;
    }
    parts
}

/// What a post sends besides the text: nobody is mentioned (`parse: []`, no users, no
/// roles), so `@everyone`, `@here`, roles and users stay plain text. Link previews stay on.
pub fn answer_message(part: &str) -> CreateMessage {
    CreateMessage::new()
        .content(part)
        .allowed_mentions(CreateAllowedMentions::new())
}

/// The three permissions a post needs in its channel.
pub const POST_PERMISSIONS: [(Permissions, &str); 3] = [
    (Permissions::VIEW_CHANNEL, "View Channel"),
    (Permissions::SEND_MESSAGES, "Send Messages"),
    (Permissions::EMBED_LINKS, "Embed Links"),
];

/// The bot's permissions in a channel after the guild's roles and the channel's overwrites,
/// as Discord computes them. `member` is the bot's own member (its user id and roles);
/// `None` when the cache does not hold it, and then nothing can be said.
pub(crate) fn channel_permissions(
    guild: u64,
    owner: u64,
    roles: &HashMap<RoleId, Permissions>,
    overwrites: &[PermissionOverwrite],
    member: Option<(UserId, &[RoleId])>,
) -> Option<Permissions> {
    let (user, member_roles) = member?;
    if user.get() == owner {
        return Some(Permissions::all());
    }
    let everyone = RoleId::new(guild);
    let role = |id: &RoleId| roles.get(id).copied().unwrap_or(Permissions::empty());
    let mut permissions = role(&everyone);
    for id in member_roles {
        permissions |= role(id);
    }
    if permissions.contains(Permissions::ADMINISTRATOR) {
        return Some(Permissions::all());
    }
    let mut role_allow = Permissions::empty();
    let mut role_deny = Permissions::empty();
    let mut member_overwrite = None;
    for overwrite in overwrites {
        match overwrite.kind {
            PermissionOverwriteType::Role(id) if id == everyone => {
                permissions &= !overwrite.deny;
                permissions |= overwrite.allow;
            }
            PermissionOverwriteType::Role(id) if member_roles.contains(&id) => {
                role_deny |= overwrite.deny;
                role_allow |= overwrite.allow;
            }
            PermissionOverwriteType::Member(id) if id == user => {
                member_overwrite = Some((overwrite.allow, overwrite.deny));
            }
            _ => {}
        }
    }
    permissions &= !role_deny;
    permissions |= role_allow;
    if let Some((allow, deny)) = member_overwrite {
        permissions &= !deny;
        permissions |= allow;
    }
    Some(permissions)
}

/// Whether the bot may post in a channel: View Channel, Send Messages and Embed Links;
/// `None` without the bot's member in the cache.
pub(crate) fn can_post(
    guild: u64,
    owner: u64,
    roles: &HashMap<RoleId, Permissions>,
    overwrites: &[PermissionOverwrite],
    member: Option<(UserId, &[RoleId])>,
) -> Option<bool> {
    channel_permissions(guild, owner, roles, overwrites, member)
        .map(|permissions| missing(permissions).is_empty())
}

/// The names of the post permissions `permissions` lacks.
fn missing(permissions: Permissions) -> Vec<&'static str> {
    POST_PERMISSIONS
        .iter()
        .filter(|(needed, _)| !permissions.contains(*needed))
        .map(|(_, name)| *name)
        .collect()
}

/// One text or announcement channel the window offers for answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextChannelInfo {
    /// The channel id, as a string (a JavaScript number cannot hold it).
    pub id: String,
    pub name: String,
    /// Whether the bot may post there; `None` when its member is not in the cache.
    pub can_post: Option<bool>,
}

/// A server's text channels, in Discord's order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextChannelGroup {
    pub guild_id: String,
    pub guild_name: String,
    pub channels: Vec<TextChannelInfo>,
}

/// The bot's permissions in a cached channel, when the cache holds the channel and the
/// bot's member.
fn cached_permissions(cache: &Cache, guild: u64, channel: u64) -> Option<Permissions> {
    let bot = cache.current_user().id;
    let guild = cache.guild(GuildId::new(guild))?;
    let channel = guild.channels.get(&ChannelId::new(channel))?;
    let member = guild.members.get(&bot)?;
    let roles: HashMap<RoleId, Permissions> = guild
        .roles
        .iter()
        .map(|(id, role)| (*id, role.permissions))
        .collect();
    channel_permissions(
        guild.id.get(),
        guild.owner_id.get(),
        &roles,
        &channel.permission_overwrites,
        Some((member.user.id, &member.roles)),
    )
}

/// The text and announcement channels of every cached server, servers by name, channels in
/// Discord's order (outside any category first, then by the category's position, then by
/// the channel's position).
pub(crate) fn text_channels(cache: &Cache) -> Vec<TextChannelGroup> {
    let bot = cache.current_user().id;
    let mut groups: Vec<TextChannelGroup> = cache
        .guilds()
        .into_iter()
        .filter_map(|id| {
            let guild = cache.guild(id)?;
            let roles: HashMap<RoleId, Permissions> = guild
                .roles
                .iter()
                .map(|(id, role)| (*id, role.permissions))
                .collect();
            let member = guild.members.get(&bot);
            let category = |parent: Option<ChannelId>| {
                parent
                    .and_then(|parent| guild.channels.get(&parent))
                    .map_or((0u8, 0u16, 0u64), |category| {
                        (1, category.position, category.id.get())
                    })
            };
            let mut channels: Vec<_> = guild
                .channels
                .values()
                .filter(|channel| matches!(channel.kind, ChannelType::Text | ChannelType::News))
                .map(|channel| {
                    let key = (
                        category(channel.parent_id),
                        channel.position,
                        channel.id.get(),
                    );
                    let info = TextChannelInfo {
                        id: channel.id.get().to_string(),
                        name: channel.name.clone(),
                        can_post: can_post(
                            id.get(),
                            guild.owner_id.get(),
                            &roles,
                            &channel.permission_overwrites,
                            member.map(|member| (member.user.id, member.roles.as_slice())),
                        ),
                    };
                    (key, info)
                })
                .collect();
            channels.sort_by_key(|(key, _)| *key);
            Some(TextChannelGroup {
                guild_id: id.get().to_string(),
                guild_name: guild.name.clone(),
                channels: channels.into_iter().map(|(_, info)| info).collect(),
            })
        })
        .collect();
    groups.sort_by_key(|group| group.guild_name.to_lowercase());
    groups
}

/// A server's name from the cache, else its id.
fn server_name(cache: Option<&Arc<Cache>>, guild: u64) -> String {
    cache
        .and_then(|cache| cache.guild(GuildId::new(guild)).map(|g| g.name.clone()))
        .unwrap_or_else(|| guild.to_string())
}

/// A channel's name from the cache, else its id.
fn channel_name(cache: Option<&Arc<Cache>>, guild: u64, channel: u64) -> String {
    cache
        .and_then(|cache| {
            let guild = cache.guild(GuildId::new(guild))?;
            let name = guild.channels.get(&ChannelId::new(channel))?.name.clone();
            Some(name)
        })
        .unwrap_or_else(|| channel.to_string())
}

/// The server a cached channel belongs to.
fn guild_of_channel(cache: &Cache, channel: u64) -> Option<u64> {
    let channel = ChannelId::new(channel);
    cache
        .guilds()
        .into_iter()
        .find(|id| {
            cache
                .guild(*id)
                .is_some_and(|guild| guild.channels.contains_key(&channel))
        })
        .map(GuildId::get)
}

/// Discord's own reason for a refused request.
fn discord_reason(error: &serenity::Error) -> String {
    match error {
        serenity::Error::Http(serenity::http::HttpError::UnsuccessfulRequest(response)) => {
            format!(
                "{} (HTTP {}, code {})",
                response.error.message,
                response.status_code.as_u16(),
                response.error.code
            )
        }
        other => other.to_string(),
    }
}

/// Posts an answer where `destination` points: formatted, split, each part in order with no
/// mentions allowed. A refused part stops the rest; nothing is retried. serenity waits out
/// Discord's rate limits itself.
pub(crate) async fn post_answer(
    shared: &Arc<Shared>,
    answer: &AgentAnswer,
    destination: AnswerDestination,
) -> PostOutcome {
    let Some(http) = shared.http() else {
        return PostOutcome::NotPosted {
            reason: "the bot is not connected to Discord".to_owned(),
            repeat: false,
        };
    };
    let cache = shared.cache();
    let guild = answer.guild;
    let session = session::session_channels(shared, GuildId::new(guild));
    let chosen = match destination {
        AnswerDestination::Channel(channel) => Some(channel),
        _ => None,
    };
    let chosen_guild = chosen
        .zip(cache.as_ref())
        .and_then(|(channel, cache)| guild_of_channel(cache, channel));
    let channel = match resolve(destination, guild, session, chosen_guild) {
        Ok(channel) => channel,
        Err(Unresolved::Left) => {
            return PostOutcome::NotPosted {
                reason: format!(
                    "the bot had left the voice channel in {}",
                    server_name(cache.as_ref(), guild)
                ),
                repeat: false,
            };
        }
        Err(Unresolved::OtherServer { channel_guild }) => {
            let first = shared
                .answers_elsewhere
                .lock()
                .expect("answers notice lock")
                .insert(guild);
            let server = server_name(cache.as_ref(), guild);
            return PostOutcome::NotPosted {
                reason: format!(
                    "the chosen channel #{} belongs to {}, not to {server}; answers from {server} are not posted",
                    channel_name(cache.as_ref(), channel_guild, chosen.unwrap_or_default()),
                    server_name(cache.as_ref(), channel_guild),
                ),
                repeat: !first,
            };
        }
        Err(Unresolved::Gone) => {
            return PostOutcome::Refused {
                channel: chosen.unwrap_or_default().to_string(),
                server: server_name(cache.as_ref(), guild),
                message:
                    "the channel is in no server the bot is in (deleted, or the bot cannot see it)"
                        .to_owned(),
            };
        }
    };
    let name = match destination {
        AnswerDestination::VoiceChannel => {
            session::session_channel_name(shared, GuildId::new(guild))
                .unwrap_or_else(|| channel_name(cache.as_ref(), guild, channel))
        }
        _ => channel_name(cache.as_ref(), guild, channel),
    };
    let server = server_name(cache.as_ref(), guild);
    let text = format_answer(answer.task.as_deref(), &answer.text);
    let parts = split_for_discord(&text, MESSAGE_LIMIT);
    for part in &parts {
        if let Err(error) = ChannelId::new(channel)
            .send_message(&http, answer_message(part))
            .await
        {
            let mut message = discord_reason(&error);
            let lacking = cache
                .as_ref()
                .and_then(|cache| cached_permissions(cache, guild, channel))
                .map(missing)
                .unwrap_or_default();
            if !lacking.is_empty() {
                message.push_str(&format!("; the bot lacks {} here", lacking.join(", ")));
            }
            return PostOutcome::Refused {
                channel: name,
                server,
                message,
            };
        }
    }
    PostOutcome::Posted {
        channel: name,
        server,
        messages: parts.len(),
        characters: text.chars().count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VOICE: u64 = 11;
    const COMMAND: u64 = 12;
    const CHOSEN: u64 = 13;
    const A: u64 = 1;
    const B: u64 = 2;

    fn in_voice() -> Option<SessionChannels> {
        Some(SessionChannels {
            voice: VOICE,
            command: COMMAND,
        })
    }

    #[test]
    fn every_row_of_the_destination_table_resolves() {
        use AnswerDestination::*;
        // voice_channel: the session's voice channel; after the leave nothing.
        assert_eq!(resolve(VoiceChannel, A, in_voice(), None), Ok(VOICE));
        assert_eq!(resolve(VoiceChannel, A, None, None), Err(Unresolved::Left));
        // command_channel: the join's channel; after the leave nothing.
        assert_eq!(resolve(CommandChannel, A, in_voice(), None), Ok(COMMAND));
        assert_eq!(
            resolve(CommandChannel, A, None, None),
            Err(Unresolved::Left)
        );
        // /join typed in the voice channel's own chat: the same channel.
        let same = Some(SessionChannels {
            voice: VOICE,
            command: VOICE,
        });
        assert_eq!(resolve(CommandChannel, A, same, None), Ok(VOICE));
        // channel: its own server, in a voice session or after the leave.
        assert_eq!(resolve(Channel(CHOSEN), A, in_voice(), Some(A)), Ok(CHOSEN));
        assert_eq!(resolve(Channel(CHOSEN), A, None, Some(A)), Ok(CHOSEN));
        // channel of another server: nothing, in either server.
        assert_eq!(
            resolve(Channel(CHOSEN), B, in_voice(), Some(A)),
            Err(Unresolved::OtherServer { channel_guild: A })
        );
        // channel the cache does not know.
        assert_eq!(
            resolve(Channel(CHOSEN), A, in_voice(), None),
            Err(Unresolved::Gone)
        );
    }

    /// A paragraph of exactly `length` characters.
    fn paragraph(index: usize, length: usize) -> String {
        let mut text = format!("Paragraph {index:02} says");
        while text.len() < length - 1 {
            text.push_str(" x");
        }
        text.truncate(length - 1);
        format!("{}.", text.trim_end())
    }

    fn balanced(part: &str) -> bool {
        part.lines().filter(|line| toggles_fence(line)).count() % 2 == 0
    }

    #[test]
    fn paragraphs_are_cut_at_blank_lines() {
        let paragraphs: Vec<String> = (0..16).map(|i| paragraph(i, 280)).collect();
        let text = paragraphs.join("\n\n");
        assert!(text.len() >= 4500, "{}", text.len());
        let parts = split_for_discord(&text, MESSAGE_LIMIT);
        assert_eq!(parts.len(), 3);
        for part in &parts {
            assert!(units(part) <= MESSAGE_LIMIT);
            assert!(
                paragraphs.iter().any(|p| part.ends_with(p.as_str())),
                "each part ends at the end of a paragraph: …{}",
                &part[part.len() - 20..]
            );
        }
        assert_eq!(parts.join("\n\n"), text, "nothing is lost or changed");
    }

    #[test]
    fn one_long_line_is_cut_at_spaces() {
        let line = "abcdefghi ".repeat(500);
        let line = line.trim_end();
        assert_eq!(line.len(), 4999);
        let parts = split_for_discord(line, MESSAGE_LIMIT);
        assert_eq!(parts.len(), 3);
        for part in &parts {
            assert!(units(part) <= MESSAGE_LIMIT);
            assert!(part.ends_with("abcdefghi") && part.starts_with("abcdefghi"));
        }
        assert_eq!(parts.join(" "), line);
    }

    #[test]
    fn a_long_code_block_is_closed_and_reopened_with_its_language() {
        let code: Vec<String> = (0..80)
            .map(|i| format!("let value_{i:03} = compute({i}); // the step"))
            .collect();
        let block = format!("```rust\n{}\n```", code.join("\n"));
        assert!(block.len() > 2500 && block.len() < 4000, "{}", block.len());
        let text = format!("Here is the code:\n\n{block}\n\nThat is all.");
        let parts = split_for_discord(&text, MESSAGE_LIMIT);
        assert_eq!(parts.len(), 2, "{parts:#?}");
        for (index, part) in parts.iter().enumerate() {
            assert!(
                units(part) <= MESSAGE_LIMIT,
                "part {index}: {}",
                units(part)
            );
            assert!(
                balanced(part),
                "part {index} opens and closes its block:\n{part}"
            );
        }
        assert!(parts[0].starts_with("Here is the code:\n\n```rust\nlet value_000"));
        assert!(parts[0].ends_with("\n```"));
        assert!(
            parts[1].starts_with("```rust\nlet value_"),
            "{:.40}",
            parts[1]
        );
        assert!(parts[1].ends_with("```\n\nThat is all."));
        // Every line of code is there once, in order.
        let lines: Vec<&str> = parts
            .iter()
            .flat_map(|part| part.lines())
            .filter(|line| line.starts_with("let value_"))
            .collect();
        assert_eq!(lines, code.iter().map(String::as_str).collect::<Vec<_>>());
    }

    #[test]
    fn a_short_code_block_is_not_cut() {
        let block = format!("```\n{}\n```", "x = 1\n".repeat(50).trim_end());
        let text = format!("{}\n{block}\nend", "intro ".repeat(300).trim_end());
        let parts = split_for_discord(&text, MESSAGE_LIMIT);
        assert_eq!(parts.len(), 2);
        assert!(parts[1].starts_with("```\nx = 1"), "{:.40}", parts[1]);
        assert!(parts.iter().all(|part| balanced(part)));
    }

    #[test]
    fn a_part_of_exactly_the_limit_stays_whole() {
        let text = "a".repeat(MESSAGE_LIMIT);
        assert_eq!(split_for_discord(&text, MESSAGE_LIMIT), vec![text.clone()]);
        let over = format!("{text}b");
        let parts = split_for_discord(&over, MESSAGE_LIMIT);
        assert_eq!(
            parts,
            vec![text, "b".to_owned()],
            "no space: cut at the limit"
        );
        let words = format!("{} {}", "a".repeat(999), "b".repeat(1000));
        assert_eq!(units(&words), MESSAGE_LIMIT);
        assert_eq!(
            split_for_discord(&words, MESSAGE_LIMIT),
            vec![words.clone()]
        );
        // A character beyond the BMP counts twice.
        let wide = "😀".repeat(1001);
        let parts = split_for_discord(&wide, MESSAGE_LIMIT);
        assert_eq!(parts.len(), 2);
        assert!(parts.iter().all(|part| units(part) <= MESSAGE_LIMIT));
    }

    #[test]
    fn the_task_line_is_small_and_cut_at_150_characters() {
        let text = format_answer(
            Some("find the weather in Bangkok tomorrow"),
            "**Sunny**, 33 °C.",
        );
        assert_eq!(
            text,
            "-# Codex: find the weather in Bangkok tomorrow\n**Sunny**, 33 °C."
        );
        assert_eq!(format_answer(None, "Hi"), "-# Codex\nHi");
        assert_eq!(format_answer(Some("  "), "Hi"), "-# Codex\nHi");
        let long = "ask ".repeat(100);
        let first = format_answer(Some(&long), "x");
        let line = first.lines().next().unwrap();
        let task = line.strip_prefix("-# Codex: ").unwrap();
        assert!(task.chars().count() <= TASK_LINE_LIMIT, "{task}");
        assert!(task.ends_with('…') && task.starts_with("ask ask"));
        let multiline = format_answer(Some("one\ntwo"), "x");
        assert!(multiline.starts_with("-# Codex: one two\n"), "{multiline}");
    }

    #[test]
    fn a_table_is_put_into_a_code_block_and_the_rest_is_unchanged() {
        let answer = "Forecast:\n\n| Day | High | Sky |\n|---|:---:|---|\n| Mon | 33 | sun |\n| Tuesday | 31 | rain |\n\nSee [the source](https://example.com).";
        assert_eq!(
            wrap_tables(answer),
            "Forecast:\n\n```\n| Day     | High | Sky  |\n| ------- | ---- | ---- |\n| Mon     | 33   | sun  |\n| Tuesday | 31   | rain |\n```\n\nSee [the source](https://example.com)."
        );
        // A table inside a code block, and a line with a pipe but no separator, stay.
        let code = "```\n| a | b |\n|---|---|\n```\nx | y";
        assert_eq!(wrap_tables(code), code);
        let posted = format_answer(Some("weather"), answer);
        assert!(posted.contains("\n```\n| Day "), "{posted}");
    }

    #[test]
    fn the_message_allows_no_mentions() {
        let body = serde_json::to_value(answer_message("@everyone <@1> <@&2>")).unwrap();
        assert_eq!(body["content"], "@everyone <@1> <@&2>");
        let mentions = &body["allowed_mentions"];
        assert_eq!(mentions["parse"], serde_json::json!([]), "{body}");
        assert_eq!(mentions["users"], serde_json::json!([]));
        assert_eq!(mentions["roles"], serde_json::json!([]));
        assert!(
            body.get("flags").is_none(),
            "link previews are not suppressed"
        );
    }

    fn overwrite(
        kind: PermissionOverwriteType,
        allow: Permissions,
        deny: Permissions,
    ) -> PermissionOverwrite {
        PermissionOverwrite { allow, deny, kind }
    }

    #[test]
    fn can_post_follows_roles_and_overwrites() {
        let guild = 100;
        let everyone = RoleId::new(guild);
        let bot_role = RoleId::new(200);
        let bot = UserId::new(300);
        let three =
            Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES | Permissions::EMBED_LINKS;
        let roles: HashMap<RoleId, Permissions> =
            [(everyone, Permissions::VIEW_CHANNEL), (bot_role, three)].into();
        let member_roles = [bot_role];
        let member = Some((bot, &member_roles[..]));
        // A role allowing all three.
        assert_eq!(can_post(guild, 1, &roles, &[], member), Some(true));
        // An overwrite denying Send Messages to the bot's role.
        let deny_send = [overwrite(
            PermissionOverwriteType::Role(bot_role),
            Permissions::empty(),
            Permissions::SEND_MESSAGES,
        )];
        assert_eq!(can_post(guild, 1, &roles, &deny_send, member), Some(false));
        let permissions = channel_permissions(guild, 1, &roles, &deny_send, member).unwrap();
        assert_eq!(missing(permissions), vec!["Send Messages"]);
        // Denied to @everyone, allowed again to the bot's role: allowed.
        let private = [
            overwrite(
                PermissionOverwriteType::Role(everyone),
                Permissions::empty(),
                Permissions::VIEW_CHANNEL,
            ),
            overwrite(
                PermissionOverwriteType::Role(bot_role),
                Permissions::VIEW_CHANNEL,
                Permissions::empty(),
            ),
        ];
        assert_eq!(can_post(guild, 1, &roles, &private, member), Some(true));
        // A member overwrite comes last.
        let member_deny = [overwrite(
            PermissionOverwriteType::Member(bot),
            Permissions::empty(),
            Permissions::EMBED_LINKS,
        )];
        assert_eq!(
            can_post(guild, 1, &roles, &member_deny, member),
            Some(false)
        );
        // Roles without Embed Links.
        let weak: HashMap<RoleId, Permissions> = [(
            everyone,
            Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES,
        )]
        .into();
        assert_eq!(
            can_post(guild, 1, &weak, &[], Some((bot, &[]))),
            Some(false)
        );
        // Administrator and the owner pass every overwrite.
        let admin: HashMap<RoleId, Permissions> = [(bot_role, Permissions::ADMINISTRATOR)].into();
        assert_eq!(can_post(guild, 1, &admin, &deny_send, member), Some(true));
        assert_eq!(can_post(guild, 300, &weak, &deny_send, member), Some(true));
        // No member in the cache: nothing can be said.
        assert_eq!(can_post(guild, 1, &roles, &[], None), None);
    }
}
