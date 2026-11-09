//! Secrets live in the operating system's credential store, never in files, snapshots,
//! events or logs. The UI writes a secret through one command and only ever reads back
//! whether it is set.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretKey {
    DiscordToken,
}

impl SecretKey {
    /// The name the UI and the status use.
    pub fn name(self) -> &'static str {
        match self {
            SecretKey::DiscordToken => "discord_token",
        }
    }

    /// The credential store target.
    pub fn target(self) -> &'static str {
        match self {
            SecretKey::DiscordToken => "VoiceBro/discord_token",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "discord_token" => Some(SecretKey::DiscordToken),
            _ => None,
        }
    }

    pub const ALL: &'static [SecretKey] = &[SecretKey::DiscordToken];
}

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("the secret is {0} bytes; the credential store allows 2560")]
    TooLong(usize),
    #[error("credential store: {0}")]
    Backend(String),
}

pub trait SecretStore: Send + Sync {
    fn get(&self, key: SecretKey) -> Result<Option<String>, SecretError>;
    fn set(&self, key: SecretKey, value: &str) -> Result<(), SecretError>;
    fn delete(&self, key: SecretKey) -> Result<(), SecretError>;
}

/// In-memory store for tests and for platforms without a credential store.
#[derive(Default)]
pub struct MemoryStore {
    values: Mutex<HashMap<SecretKey, String>>,
}

impl SecretStore for MemoryStore {
    fn get(&self, key: SecretKey) -> Result<Option<String>, SecretError> {
        Ok(self.values.lock().expect("memory store lock").get(&key).cloned())
    }

    fn set(&self, key: SecretKey, value: &str) -> Result<(), SecretError> {
        self.values
            .lock()
            .expect("memory store lock")
            .insert(key, value.to_owned());
        Ok(())
    }

    fn delete(&self, key: SecretKey) -> Result<(), SecretError> {
        self.values.lock().expect("memory store lock").remove(&key);
        Ok(())
    }
}

/// The store this build uses: the Windows Credential Manager, or memory elsewhere.
pub fn platform_store() -> Box<dyn SecretStore> {
    #[cfg(windows)]
    {
        Box::new(windows::CredentialManagerStore)
    }
    #[cfg(not(windows))]
    {
        Box::new(MemoryStore::default())
    }
}

#[cfg(windows)]
pub mod windows {
    //! Generic credentials, persisted for this user across reboots, blob stored as UTF-16
    //! as the Windows tools expect.

    use super::{SecretError, SecretKey, SecretStore};
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{FILETIME, TRUE};
    use windows_sys::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    };

    const BLOB_LIMIT: usize = 2560;
    const ERROR_NOT_FOUND: i32 = 1168;
    const USER_NAME: &str = "voicebro";

    pub struct CredentialManagerStore;

    fn wide(text: &str) -> Vec<u16> {
        OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn backend_error(call: &str) -> SecretError {
        SecretError::Backend(format!("{call}: {}", std::io::Error::last_os_error()))
    }

    impl SecretStore for CredentialManagerStore {
        fn get(&self, key: SecretKey) -> Result<Option<String>, SecretError> {
            let target = wide(key.target());
            let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
            // SAFETY: `credential` receives a system-owned pointer, released with CredFree.
            let ok = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
            if ok != TRUE {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() == Some(ERROR_NOT_FOUND) {
                    return Ok(None);
                }
                return Err(SecretError::Backend(format!("CredReadW: {error}")));
            }
            // SAFETY: on success the pointer is a valid CREDENTIALW with a blob of the given size.
            let value = unsafe {
                let bytes = std::slice::from_raw_parts(
                    (*credential).CredentialBlob,
                    (*credential).CredentialBlobSize as usize,
                );
                let units: Vec<u16> = bytes
                    .chunks_exact(2)
                    .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                    .collect();
                let text = String::from_utf16_lossy(&units);
                CredFree(credential as *const _);
                text
            };
            Ok(Some(value))
        }

        fn set(&self, key: SecretKey, value: &str) -> Result<(), SecretError> {
            let mut target = wide(key.target());
            let mut user = wide(USER_NAME);
            let mut blob: Vec<u8> = value
                .encode_utf16()
                .flat_map(|unit| unit.to_le_bytes())
                .collect();
            if blob.len() > BLOB_LIMIT {
                return Err(SecretError::TooLong(blob.len()));
            }
            let credential = CREDENTIALW {
                Flags: 0,
                Type: CRED_TYPE_GENERIC,
                TargetName: target.as_mut_ptr(),
                Comment: std::ptr::null_mut(),
                LastWritten: FILETIME {
                    dwLowDateTime: 0,
                    dwHighDateTime: 0,
                },
                CredentialBlobSize: blob.len() as u32,
                CredentialBlob: blob.as_mut_ptr(),
                Persist: CRED_PERSIST_LOCAL_MACHINE,
                AttributeCount: 0,
                Attributes: std::ptr::null_mut(),
                TargetAlias: std::ptr::null_mut(),
                UserName: user.as_mut_ptr(),
            };
            // SAFETY: every pointer in `credential` points into a live local buffer for the call.
            let ok = unsafe { CredWriteW(&credential, 0) };
            if ok != TRUE {
                return Err(backend_error("CredWriteW"));
            }
            Ok(())
        }

        fn delete(&self, key: SecretKey) -> Result<(), SecretError> {
            let target = wide(key.target());
            // SAFETY: plain call with a valid null-terminated wide string.
            let ok = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
            if ok != TRUE {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() == Some(ERROR_NOT_FOUND) {
                    return Ok(());
                }
                return Err(SecretError::Backend(format!("CredDeleteW: {error}")));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_targets_round_trip() {
        for key in SecretKey::ALL {
            assert_eq!(SecretKey::from_name(key.name()), Some(*key));
            assert!(key.target().starts_with("VoiceBro/"));
        }
        assert!(SecretKey::from_name("password").is_none());
    }

    #[test]
    fn memory_store_sets_gets_and_deletes() {
        let store = MemoryStore::default();
        assert_eq!(store.get(SecretKey::DiscordToken).unwrap(), None);
        store.set(SecretKey::DiscordToken, "abc").unwrap();
        assert_eq!(store.get(SecretKey::DiscordToken).unwrap().as_deref(), Some("abc"));
        store.delete(SecretKey::DiscordToken).unwrap();
        assert_eq!(store.get(SecretKey::DiscordToken).unwrap(), None);
    }

    /// Touches the real Credential Manager; opt in with `VOICEBRO_SECRET_LIVE=1`.
    #[cfg(windows)]
    #[test]
    fn credential_manager_round_trip_when_enabled() {
        if std::env::var_os("VOICEBRO_SECRET_LIVE").is_none() {
            return;
        }
        let store = windows::CredentialManagerStore;
        let value = format!("live-test-{}", std::process::id());
        store.set(SecretKey::DiscordToken, &value).unwrap();
        assert_eq!(store.get(SecretKey::DiscordToken).unwrap().as_deref(), Some(value.as_str()));
        store.delete(SecretKey::DiscordToken).unwrap();
        assert_eq!(store.get(SecretKey::DiscordToken).unwrap(), None);
    }
}
