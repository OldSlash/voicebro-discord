//! Whose voice reaches the wake word and the assistant (change `voice-consent`, design D4):
//! one filter for the three access modes, rebuilt whenever the mode, the owner link or the
//! consent records change, and read by the receive path once per tick.
//!
//! The filter is replaced whole (`Arc`), so a tick clones one pointer and holds no lock while
//! it mixes. The owner and the consent records call [`AccessState::refresh`] after each of
//! their changes, so a link, an unlink, an `/optin` or an `/optout` applies from the next
//! frame.

use crate::consent::ConsentState;
use crate::owner::OwnerState;
use crate::settings::AccessMode;
use std::collections::HashSet;
use std::sync::{Arc, Mutex, RwLock};

/// What the receive path lets through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceFilter {
    /// Every source: the `everyone` mode.
    Everyone,
    /// Only sources whose Discord account is in the set: the owner (`owner`), or the owner
    /// and everyone recorded (`agreed`). An empty set lets nobody through.
    Only(HashSet<u64>),
}

impl VoiceFilter {
    /// Whether a source with this account passes; a source with no known account passes
    /// only [`VoiceFilter::Everyone`].
    pub fn passes(&self, user: Option<u64>) -> bool {
        match self {
            VoiceFilter::Everyone => true,
            VoiceFilter::Only(ids) => user.is_some_and(|id| ids.contains(&id)),
        }
    }

    pub fn is_everyone(&self) -> bool {
        matches!(self, VoiceFilter::Everyone)
    }
}

/// The access mode with the owner and the records it reads.
pub struct AccessState {
    mode: Mutex<AccessMode>,
    owner: Arc<OwnerState>,
    consent: Arc<ConsentState>,
    filter: RwLock<Arc<VoiceFilter>>,
    /// Called after every rebuild of the filter (the participant names, design D6). They
    /// must not change the mode, the link or the records.
    observers: Mutex<Vec<Box<dyn Fn() + Send + Sync>>>,
}

impl AccessState {
    /// Starts in the `everyone` mode and registers itself with the owner and the records.
    pub fn new(owner: Arc<OwnerState>, consent: Arc<ConsentState>) -> Arc<Self> {
        let state = Arc::new(Self {
            mode: Mutex::new(AccessMode::Everyone),
            owner: Arc::clone(&owner),
            consent: Arc::clone(&consent),
            filter: RwLock::new(Arc::new(VoiceFilter::Everyone)),
            observers: Mutex::new(Vec::new()),
        });
        let weak = Arc::downgrade(&state);
        owner.set_listener(Box::new(move || {
            if let Some(state) = weak.upgrade() {
                state.refresh();
            }
        }));
        let weak = Arc::downgrade(&state);
        consent.set_listener(Box::new(move || {
            if let Some(state) = weak.upgrade() {
                state.refresh();
            }
        }));
        state.refresh();
        state
    }

    pub fn mode(&self) -> AccessMode {
        *self.mode.lock().expect("access mode lock")
    }

    /// `discord.access` changed (or was read at start).
    pub fn set_mode(&self, mode: AccessMode) {
        *self.mode.lock().expect("access mode lock") = mode;
        // The owner state keeps its own owner-only flag for its status.
        self.owner.set_mode(mode == AccessMode::Owner);
        self.refresh();
    }

    /// The filter of the next tick.
    pub fn filter(&self) -> Arc<VoiceFilter> {
        Arc::clone(&self.filter.read().expect("access filter lock"))
    }

    /// Rebuilds the filter from the mode, the link and the records. Reads only lock-free or
    /// separately locked parts of the owner and the records, so it may run from their
    /// listeners.
    pub fn refresh(&self) {
        let filter = match self.mode() {
            AccessMode::Everyone => VoiceFilter::Everyone,
            AccessMode::Owner => VoiceFilter::Only(self.owner.linked_id().into_iter().collect()),
            AccessMode::Agreed => {
                let mut ids = (*self.consent.ids()).clone();
                ids.extend(self.owner.linked_id());
                VoiceFilter::Only(ids)
            }
        };
        *self.filter.write().expect("access filter lock") = Arc::new(filter);
        for observer in self.observers.lock().expect("access observers lock").iter() {
            observer();
        }
    }

    /// Adds a function called after every rebuild of the filter.
    pub fn observe(&self, observer: Box<dyn Fn() + Send + Sync>) {
        self.observers
            .lock()
            .expect("access observers lock")
            .push(observer);
    }

    /// The names of `people` (account id, name) whose voice the current filter lets
    /// through, in their order (design D6).
    pub fn passing_names(&self, people: &[(u64, String)]) -> Vec<String> {
        let filter = self.filter();
        people
            .iter()
            .filter(|(id, _)| filter.passes(Some(*id)))
            .map(|(_, name)| name.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consent::{MemoryConsentStore, CONSENT_TEXT_VERSION};
    use crate::events::EventBus;
    use crate::owner::{MemoryLinkStore, OwnerLink};

    fn state(owner_id: Option<u64>) -> (Arc<AccessState>, Arc<ConsentState>, Arc<OwnerState>) {
        let links = Arc::new(MemoryLinkStore::default());
        if let Some(id) = owner_id {
            use crate::owner::LinkStore;
            links
                .save(&OwnerLink {
                    version: 1,
                    discord_user_id: id,
                    discord_name: "Owner".to_owned(),
                    assistant: "codex".to_owned(),
                    account: "x".to_owned(),
                    linked_at: "t".to_owned(),
                })
                .unwrap();
        }
        let owner = Arc::new(OwnerState::new(links, EventBus::default()));
        let consent = Arc::new(ConsentState::new(
            Arc::new(MemoryConsentStore::default()),
            EventBus::default(),
            "codex",
        ));
        let access = AccessState::new(Arc::clone(&owner), Arc::clone(&consent));
        (access, consent, owner)
    }

    fn only(ids: &[u64]) -> VoiceFilter {
        VoiceFilter::Only(ids.iter().copied().collect())
    }

    #[test]
    fn each_mode_gives_its_filter_and_follows_every_change() {
        let (access, consent, owner) = state(Some(100));
        assert_eq!(*access.filter(), VoiceFilter::Everyone);
        access.set_mode(AccessMode::Owner);
        assert_eq!(*access.filter(), only(&[100]));
        access.set_mode(AccessMode::Agreed);
        assert_eq!(*access.filter(), only(&[100]), "the owner without a record");
        consent.agree(7, "Alex", CONSENT_TEXT_VERSION, "t");
        assert_eq!(
            *access.filter(),
            only(&[100, 7]),
            "an /optin applies at once"
        );
        consent.withdraw(7, false);
        assert_eq!(*access.filter(), only(&[100]));
        owner.unlink().unwrap();
        assert_eq!(*access.filter(), only(&[]), "nobody");
        access.set_mode(AccessMode::Owner);
        assert_eq!(
            *access.filter(),
            only(&[]),
            "owner mode without a link: nobody"
        );
        access.set_mode(AccessMode::Everyone);
        assert!(access.filter().is_everyone());
    }

    #[test]
    fn only_known_accounts_pass_a_limited_filter() {
        let filter = only(&[7]);
        assert!(filter.passes(Some(7)));
        assert!(!filter.passes(Some(8)));
        assert!(!filter.passes(None), "an unknown source");
        assert!(VoiceFilter::Everyone.passes(None));
        assert!(!only(&[]).passes(Some(7)));
    }

    #[test]
    fn names_follow_the_filter() {
        let (access, consent, _) = state(None);
        let people = vec![(7, "Alex".to_owned()), (8, "Sam".to_owned())];
        assert_eq!(access.passing_names(&people), ["Alex", "Sam"]);
        access.set_mode(AccessMode::Agreed);
        assert!(access.passing_names(&people).is_empty());
        consent.agree(7, "Alex", CONSENT_TEXT_VERSION, "t");
        assert_eq!(access.passing_names(&people), ["Alex"]);
    }
}
