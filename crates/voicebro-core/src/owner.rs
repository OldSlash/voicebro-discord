//! The owner link (change `owner-only-access`): which Discord account owns this VoiceBro,
//! the one-time code that links it, and the voice filter value the receive path reads.
//!
//! The owner is set only by a code shown in the window and sent back from Discord with
//! `/link`, so the account id is the one Discord put in the interaction. The link lives in
//! `owner-link.bin` in the app's folder, encrypted with DPAPI for the current Windows user
//! ([`FileLinkStore`]); it never goes into `settings.json`. The mode itself
//! (`discord.owner_only`) is a setting; the commands are never limited, only the voice.

use crate::events::{AppEvent, EventBus};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Characters in a code.
pub const CODE_LEN: usize = 8;
/// How long a code waits for `/link`.
pub const CODE_LIFETIME: Duration = Duration::from_secs(10 * 60);
/// Wrong codes, from anyone, that end a waiting code.
pub const MAX_WRONG_TRIES: u8 = 5;
/// [`OwnerState::filter`]: the mode is off, everyone passes.
pub const FILTER_EVERYONE: u64 = 0;
/// [`OwnerState::filter`]: the mode is on and nobody is linked, nobody passes. Discord ids
/// are snowflakes and never reach this value.
pub const FILTER_NOBODY: u64 = u64::MAX;

/// Crockford base32: no `I`, `L`, `O` or `U`, so nothing reads as something else. 32
/// symbols, so a random byte modulo 32 is uniform.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const FILE_VERSION: u32 = 1;

/// The link file in the app's folder.
pub fn link_path(data_dir: &Path) -> PathBuf {
    data_dir.join("owner-link.bin")
}

/// The stored link: the Discord account and the assistant account it was made with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerLink {
    pub version: u32,
    pub discord_user_id: u64,
    /// The account's name at the moment of linking; a label only, the id decides.
    pub discord_name: String,
    /// The assistant engine the account belongs to (`codex`).
    pub assistant: String,
    /// [`account_fingerprint`] of the assistant account read when the code was issued.
    pub account: String,
    /// RFC 3339, UTC.
    pub linked_at: String,
}

/// The assistant account a link is bound to: SHA-256 of `<type>:<email>`, the email trimmed
/// and lower-cased. An API-key sign-in has no email, so its fingerprint covers the type
/// only. The email itself is never stored.
pub fn account_fingerprint(account_type: Option<&str>, email: Option<&str>) -> String {
    let kind = account_type.unwrap_or("").trim();
    let email = email.unwrap_or("").trim().to_lowercase();
    Sha256::digest(format!("{kind}:{email}").as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The linked owner as the window shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LinkedOwner {
    /// The Discord user id, as a string (JavaScript numbers lose its last digits).
    pub id: String,
    pub name: String,
    pub linked_at: String,
}

/// The code waiting for `/link`. Its `Debug` hides the code, so no `{:?}` of a status or an
/// event can write it to a log.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct PendingCode {
    /// As the window shows it: `XXXX-XXXX`.
    pub code: String,
    /// When it expires, Unix milliseconds; the window counts down to it.
    pub expires_at_ms: u64,
    pub wrong_tries: u8,
}

impl std::fmt::Debug for PendingCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingCode")
            .field("code", &"<hidden>")
            .field("expires_at_ms", &self.expires_at_ms)
            .field("wrong_tries", &self.wrong_tries)
            .finish()
    }
}

/// Everything the window needs for the Access card. `discord.owner_only` comes with the
/// settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct OwnerStatus {
    pub linked: Option<LinkedOwner>,
    pub pending: Option<PendingCode>,
    /// Why the last code ended without a link: expired, cancelled, ended after 5 tries.
    pub code_ended: Option<String>,
    /// Why there is no link although there was or could be one: the Codex account changed,
    /// the link file could not be read.
    pub link_ended: Option<String>,
}

/// What a `/link` led to; the bot's ephemeral reply is [`CompleteOutcome::reply`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompleteOutcome {
    Linked {
        name: String,
    },
    Wrong {
        tries_left: u8,
    },
    EndedAfterTries,
    Expired,
    NoCode,
    /// The code was right but the link could not be stored.
    NotStored {
        reason: String,
    },
}

impl CompleteOutcome {
    /// The reply only the sender sees.
    pub fn reply(&self) -> String {
        match self {
            CompleteOutcome::Linked { name } => format!(
                "Linked: {name} is now the owner of this VoiceBro. While \"Only my Discord account\" is on, only your voice reaches the assistant."
            ),
            CompleteOutcome::Wrong { tries_left } => format!(
                "That is not the code shown in the VoiceBro window. {tries_left} {} left.",
                if *tries_left == 1 { "try" } else { "tries" }
            ),
            CompleteOutcome::EndedAfterTries => format!(
                "That is not the code, and it was the {MAX_WRONG_TRIES}th wrong one: the code has ended. Press Link in the VoiceBro window for a new one."
            ),
            CompleteOutcome::Expired => {
                "The code expired. Press Link in the VoiceBro window for a new one.".to_owned()
            }
            CompleteOutcome::NoCode => {
                "No code is waiting. Press Link in the VoiceBro window first, then send the code it shows.".to_owned()
            }
            CompleteOutcome::NotStored { reason } => format!(
                "The code was right, but VoiceBro could not store the link ({reason}). Nothing was linked."
            ),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OwnerError {
    #[error("no random source for the code: {0}")]
    Random(String),
}

/// Where the link is kept.
pub trait LinkStore: Send + Sync {
    /// `Ok(None)` when there is no link; an error when a file is there but unreadable.
    fn load(&self) -> Result<Option<OwnerLink>, String>;
    fn save(&self, link: &OwnerLink) -> Result<(), String>;
    fn delete(&self) -> Result<(), String>;
    /// Where it is, for the log.
    fn describe(&self) -> String;
}

/// For tests and platforms without DPAPI: the link lasts as long as the process.
#[derive(Default)]
pub struct MemoryLinkStore {
    link: Mutex<Option<OwnerLink>>,
}

impl LinkStore for MemoryLinkStore {
    fn load(&self) -> Result<Option<OwnerLink>, String> {
        Ok(self.link.lock().expect("memory link lock").clone())
    }
    fn save(&self, link: &OwnerLink) -> Result<(), String> {
        *self.link.lock().expect("memory link lock") = Some(link.clone());
        Ok(())
    }
    fn delete(&self) -> Result<(), String> {
        *self.link.lock().expect("memory link lock") = None;
        Ok(())
    }
    fn describe(&self) -> String {
        "memory".to_owned()
    }
}

/// `owner-link.bin`: the link's JSON encrypted with DPAPI for the current Windows user,
/// written through `owner-link.bin.tmp` and a rename.
pub struct FileLinkStore {
    path: PathBuf,
}

impl FileLinkStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn tmp_path(&self) -> PathBuf {
        let mut name = self.path.as_os_str().to_owned();
        name.push(".tmp");
        PathBuf::from(name)
    }
}

impl LinkStore for FileLinkStore {
    fn load(&self) -> Result<Option<OwnerLink>, String> {
        let sealed = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("cannot read it: {error}")),
        };
        let plain =
            dpapi::unprotect(&sealed).map_err(|error| format!("cannot decrypt it: {error}"))?;
        let link: OwnerLink =
            serde_json::from_slice(&plain).map_err(|error| format!("not a link: {error}"))?;
        if link.version != FILE_VERSION {
            return Err(format!("unknown version {}", link.version));
        }
        Ok(Some(link))
    }

    fn save(&self, link: &OwnerLink) -> Result<(), String> {
        let plain = serde_json::to_vec(link).map_err(|error| error.to_string())?;
        let sealed =
            dpapi::protect(&plain).map_err(|error| format!("cannot encrypt it: {error}"))?;
        let tmp = self.tmp_path();
        std::fs::write(&tmp, sealed)
            .map_err(|error| format!("cannot write {}: {error}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|error| format!("cannot replace {}: {error}", self.path.display()))
    }

    fn delete(&self) -> Result<(), String> {
        let _ = std::fs::remove_file(self.tmp_path());
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("cannot delete {}: {error}", self.path.display())),
        }
    }

    fn describe(&self) -> String {
        self.path.display().to_string()
    }
}

/// The store this build uses: the DPAPI file on Windows, memory elsewhere.
pub fn platform_store(data_dir: &Path) -> Arc<dyn LinkStore> {
    if cfg!(windows) {
        Arc::new(FileLinkStore::new(link_path(data_dir)))
    } else {
        Arc::new(MemoryLinkStore::default())
    }
}

#[cfg(windows)]
mod dpapi {
    //! `CryptProtectData` for the current user, with an app constant as the optional
    //! entropy. The entropy is no secret (the source is public); it keeps a program that
    //! calls DPAPI without it from opening the blob by chance.

    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    const ENTROPY: &[u8] = b"VoiceBro owner link v1";

    fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        }
    }

    /// Copies the system-allocated output and frees it.
    fn take(output: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        if output.pbData.is_null() {
            return Vec::new();
        }
        // SAFETY: on success DPAPI hands over `cbData` bytes at `pbData`, allocated with
        // LocalAlloc; they are copied before the block is freed once.
        unsafe {
            let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
            LocalFree(output.pbData as _);
            bytes
        }
    }

    pub fn protect(plain: &[u8]) -> std::io::Result<Vec<u8>> {
        let input = blob(plain);
        let entropy = blob(ENTROPY);
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        // SAFETY: the input and entropy blobs point into live slices that DPAPI only reads;
        // `output` receives a block freed by `take`.
        let ok = unsafe {
            CryptProtectData(
                &input,
                std::ptr::null(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(take(output))
    }

    pub fn unprotect(sealed: &[u8]) -> std::io::Result<Vec<u8>> {
        let input = blob(sealed);
        let entropy = blob(ENTROPY);
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        // SAFETY: as in `protect`; no description is asked for.
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(take(output))
    }
}

#[cfg(not(windows))]
mod dpapi {
    fn unsupported() -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::Unsupported, "DPAPI is Windows only")
    }
    pub fn protect(_: &[u8]) -> std::io::Result<Vec<u8>> {
        Err(unsupported())
    }
    pub fn unprotect(_: &[u8]) -> std::io::Result<Vec<u8>> {
        Err(unsupported())
    }
}

/// A fresh code from the OS random source.
fn new_code() -> Result<[u8; CODE_LEN], OwnerError> {
    use rand::TryRngCore;
    let mut bytes = [0u8; CODE_LEN];
    rand::rngs::OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|error| OwnerError::Random(error.to_string()))?;
    Ok(bytes.map(|byte| ALPHABET[usize::from(byte % 32)]))
}

/// `XXXX-XXXX`.
fn format_code(code: &[u8; CODE_LEN]) -> String {
    let text = String::from_utf8_lossy(code);
    format!("{}-{}", &text[..4], &text[4..])
}

/// A typed code as the alphabet spells it: upper case, spaces and dashes dropped, `O` read
/// as `0` and `I`, `L` as `1`. `None` when it cannot be a code.
pub fn normalize_code(typed: &str) -> Option<[u8; CODE_LEN]> {
    let mut out = [0u8; CODE_LEN];
    let mut count = 0;
    for character in typed.chars() {
        let symbol = match character.to_ascii_uppercase() {
            ' ' | '-' => continue,
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        };
        if !symbol.is_ascii() || !ALPHABET.contains(&(symbol as u8)) || count == CODE_LEN {
            return None;
        }
        out[count] = symbol as u8;
        count += 1;
    }
    (count == CODE_LEN).then_some(out)
}

/// Every byte compared, whatever the first difference.
fn same_code(a: &[u8; CODE_LEN], b: &[u8; CODE_LEN]) -> bool {
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

struct Waiting {
    code: [u8; CODE_LEN],
    account: String,
    expires_at: Instant,
    expires_at_ms: u64,
    wrong_tries: u8,
}

#[derive(Default)]
struct Inner {
    mode_on: bool,
    link: Option<OwnerLink>,
    waiting: Option<Waiting>,
    code_ended: Option<String>,
    link_ended: Option<String>,
}

type Clock = Box<dyn Fn() -> Instant + Send + Sync>;

/// The owner state the runtime, the Discord adapter and the window share.
pub struct OwnerState {
    inner: Mutex<Inner>,
    filter: AtomicU64,
    store: Arc<dyn LinkStore>,
    events: EventBus,
    clock: Clock,
}

impl OwnerState {
    /// Reads the stored link; an unreadable file is no link, logged once with its reason.
    pub fn new(store: Arc<dyn LinkStore>, events: EventBus) -> Self {
        Self::with_clock(store, events, Box::new(Instant::now))
    }

    pub fn with_clock(store: Arc<dyn LinkStore>, events: EventBus, clock: Clock) -> Self {
        let mut inner = Inner::default();
        match store.load() {
            Ok(link) => inner.link = link,
            Err(reason) => {
                log::warn!(
                    "owner link: {} could not be read ({reason}); no owner is linked",
                    store.describe()
                );
                inner.link_ended = Some("the link file could not be read".to_owned());
            }
        }
        let state = Self {
            inner: Mutex::new(inner),
            filter: AtomicU64::new(FILTER_EVERYONE),
            store,
            events,
            clock,
        };
        state.refresh_filter(&state.lock());
        state
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("owner state lock")
    }

    fn refresh_filter(&self, inner: &Inner) {
        let value = match (&inner.link, inner.mode_on) {
            (_, false) => FILTER_EVERYONE,
            (None, true) => FILTER_NOBODY,
            (Some(link), true) => link.discord_user_id,
        };
        self.filter.store(value, Ordering::Release);
    }

    /// Publishes the status and recomputes the filter after a change.
    fn changed(&self, inner: &Inner) {
        self.refresh_filter(inner);
        self.events
            .publish(AppEvent::Owner(Box::new(self.status_of(inner))));
    }

    /// `0` everyone, [`FILTER_NOBODY`] nobody, else the only Discord user id whose voice
    /// passes. Read by the receive path once per tick.
    pub fn filter(&self) -> u64 {
        self.filter.load(Ordering::Acquire)
    }

    /// `discord.owner_only` changed (or was read at start).
    pub fn set_mode(&self, on: bool) {
        let mut inner = self.lock();
        if inner.mode_on == on {
            return;
        }
        inner.mode_on = on;
        self.changed(&inner);
    }

    pub fn status(&self) -> OwnerStatus {
        self.status_of(&self.lock())
    }

    fn status_of(&self, inner: &Inner) -> OwnerStatus {
        let now = (self.clock)();
        let expired = inner
            .waiting
            .as_ref()
            .is_some_and(|waiting| now >= waiting.expires_at);
        OwnerStatus {
            linked: inner.link.as_ref().map(|link| LinkedOwner {
                id: link.discord_user_id.to_string(),
                name: link.discord_name.clone(),
                linked_at: link.linked_at.clone(),
            }),
            pending: inner
                .waiting
                .as_ref()
                .filter(|_| !expired)
                .map(|waiting| PendingCode {
                    code: format_code(&waiting.code),
                    expires_at_ms: waiting.expires_at_ms,
                    wrong_tries: waiting.wrong_tries,
                }),
            code_ended: if expired {
                Some("the code expired".to_owned())
            } else {
                inner.code_ended.clone()
            },
            link_ended: inner.link_ended.clone(),
        }
    }

    /// Issues a code bound to the assistant account `account` (its fingerprint). A code
    /// already waiting is replaced.
    pub fn issue_code(&self, account: String) -> Result<PendingCode, OwnerError> {
        let code = new_code()?;
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as u64)
            .unwrap_or_default();
        let mut inner = self.lock();
        inner.waiting = Some(Waiting {
            code,
            account,
            expires_at: (self.clock)() + CODE_LIFETIME,
            expires_at_ms: now_ms + CODE_LIFETIME.as_millis() as u64,
            wrong_tries: 0,
        });
        inner.code_ended = None;
        self.events.log(format!(
            "owner link: a code was issued, valid for {} minutes",
            CODE_LIFETIME.as_secs() / 60
        ));
        self.changed(&inner);
        Ok(self
            .status_of(&inner)
            .pending
            .expect("the code just issued is waiting"))
    }

    /// Cancel in the window: the waiting code ends. `false` when none was waiting.
    pub fn cancel_code(&self) -> bool {
        let mut inner = self.lock();
        if inner.waiting.take().is_none() {
            return false;
        }
        inner.code_ended = Some("the code was cancelled".to_owned());
        self.events.log("owner link: the code was cancelled");
        self.changed(&inner);
        true
    }

    /// A `/link` from Discord: `user` is the id Discord put in the interaction.
    pub fn complete(&self, user: u64, name: &str, typed: &str) -> CompleteOutcome {
        let now = (self.clock)();
        let mut inner = self.lock();
        let Some(waiting) = inner.waiting.as_mut() else {
            return CompleteOutcome::NoCode;
        };
        if now >= waiting.expires_at {
            inner.waiting = None;
            inner.code_ended = Some("the code expired".to_owned());
            self.events
                .log("owner link: a /link came after the code expired");
            self.changed(&inner);
            return CompleteOutcome::Expired;
        }
        let right = normalize_code(typed).is_some_and(|code| same_code(&code, &waiting.code));
        if !right {
            waiting.wrong_tries += 1;
            let tries = waiting.wrong_tries;
            if tries >= MAX_WRONG_TRIES {
                inner.waiting = None;
                inner.code_ended = Some(format!(
                    "the code was ended after {MAX_WRONG_TRIES} wrong tries"
                ));
                self.events.log(format!(
                    "owner link: wrong code ({tries} of {MAX_WRONG_TRIES}); the code has ended"
                ));
                self.changed(&inner);
                return CompleteOutcome::EndedAfterTries;
            }
            self.events.log(format!(
                "owner link: wrong code ({tries} of {MAX_WRONG_TRIES})"
            ));
            self.changed(&inner);
            return CompleteOutcome::Wrong {
                tries_left: MAX_WRONG_TRIES - tries,
            };
        }
        let account = waiting.account.clone();
        inner.waiting = None;
        let link = OwnerLink {
            version: FILE_VERSION,
            discord_user_id: user,
            discord_name: name.to_owned(),
            assistant: "codex".to_owned(),
            account,
            linked_at: chrono::Utc::now().to_rfc3339(),
        };
        if let Err(reason) = self.store.save(&link) {
            log::error!("owner link: could not store the link: {reason}");
            inner.code_ended = Some(format!("the link could not be stored: {reason}"));
            self.changed(&inner);
            return CompleteOutcome::NotStored { reason };
        }
        match inner.link.as_ref() {
            Some(previous) if previous.discord_user_id != user => self.events.log(format!(
                "owner link: owner changed from {} ({}) to {name} ({user})",
                previous.discord_name, previous.discord_user_id
            )),
            _ => self
                .events
                .log(format!("owner link: linked {name} ({user})")),
        }
        inner.link = Some(link);
        inner.code_ended = None;
        inner.link_ended = None;
        self.changed(&inner);
        CompleteOutcome::Linked {
            name: name.to_owned(),
        }
    }

    /// Unlink in the window. `false` when nothing was linked.
    pub fn unlink(&self) -> Result<bool, String> {
        let mut inner = self.lock();
        if inner.link.is_none() {
            return Ok(false);
        }
        self.store.delete()?;
        inner.link = None;
        inner.link_ended = None;
        self.events.log("owner link: removed with Unlink");
        self.changed(&inner);
        Ok(true)
    }

    /// The assistant reported a signed-in account (Check, Link, a join, the read at start).
    /// A link made with another account is removed; returns `true` then.
    pub fn check_account(&self, fingerprint: &str) -> bool {
        let mut inner = self.lock();
        let differs = inner
            .link
            .as_ref()
            .is_some_and(|link| link.account != fingerprint);
        if !differs {
            return false;
        }
        if let Err(reason) = self.store.delete() {
            log::error!("owner link: could not delete the link file: {reason}");
        }
        inner.link = None;
        inner.link_ended = Some("the Codex account changed".to_owned());
        self.events
            .log("owner link: removed because the Codex account changed; link again in the window");
        self.changed(&inner);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestClock(Arc<Mutex<Instant>>);

    impl TestClock {
        fn new() -> (Self, Clock) {
            let now = Arc::new(Mutex::new(Instant::now()));
            let shared = Arc::clone(&now);
            (
                Self(now),
                Box::new(move || *shared.lock().expect("test clock")),
            )
        }
        fn advance(&self, by: Duration) {
            *self.0.lock().expect("test clock") += by;
        }
    }

    fn state() -> (OwnerState, TestClock, Arc<MemoryLinkStore>) {
        let (clock, boxed) = TestClock::new();
        let store = Arc::new(MemoryLinkStore::default());
        let state = OwnerState::with_clock(
            Arc::clone(&store) as Arc<dyn LinkStore>,
            EventBus::new(64),
            boxed,
        );
        (state, clock, store)
    }

    fn typed(pending: &PendingCode) -> String {
        pending.code.clone()
    }

    #[test]
    fn a_code_has_eight_symbols_of_the_alphabet_shown_in_two_groups() {
        for _ in 0..200 {
            let code = new_code().unwrap();
            assert!(code.iter().all(|symbol| ALPHABET.contains(symbol)));
            for banned in b"ILOU" {
                assert!(!code.contains(banned));
            }
            let shown = format_code(&code);
            assert_eq!(shown.len(), 9);
            assert_eq!(shown.as_bytes()[4], b'-');
        }
    }

    #[test]
    fn a_code_typed_loosely_is_the_same_code() {
        let code = *b"K7QF2M9X";
        for typed in ["K7QF-2M9X", "k7qf2m9x", "K7QF 2M9X", " k7qf-2m9x "] {
            assert_eq!(normalize_code(typed), Some(code), "{typed}");
        }
        assert_eq!(normalize_code("1O2I3L45"), Some(*b"10213145"));
        assert_eq!(normalize_code("K7QF2M9"), None, "too short");
        assert_eq!(normalize_code("K7QF2M9XY"), None, "too long");
        assert_eq!(normalize_code("K7QF2M9U"), None, "U is not a symbol");
        assert!(same_code(&code, b"K7QF2M9X"));
        assert!(!same_code(&code, b"K7QF2M9Y"));
    }

    #[test]
    fn the_right_code_links_the_sender_and_works_once() {
        let (state, _, store) = state();
        let pending = state.issue_code("fp-x".to_owned()).unwrap();
        assert_eq!(
            state.complete(111, "Alex", &typed(&pending).to_lowercase()),
            CompleteOutcome::Linked {
                name: "Alex".to_owned()
            }
        );
        let status = state.status();
        assert_eq!(status.linked.as_ref().unwrap().id, "111");
        assert!(status.pending.is_none());
        assert_eq!(store.load().unwrap().unwrap().account, "fp-x");
        assert_eq!(
            state.complete(222, "Sam", &typed(&pending)),
            CompleteOutcome::NoCode,
            "a used code changes nothing"
        );
        assert_eq!(state.status().linked.unwrap().id, "111");
    }

    #[test]
    fn a_code_ends_after_ten_minutes() {
        let (state, clock, _) = state();
        let pending = state.issue_code("fp".to_owned()).unwrap();
        clock.advance(CODE_LIFETIME - Duration::from_secs(1));
        assert!(state.status().pending.is_some());
        clock.advance(Duration::from_secs(61));
        let status = state.status();
        assert!(status.pending.is_none(), "the window shows no code waiting");
        assert_eq!(status.code_ended.as_deref(), Some("the code expired"));
        assert_eq!(
            state.complete(111, "Alex", &typed(&pending)),
            CompleteOutcome::Expired
        );
        assert!(state.status().linked.is_none());
    }

    #[test]
    fn the_fifth_wrong_code_ends_it() {
        let (state, _, _) = state();
        let pending = state.issue_code("fp".to_owned()).unwrap();
        for tries_left in (1..MAX_WRONG_TRIES).rev() {
            assert_eq!(
                state.complete(999, "Guess", "AAAA-AAAA"),
                CompleteOutcome::Wrong { tries_left }
            );
        }
        assert_eq!(state.status().pending.unwrap().wrong_tries, 4);
        assert_eq!(
            state.complete(999, "Guess", "nonsense"),
            CompleteOutcome::EndedAfterTries
        );
        assert_eq!(
            state.complete(111, "Alex", &typed(&pending)),
            CompleteOutcome::NoCode,
            "the right code afterwards links nothing"
        );
        assert!(state.status().linked.is_none());
        assert!(state.status().code_ended.unwrap().contains("5 wrong tries"));
    }

    #[test]
    fn link_again_replaces_the_code_and_cancel_ends_it() {
        let (state, _, _) = state();
        let first = state.issue_code("fp".to_owned()).unwrap();
        let second = state.issue_code("fp".to_owned()).unwrap();
        if first.code != second.code {
            assert!(matches!(
                state.complete(111, "Alex", &typed(&first)),
                CompleteOutcome::Wrong { .. }
            ));
        }
        assert!(state.cancel_code());
        assert_eq!(
            state.status().code_ended.as_deref(),
            Some("the code was cancelled")
        );
        assert_eq!(
            state.complete(111, "Alex", &typed(&second)),
            CompleteOutcome::NoCode
        );
        assert!(!state.cancel_code(), "nothing left to cancel");
    }

    #[test]
    fn the_filter_follows_the_mode_and_the_link() {
        let (state, _, _) = state();
        assert_eq!(state.filter(), FILTER_EVERYONE);
        state.set_mode(true);
        assert_eq!(state.filter(), FILTER_NOBODY, "on without a link: nobody");
        let pending = state.issue_code("fp".to_owned()).unwrap();
        state.complete(1091401169441656933, "Alex", &typed(&pending));
        assert_eq!(state.filter(), 1091401169441656933);
        state.set_mode(false);
        assert_eq!(state.filter(), FILTER_EVERYONE, "off: everyone, link kept");
        assert!(state.status().linked.is_some());
        state.set_mode(true);
        assert!(state.unlink().unwrap());
        assert_eq!(state.filter(), FILTER_NOBODY);
        assert!(!state.unlink().unwrap(), "nothing left to unlink");
    }

    #[test]
    fn another_account_replaces_the_owner() {
        let (state, _, _) = state();
        let pending = state.issue_code("fp".to_owned()).unwrap();
        state.complete(1, "A", &typed(&pending));
        let pending = state.issue_code("fp".to_owned()).unwrap();
        state.complete(2, "B", &typed(&pending));
        let linked = state.status().linked.unwrap();
        assert_eq!((linked.id.as_str(), linked.name.as_str()), ("2", "B"));
    }

    #[test]
    fn the_debug_form_never_holds_the_code() {
        let (state, _, _) = state();
        let pending = state.issue_code("fp".to_owned()).unwrap();
        let event = AppEvent::Owner(Box::new(state.status()));
        let debug = format!("{event:?} {pending:?}");
        let bare = pending.code.replace('-', "");
        assert!(!debug.contains(&pending.code), "{debug}");
        assert!(!debug.contains(&bare), "{debug}");
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["pending"]["code"], pending.code, "the window gets it");
    }

    /// Task 5.2: each outcome has its own reply, and none holds a code.
    #[test]
    fn every_outcome_has_its_reply() {
        let replies = [
            (
                CompleteOutcome::Linked {
                    name: "Alex".to_owned(),
                },
                "Linked: Alex",
            ),
            (CompleteOutcome::Wrong { tries_left: 4 }, "4 tries left"),
            (CompleteOutcome::Wrong { tries_left: 1 }, "1 try left"),
            (CompleteOutcome::EndedAfterTries, "5th wrong one"),
            (CompleteOutcome::Expired, "The code expired"),
            (CompleteOutcome::NoCode, "No code is waiting"),
            (
                CompleteOutcome::NotStored {
                    reason: "disk full".to_owned(),
                },
                "could not store the link (disk full)",
            ),
        ];
        for (outcome, expected) in replies {
            let reply = outcome.reply();
            assert!(reply.contains(expected), "{outcome:?}: {reply}");
        }
    }

    /// Change `owner-only-access`, D4: the fingerprint ignores case and spaces in the
    /// email, an API-key account has its own, and two emails differ.
    #[test]
    fn the_account_fingerprint() {
        let a = account_fingerprint(Some("chatgpt"), Some(" Me@Example.com "));
        assert_eq!(
            a,
            account_fingerprint(Some("chatgpt"), Some("me@example.com"))
        );
        assert_eq!(a.len(), 64);
        assert_ne!(
            a,
            account_fingerprint(Some("chatgpt"), Some("you@example.com"))
        );
        let key = account_fingerprint(Some("apiKey"), None);
        assert_eq!(key, account_fingerprint(Some("apiKey"), Some("")));
        assert_ne!(key, a);
    }

    #[test]
    fn another_assistant_account_removes_the_link_and_signing_out_does_not() {
        let (state, _, store) = state();
        state.set_mode(true);
        let pending = state.issue_code("fp-x".to_owned()).unwrap();
        state.complete(111, "Alex", &typed(&pending));
        assert!(!state.check_account("fp-x"), "the same account keeps it");
        assert!(state.status().linked.is_some());
        assert!(state.check_account("fp-y"));
        let status = state.status();
        assert!(status.linked.is_none());
        assert_eq!(
            status.link_ended.as_deref(),
            Some("the Codex account changed")
        );
        assert_eq!(state.filter(), FILTER_NOBODY);
        assert!(store.load().unwrap().is_none(), "the file is gone");
        assert!(
            !state.check_account("fp-x"),
            "the first account does not bring it back"
        );
    }

    #[cfg(windows)]
    mod file {
        use super::*;

        fn sample() -> OwnerLink {
            OwnerLink {
                version: FILE_VERSION,
                discord_user_id: 1091401169441656933,
                discord_name: "Alex".to_owned(),
                assistant: "codex".to_owned(),
                account: account_fingerprint(Some("chatgpt"), Some("me@example.com")),
                linked_at: "2026-11-16T12:00:00+00:00".to_owned(),
            }
        }

        #[test]
        fn a_link_survives_a_write_and_a_read() {
            let dir = tempfile::tempdir().unwrap();
            let store = FileLinkStore::new(link_path(dir.path()));
            assert_eq!(store.load().unwrap(), None, "no file: no link");
            store.save(&sample()).unwrap();
            assert_eq!(store.load().unwrap(), Some(sample()));
            let bytes = std::fs::read(link_path(dir.path())).unwrap();
            assert!(
                !String::from_utf8_lossy(&bytes).contains("Alex"),
                "the file is encrypted"
            );
            assert!(!dir.path().join("owner-link.bin.tmp").exists());

            let state = OwnerState::new(
                Arc::new(FileLinkStore::new(link_path(dir.path()))),
                EventBus::new(8),
            );
            assert_eq!(
                state.status().linked.unwrap().name,
                "Alex",
                "a restart keeps it"
            );
            assert!(state.unlink().unwrap());
            assert!(!link_path(dir.path()).exists(), "Unlink leaves no file");
        }

        #[test]
        fn a_changed_byte_reads_as_unreadable() {
            let dir = tempfile::tempdir().unwrap();
            let store = FileLinkStore::new(link_path(dir.path()));
            store.save(&sample()).unwrap();
            let mut bytes = std::fs::read(link_path(dir.path())).unwrap();
            let middle = bytes.len() / 2;
            bytes[middle] ^= 0x01;
            std::fs::write(link_path(dir.path()), &bytes).unwrap();
            assert!(store.load().is_err());
            let state = OwnerState::new(
                Arc::new(FileLinkStore::new(link_path(dir.path()))),
                EventBus::new(8),
            );
            let status = state.status();
            assert!(status.linked.is_none());
            assert_eq!(
                status.link_ended.as_deref(),
                Some("the link file could not be read")
            );
        }

        #[test]
        fn an_unknown_version_reads_as_unreadable() {
            let dir = tempfile::tempdir().unwrap();
            let mut link = sample();
            link.version = 2;
            let sealed = dpapi::protect(&serde_json::to_vec(&link).unwrap()).unwrap();
            std::fs::write(link_path(dir.path()), sealed).unwrap();
            let error = FileLinkStore::new(link_path(dir.path()))
                .load()
                .unwrap_err();
            assert!(error.contains("version 2"), "{error}");
        }
    }
}
