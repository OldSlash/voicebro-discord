//! Who is speaking: audio source id to display name, per guild, outliving the call.

use serenity::all::{Cache, GuildId, Http, UserId};
use std::collections::HashMap;
use std::sync::Mutex;

/// Display names by Discord audio source (SSRC) for one guild.
#[derive(Default)]
pub struct SpeakerDirectory {
    names: Mutex<HashMap<u32, String>>,
}

impl SpeakerDirectory {
    pub fn name(&self, ssrc: u32) -> Option<String> {
        self.names
            .lock()
            .expect("speaker directory lock")
            .get(&ssrc)
            .cloned()
    }

    /// Stores a name; returns `true` when it is new or changed.
    pub fn remember(&self, ssrc: u32, name: String) -> bool {
        let mut names = self.names.lock().expect("speaker directory lock");
        match names.get(&ssrc) {
            Some(existing) if *existing == name => false,
            _ => {
                names.insert(ssrc, name);
                true
            }
        }
    }

    pub fn snapshot(&self) -> Vec<(u32, String)> {
        let names = self.names.lock().expect("speaker directory lock");
        let mut all: Vec<(u32, String)> = names.iter().map(|(k, v)| (*k, v.clone())).collect();
        all.sort();
        all
    }
}

/// The name to assume for an audio source Discord did not name: the only person in the
/// channel, when there is exactly one (the first version's fallback). `participants` are
/// the non-bot members in the bot's channel.
pub fn sole_person(participants: &[String]) -> Option<&str> {
    match participants {
        [only] => Some(only.as_str()),
        _ => None,
    }
}

/// A display name safe to put in a label: printable, trimmed, at most 32 characters.
pub fn clean_display_name(name: &str) -> Option<String> {
    let cleaned: String = name
        .chars()
        .filter(|character| {
            character.is_alphanumeric() || matches!(character, ' ' | '_' | '-' | '.')
        })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned: String = cleaned.chars().take(32).collect();
    (!cleaned.is_empty()).then_some(cleaned)
}

/// Server nickname, then display name, then user name; `None` for bots or unresolvable
/// members.
pub async fn member_label(
    http: &Http,
    cache: &Cache,
    guild: GuildId,
    user: UserId,
) -> Option<String> {
    let cached = cache
        .guild(guild)
        .and_then(|guild| guild.members.get(&user).cloned());
    let member = match cached {
        Some(member) => member,
        None => guild.member(http, user).await.ok()?,
    };
    if member.user.bot {
        return None;
    }
    clean_display_name(member.display_name()).or_else(|| clean_display_name(&member.user.name))
}

/// Non-bot members in `channel`, by display name, sorted. The cache gives the user ids;
/// names come from the cache or HTTP.
pub async fn channel_people(
    http: &Http,
    cache: &Cache,
    guild: GuildId,
    channel: serenity::all::ChannelId,
) -> Vec<String> {
    let bot_id = cache.current_user().id;
    let users: Vec<UserId> = match cache.guild(guild) {
        Some(guild_ref) => guild_ref
            .voice_states
            .values()
            .filter(|state| state.channel_id == Some(channel) && state.user_id != bot_id)
            .map(|state| state.user_id)
            .collect(),
        None => Vec::new(),
    };
    let mut names = Vec::new();
    for user in users {
        if let Some(name) = member_label(http, cache, guild, user).await {
            names.push(name);
        }
    }
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names_are_cleaned_for_labels() {
        assert_eq!(clean_display_name("  Alex  🎮 ").as_deref(), Some("Alex"));
        assert_eq!(
            clean_display_name("Dark_Knight-99").as_deref(),
            Some("Dark_Knight-99")
        );
        assert_eq!(
            clean_display_name("Sam **bold**").as_deref(),
            Some("Sam bold")
        );
        assert_eq!(clean_display_name("🔥🔥"), None);
        assert_eq!(
            clean_display_name(&"x".repeat(50)).map(|name| name.len()),
            Some(32)
        );
    }

    #[test]
    fn an_unnamed_source_is_the_only_person_and_a_real_name_wins_later() {
        assert_eq!(sole_person(&["Alex".to_owned()]), Some("Alex"));
        assert_eq!(sole_person(&["Alex".to_owned(), "Sam".to_owned()]), None);
        assert_eq!(sole_person(&[]), None);
        let directory = SpeakerDirectory::default();
        assert!(directory.remember(5, sole_person(&["Alex".to_owned()]).unwrap().to_owned()));
        assert!(
            directory.remember(5, "Sam".to_owned()),
            "the real mapping replaces it"
        );
        assert_eq!(directory.name(5).as_deref(), Some("Sam"));
    }

    #[test]
    fn directory_reports_new_and_changed_names() {
        let directory = SpeakerDirectory::default();
        assert!(directory.remember(1, "Alex".to_owned()));
        assert!(!directory.remember(1, "Alex".to_owned()));
        assert!(directory.remember(1, "Sam".to_owned()));
        assert_eq!(directory.name(1).as_deref(), Some("Sam"));
        assert_eq!(directory.snapshot(), vec![(1, "Sam".to_owned())]);
    }
}
