//! Continuity across calls and the words that tell the model who is in the channel.
//!
//! Every call that follows a deactivation continues the same conversation: same thread,
//! same voice session id, the last realtime session id, and the transcript so far as
//! initial items. The user's instructions and the call context go last, because the probe
//! showed the model follows the last developer item.

use super::thread::ThreadInfo;
use crate::settings::SpeakerLabels;
use serde_json::{json, Value};
use std::collections::VecDeque;

/// History kept for the initial items of the next call: at most this many items.
pub const HISTORY_MAX_ITEMS: usize = 100;
/// ... and this many bytes. Codex limits initial items to 8 192 tokens in total; the
/// instructions and the names also go there.
pub const HISTORY_MAX_BYTES: usize = 24_000;
pub const HISTORY_HINT: &str = "Below is the beginning of this conversation before a pause. Continue it as one conversation and do not greet again.";

/// Who the call is with: the person who said the activation phrase and everyone in the
/// voice channel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallContext {
    pub caller: Option<String>,
    pub participants: Vec<String>,
}

impl CallContext {
    /// One line for the chat log.
    pub fn summary(&self) -> Option<String> {
        let mut parts = Vec::new();
        if !self.participants.is_empty() {
            parts.push(format!("In the channel: {}.", self.participants.join(", ")));
        }
        if let Some(caller) = self.caller.as_ref() {
            parts.push(format!("Addressed by: {caller}."));
        }
        (!parts.is_empty()).then(|| parts.join(" "))
    }
}

/// The call instructions: the user's instructions plus who is in the call.
pub fn call_instructions(base: &str, labels: SpeakerLabels, call: &CallContext) -> String {
    let mut lines = Vec::new();
    if labels != SpeakerLabels::Off {
        if !call.participants.is_empty() {
            lines.push(format!(
                "You are in a Discord voice channel and everyone there hears you. In the channel now: {}.",
                call.participants.join(", ")
            ));
        }
        if let Some(caller) = call.caller.as_ref() {
            lines.push(format!(
                "{caller} has just addressed you: answer them and use their name when it fits."
            ));
        }
        if !lines.is_empty() {
            lines.push(
                "These are Discord display names; do not read the list aloud and do not verify it."
                    .to_owned(),
            );
        }
        if labels == SpeakerLabels::Changes {
            lines.push("When a different person starts talking, a service note \"now speaking: name\" arrives in the context. It is not a question or a task: do not answer it, do not verify anything and do not read it aloud; just keep in mind who is speaking.".to_owned());
        }
    }
    if lines.is_empty() {
        return base.trim().to_owned();
    }
    if base.trim().is_empty() {
        return lines.join(" ");
    }
    format!("{}\n\n{}", base.trim_end(), lines.join(" "))
}

/// Who is in the call, as one developer note for the start of the call (`None` when
/// labels are off or nobody is known).
pub fn call_names_note(labels: SpeakerLabels, call: &CallContext) -> Option<String> {
    let text = call_instructions("", labels, call);
    (!text.trim().is_empty()).then(|| text.trim().to_owned())
}

/// The mid-call note sent when someone else starts talking (`changes` mode only).
pub fn speaker_note(names: &[String]) -> Option<String> {
    let names = match names {
        [] => return None,
        [name] => format!("now speaking: {name}"),
        names => format!("now speaking: {}", names.join(", ")),
    };
    Some(format!(
        "Service note, do not answer it and do not verify anything: {names}."
    ))
}

/// What one guild's voice conversation remembers while the app runs.
pub struct Conversation {
    pub thread: Option<ThreadInfo>,
    pub voice_session_id: String,
    pub realtime_session_id: Option<String>,
    items: VecDeque<(&'static str, String)>,
    /// The names of the people last heard, prefixed to user transcripts in the history.
    pub speakers: Option<String>,
}

impl Conversation {
    pub fn new(guild: u64) -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        Self {
            thread: None,
            voice_session_id: format!("voicebro-{guild}-{stamp:x}"),
            realtime_session_id: None,
            items: VecDeque::new(),
            speakers: None,
        }
    }

    pub fn push(&mut self, role: &'static str, text: String) {
        self.items.push_back((role, text));
        let mut bytes = self.items.iter().map(|(_, text)| text.len()).sum::<usize>();
        while self.items.len() > HISTORY_MAX_ITEMS || bytes > HISTORY_MAX_BYTES {
            let Some((_, removed)) = self.items.pop_front() else {
                break;
            };
            bytes -= removed.len();
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn bytes(&self) -> usize {
        self.items.iter().map(|(_, text)| text.len()).sum()
    }

    pub fn last_text(&self) -> Option<&str> {
        self.items.back().map(|(_, text)| text.as_str())
    }

    /// Initial items for the next call: the transcript so far, then the notes for this
    /// call (instructions and who is in it), which come last so they are the freshest.
    pub fn initial_items(&self, notes: &[String]) -> Vec<Value> {
        let mut items = Vec::new();
        if !self.items.is_empty() {
            items.push(json!({ "role": "developer", "text": HISTORY_HINT }));
            items.extend(
                self.items
                    .iter()
                    .map(|(role, text)| json!({ "role": role, "text": text })),
            );
        }
        items.extend(
            notes
                .iter()
                .map(|text| json!({ "role": "developer", "text": text })),
        );
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_is_bounded_and_notes_come_last() {
        let mut conversation = Conversation::new(7);
        assert!(conversation.voice_session_id.starts_with("voicebro-7-"));
        assert!(conversation.initial_items(&[]).is_empty());
        assert_eq!(
            conversation.initial_items(&["note".to_owned()]),
            vec![json!({ "role": "developer", "text": "note" })]
        );
        conversation.push("user", "Alex: remember the number forty two".to_owned());
        conversation.push("assistant", "Noted.".to_owned());
        let items = conversation.initial_items(&["names".to_owned(), "instructions".to_owned()]);
        assert_eq!(items.len(), 5);
        assert_eq!(items[0]["text"], HISTORY_HINT);
        assert_eq!(
            items[1],
            json!({ "role": "user", "text": "Alex: remember the number forty two" })
        );
        assert_eq!(items[2]["role"], "assistant");
        assert_eq!(
            items[4],
            json!({ "role": "developer", "text": "instructions" })
        );
        for index in 0..500 {
            conversation.push("user", format!("{index}: {}", "word ".repeat(20)));
        }
        assert!(conversation.len() <= HISTORY_MAX_ITEMS);
        assert!(conversation.bytes() <= HISTORY_MAX_BYTES);
        assert!(conversation.last_text().unwrap().starts_with("499:"));
    }

    #[test]
    fn names_go_into_the_call_instructions_in_english() {
        let call = CallContext {
            caller: Some("Alex".to_owned()),
            participants: vec!["Alex".to_owned(), "Bob".to_owned()],
        };
        let at_start = call_instructions("Base.", SpeakerLabels::Start, &call);
        assert!(at_start.starts_with("Base.\n\n"));
        assert!(at_start.contains("In the channel now: Alex, Bob."));
        assert!(at_start.contains("Alex has just addressed you"));
        assert!(!at_start.contains("service note"));
        let on_change = call_instructions("Base.", SpeakerLabels::Changes, &call);
        assert!(on_change.contains("service note"));
        assert_eq!(
            call_instructions("Base.", SpeakerLabels::Off, &call),
            "Base."
        );
        assert_eq!(
            call_instructions("Base.", SpeakerLabels::Start, &CallContext::default()),
            "Base."
        );
        assert_eq!(
            call.summary().as_deref(),
            Some("In the channel: Alex, Bob. Addressed by: Alex.")
        );
        assert!(call_names_note(SpeakerLabels::Start, &call)
            .unwrap()
            .starts_with("You are in a Discord voice channel"));
        assert_eq!(call_names_note(SpeakerLabels::Off, &call), None);
        assert_eq!(speaker_note(&[]), None);
        assert_eq!(
            speaker_note(&["Alex".to_owned()]).as_deref(),
            Some("Service note, do not answer it and do not verify anything: now speaking: Alex.")
        );
    }
}
