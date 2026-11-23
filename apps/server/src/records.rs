//! The window's access and token controls as terminal commands (change
//! `cross-platform-headless`, design D8): Unlink, the list of people who agreed with
//! Remove, and removing the stored token. They construct no runtime and connect to nothing;
//! a running bot keeps what it loaded at start, so they are meant for a stopped bot.

use anyhow::{bail, Result};
use std::io::Write;
use std::path::Path;
use voicebro_core::consent::{self, ConsentState, WithdrawOutcome};
use voicebro_core::owner::{self, OwnerState};
use voicebro_core::secrets::SecretKey;
use voicebro_core::settings::store::SettingsStore;
use voicebro_core::{protect, EventBus, Settings};

/// The settings file or the defaults: the engine opens the consent store as at a start,
/// and the Linux encryption decides how a changed records file is written.
fn settings_of(store: &SettingsStore) -> Result<Settings> {
    let settings = store.load()?.unwrap_or_default();
    protect::set_encryption(settings.linux.encrypt_files);
    Ok(settings)
}

fn consent_state(data_dir: &Path, settings: &Settings) -> ConsentState {
    ConsentState::new(
        consent::platform_store(data_dir),
        EventBus::for_terminal(),
        &consent::recipient_key(settings),
    )
}

/// `--unlink-owner`: Unlink, from the terminal.
pub fn unlink_owner(data_dir: &Path) -> Result<()> {
    let owner = OwnerState::new(owner::platform_store(data_dir), EventBus::for_terminal());
    let linked = owner.status().linked;
    if owner.unlink().map_err(anyhow::Error::msg)? {
        let linked = linked
            .map(|linked| format!("{} ({})", linked.name, linked.id))
            .unwrap_or_default();
        log::info!("Removed the owner link of {linked}.");
        return Ok(());
    }
    // A file that could not be read is no link; it goes too, so the next link starts clean.
    let path = owner::link_path(data_dir);
    if path.exists() {
        protect::delete_sealed(&path).map_err(anyhow::Error::msg)?;
        log::info!(
            "No owner could be read from {}; the unreadable file was deleted.",
            path.display()
        );
    } else {
        log::info!("No owner was linked; nothing changed.");
    }
    Ok(())
}

/// `--list-consents`: the people who agreed, on standard output.
pub fn list_consents(data_dir: &Path, store: &SettingsStore) -> Result<()> {
    let settings = settings_of(store)?;
    let status = consent_state(data_dir, &settings).status();
    if let Some(reason) = &status.unreadable {
        log::warn!("{reason}");
    }
    let mut out = std::io::stdout().lock();
    if status.records.is_empty() {
        writeln!(out, "Nobody has agreed.")?;
    }
    for record in &status.records {
        writeln!(
            out,
            "{}  {}  agreed {}",
            record.id, record.name, record.agreed_at
        )?;
    }
    out.flush()?;
    Ok(())
}

/// `--remove-consent <id>`: Remove, from the terminal; nothing is sent to the person.
pub fn remove_consent(data_dir: &Path, store: &SettingsStore, id: &str) -> Result<()> {
    let Some(user) = id.trim().parse::<u64>().ok().filter(|user| *user > 0) else {
        bail!("«{id}» is not a Discord user id; nothing was removed (voicebro-server --list-consents shows the ids)");
    };
    let settings = settings_of(store)?;
    match consent_state(data_dir, &settings).withdraw(user, true) {
        WithdrawOutcome::Removed { name } => {
            log::info!("Removed the consent record of {name} ({user}); nothing was sent to them.");
            Ok(())
        }
        WithdrawOutcome::NothingToRemove => {
            bail!("nobody with the id {user} has agreed; nothing was removed")
        }
    }
}

/// `--clear-token`: the stored token goes; the file `DISCORD_TOKEN_FILE` names stays.
pub fn clear_token(data_dir: &Path) -> Result<()> {
    let store = super::token_store(data_dir);
    // An unreadable stored token still counts as stored: it is removed all the same.
    let stored = store
        .get(SecretKey::DiscordToken)
        .map_or(true, |token| token.is_some());
    if !stored {
        log::info!("No Discord token was stored; nothing changed.");
        return Ok(());
    }
    store.delete(SecretKey::DiscordToken)?;
    if cfg!(windows) {
        log::info!(
            "Removed the Discord token from Windows Credential Manager ({}); the desktop app no longer has it either.",
            SecretKey::DiscordToken.target()
        );
    } else {
        log::info!(
            "Removed the Discord token ({}).",
            voicebro_core::secrets::token_path(data_dir).display()
        );
    }
    Ok(())
}
