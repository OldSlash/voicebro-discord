//! Who is speaking: audio source id to Discord account and display name, per guild,
//! outliving the call.

use serenity::all::{Cache, GuildId, Http, UserId};
use std::collections::HashMap;
use std::sync::Mutex;

/// What is known about one audio source (SSRC). A source Discord announced has an entry
/// with its account, bots included; only people have a name (change `owner-only-access`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpeakerEntry {
    pub user: Option<u64>,
    pub bot: bool,
    pub name: Option<String>,
}

/// Accounts and display names by Discord audio source (SSRC) for one guild.
#[derive(Default)]
pub struct SpeakerDirectory {
    entries: Mutex<HashMap<u32, SpeakerEntry>>,
}

impl SpeakerDirectory {
    pub fn name(&self, ssrc: u32) -> Option<String> {
        self.lock().get(&ssrc).and_then(|entry| entry.name.clone())
    }

    /// The Discord account the source belongs to, when known.
    pub fn user(&self, ssrc: u32) -> Option<u64> {
        self.lock().get(&ssrc).and_then(|entry| entry.user)
    }

    /// Whether the source has an entry at all: Discord announced it, or the "only person in
    /// the channel" rule named it.
    pub fn knows(&self, ssrc: u32) -> bool {
        self.lock().contains_key(&ssrc)
    }

    /// Records the account a speaking event names, before its member is looked up, so the
    /// owner filter knows the source from the first frame; a name already known stays.
    pub fn set_user(&self, ssrc: u32, user: u64) {
        let mut entries = self.lock();
        let entry = entries.entry(ssrc).or_default();
        if entry.user != Some(user) {
            *entry = SpeakerEntry {
                user: Some(user),
                bot: false,
                name: None,
            };
        }
    }

    /// Stores what is known about a source; returns `true` when it now has a name that is
    /// new or changed.
    pub fn remember(&self, ssrc: u32, entry: SpeakerEntry) -> bool {
        let mut entries = self.lock();
        let old_name = entries.get(&ssrc).and_then(|old| old.name.as_ref());
        let name_changed = entry.name.is_some() && old_name != entry.name.as_ref();
        entries.insert(ssrc, entry);
        name_changed
    }

    /// Display names of the sources that have one, sorted by source.
    pub fn snapshot(&self) -> Vec<(u32, String)> {
        let entries = self.lock();
        let mut all: Vec<(u32, String)> = entries
            .iter()
            .filter_map(|(ssrc, entry)| entry.name.clone().map(|name| (*ssrc, name)))
            .collect();
        all.sort();
        all
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u32, SpeakerEntry>> {
        self.entries.lock().expect("speaker directory lock")
    }
}

/// The person to assume for an audio source Discord never announced: the only person in the
/// channel, when there is exactly one. `people` are the non-bot members in the bot's
/// channel with their account ids.
pub fn sole_person(people: &[(u64, String)]) -> Option<&(u64, String)> {
    match people {
        [only] => Some(only),
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

/// What a member's speaking event tells: their label (server nickname, then display name,
/// then user name; `None` for a bot or a name with nothing printable) and whether the
/// account is a bot. `None` when the member cannot be resolved.
pub async fn member_info(
    http: &Http,
    cache: &Cache,
    guild: GuildId,
    user: UserId,
) -> Option<(Option<String>, bool)> {
    let cached = cache
        .guild(guild)
        .and_then(|guild| guild.members.get(&user).cloned());
    let member = match cached {
        Some(member) => member,
        None => guild.member(http, user).await.ok()?,
    };
    if member.user.bot {
        return Some((None, true));
    }
    let name =
        clean_display_name(member.display_name()).or_else(|| clean_display_name(&member.user.name));
    Some((name, false))
}

/// Server nickname, then display name, then user name; `None` for bots or unresolvable
/// members.
pub async fn member_label(
    http: &Http,
    cache: &Cache,
    guild: GuildId,
    user: UserId,
) -> Option<String> {
    member_info(http, cache, guild, user)
        .await
        .and_then(|(name, _)| name)
}

/// Non-bot members in `channel` with their account ids, sorted by display name. The cache
/// gives the user ids; names come from the cache or HTTP.
pub async fn channel_people(
    http: &Http,
    cache: &Cache,
    guild: GuildId,
    channel: serenity::all::ChannelId,
) -> Vec<(u64, String)> {
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
    let mut people = Vec::new();
    for user in users {
        if let Some(name) = member_label(http, cache, guild, user).await {
            people.push((user.get(), name));
        }
    }
    people.sort_by_key(|(id, name)| (name.to_lowercase(), *id));
    people.dedup_by_key(|(id, _)| *id);
    people
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(user: u64, name: &str) -> SpeakerEntry {
        SpeakerEntry {
            user: Some(user),
            bot: false,
            name: Some(name.to_owned()),
        }
    }

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
        let alex = (7, "Alex".to_owned());
        assert_eq!(sole_person(std::slice::from_ref(&alex)), Some(&alex));
        assert_eq!(sole_person(&[alex.clone(), (8, "Sam".to_owned())]), None);
        assert_eq!(sole_person(&[]), None);
        let directory = SpeakerDirectory::default();
        let (id, name) = sole_person(std::slice::from_ref(&alex)).unwrap();
        assert!(directory.remember(5, person(*id, name)));
        assert_eq!(
            directory.user(5),
            Some(7),
            "the account comes with the name"
        );
        assert!(
            directory.remember(5, person(8, "Sam")),
            "the real mapping replaces it"
        );
        assert_eq!(directory.name(5).as_deref(), Some("Sam"));
        assert_eq!(directory.user(5), Some(8));
    }

    /// Change `owner-only-access`, scenario "A bot's stream": a bot's source has an entry
    /// with its account and no name, so the "only person" rule never names it.
    #[test]
    fn a_bots_source_is_known_and_has_no_name() {
        let directory = SpeakerDirectory::default();
        assert!(!directory.knows(9));
        let renamed = directory.remember(
            9,
            SpeakerEntry {
                user: Some(4242),
                bot: true,
                name: None,
            },
        );
        assert!(!renamed, "no name to report");
        assert!(directory.knows(9), "the rule skips a known source");
        assert_eq!(directory.user(9), Some(4242));
        assert_eq!(directory.name(9), None);
        assert!(directory.snapshot().is_empty(), "a bot has no label");
    }

    /// The account of a speaking event is there before its name; a known name stays, and
    /// another account on the same source starts over.
    #[test]
    fn the_account_is_known_before_the_name() {
        let directory = SpeakerDirectory::default();
        directory.set_user(3, 10);
        assert!(directory.knows(3));
        assert_eq!((directory.user(3), directory.name(3)), (Some(10), None));
        assert!(
            directory.remember(3, person(10, "Alex")),
            "the name arrives"
        );
        directory.set_user(3, 10);
        assert_eq!(directory.name(3).as_deref(), Some("Alex"), "the name stays");
        directory.set_user(3, 11);
        assert_eq!((directory.user(3), directory.name(3)), (Some(11), None));
    }

    #[test]
    fn directory_reports_new_and_changed_names() {
        let directory = SpeakerDirectory::default();
        assert!(directory.remember(1, person(10, "Alex")));
        assert!(!directory.remember(1, person(10, "Alex")));
        assert!(directory.remember(1, person(10, "Sam")));
        assert_eq!(directory.name(1).as_deref(), Some("Sam"));
        assert_eq!(directory.snapshot(), vec![(1, "Sam".to_owned())]);
    }
}
