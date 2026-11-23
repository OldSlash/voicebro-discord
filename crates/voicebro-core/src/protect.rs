//! Files only the current user can read (change `voice-consent`, design D2): on Windows
//! DPAPI's `CryptProtectData` for the current user, with a constant per file as the
//! optional entropy, written through `<file>.tmp` and a rename. The owner link
//! (`owner-link.bin`), the consent records (`consents.bin`) and, on Linux, the headless
//! app's Discord token (`discord-token.bin`) use it. The entropy is no secret (the source is
//! public); it keeps a program that calls DPAPI without it from opening a file by chance.
//!
//! Linux (change `cross-platform-headless`, design D8; not tested) has no DPAPI: a file is
//! XChaCha20-Poly1305 with the entropy as associated data, under a key derived with
//! HKDF-SHA256 from `~/.config/voicebro/protect.key` and the machine id. While
//! `linux.encrypt_files` is off ([`set_encryption`]) the file holds the plain bytes behind a
//! tag instead, and is still created readable by its user only.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// `linux.encrypt_files`, set at start; Windows always encrypts and never reads it.
static ENCRYPT: AtomicBool = AtomicBool::new(true);

/// Sets whether Linux encrypts the files it writes from now on.
pub fn set_encryption(on: bool) {
    ENCRYPT.store(on, Ordering::Relaxed);
}

/// Whether Linux encrypts the files it writes; always `true` on Windows.
pub fn encryption() -> bool {
    cfg!(windows) || ENCRYPT.load(Ordering::Relaxed)
}

/// Rewrites `path` in the form [`encryption`] asks for when it is readable in the other
/// form: `Ok(true)` when it was rewritten. Linux only; on Windows nothing changes.
pub fn reseal(path: &Path, entropy: &[u8]) -> Result<bool, String> {
    if cfg!(windows) {
        return Ok(false);
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("cannot read it: {error}")),
    };
    let encrypted = match sealed::form(&bytes) {
        Some(form) => form == sealed::Form::Encrypted,
        None => return Err("it is not a VoiceBro file".to_owned()),
    };
    if encrypted == encryption() {
        return Ok(false);
    }
    let plain =
        unprotect(&bytes, entropy).map_err(|error| format!("cannot decrypt it: {error}"))?;
    write_sealed(path, &plain, entropy)?;
    Ok(true)
}

/// `<path>.tmp`, where a new version is written before the rename.
pub fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

/// The decrypted bytes of `path`; `Ok(None)` when there is no file, an error with the
/// reason when it cannot be read or decrypted.
pub fn read_sealed(path: &Path, entropy: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let sealed = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot read it: {error}")),
    };
    unprotect(&sealed, entropy)
        .map(Some)
        .map_err(|error| format!("cannot decrypt it: {error}"))
}

/// Encrypts `plain` and replaces `path` with it through `<path>.tmp`.
pub fn write_sealed(path: &Path, plain: &[u8], entropy: &[u8]) -> Result<(), String> {
    let sealed = protect(plain, entropy).map_err(|error| format!("cannot encrypt it: {error}"))?;
    let tmp = tmp_path(path);
    write_private(&tmp, &sealed)
        .map_err(|error| format!("cannot write {}: {error}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .map_err(|error| format!("cannot replace {}: {error}", path.display()))
}

/// Writes a new file that on Linux only its user can read or write (mode 0600).
#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let _ = std::fs::remove_file(path);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)
}

/// Deletes `path` and a leftover `<path>.tmp`; a missing file is no error.
pub fn delete_sealed(path: &Path) -> Result<(), String> {
    let _ = std::fs::remove_file(tmp_path(path));
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot delete {}: {error}", path.display())),
    }
}

#[cfg(windows)]
pub use dpapi::{protect, unprotect};

#[cfg(not(windows))]
pub use sealed::{protect, unprotect};

#[cfg(windows)]
mod dpapi {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

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

    /// Encrypts for the current Windows user with `entropy`.
    pub fn protect(plain: &[u8], entropy: &[u8]) -> std::io::Result<Vec<u8>> {
        let input = blob(plain);
        let entropy = blob(entropy);
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

    /// Decrypts what [`protect`] made with the same `entropy` for the same user.
    pub fn unprotect(sealed: &[u8], entropy: &[u8]) -> std::io::Result<Vec<u8>> {
        let input = blob(sealed);
        let entropy = blob(entropy);
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

/// The Linux file format. Compiled on every target so Windows checks it; only Linux calls
/// `protect` and `unprotect` from here.
#[cfg_attr(windows, allow(dead_code))]
mod sealed {
    use chacha20poly1305::aead::{Aead, KeyInit, Payload};
    use chacha20poly1305::{XChaCha20Poly1305, XNonce};
    use std::io::{Error, ErrorKind, Result};

    /// An encrypted file: the tag, a 24-byte nonce, the ciphertext with its 16-byte tag.
    const ENCRYPTED: &[u8; 4] = b"VBE1";
    /// A plain file (`linux.encrypt_files` off): the tag, then the bytes.
    const PLAIN: &[u8; 4] = b"VBP1";
    const NONCE_LEN: usize = 24;
    /// HKDF's info: names what the derived key is for.
    const KEY_INFO: &[u8] = b"VoiceBro protect v1";

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Form {
        Encrypted,
        Plain,
    }

    /// The form a file says it has, by its tag.
    pub fn form(bytes: &[u8]) -> Option<Form> {
        match bytes.get(..4) {
            Some(tag) if tag == ENCRYPTED => Some(Form::Encrypted),
            Some(tag) if tag == PLAIN => Some(Form::Plain),
            _ => None,
        }
    }

    /// The encryption key: HKDF-SHA256 of the key file's bytes, salted with the machine id.
    pub fn derive(key_file: &[u8], machine_id: &[u8]) -> [u8; 32] {
        let mut key = [0u8; 32];
        hkdf::Hkdf::<sha2_010::Sha256>::new(Some(machine_id), key_file)
            .expand(KEY_INFO, &mut key)
            .expect("32 bytes is a valid HKDF-SHA256 length");
        key
    }

    /// Fills `bytes` from the OS random source.
    pub fn random(bytes: &mut [u8]) -> Result<()> {
        use rand::TryRngCore;
        rand::rngs::OsRng
            .try_fill_bytes(bytes)
            .map_err(|error| Error::other(format!("the OS random source failed: {error}")))
    }

    pub fn seal(key: &[u8; 32], plain: &[u8], entropy: &[u8]) -> Result<Vec<u8>> {
        let mut nonce = [0u8; NONCE_LEN];
        random(&mut nonce)?;
        let body = XChaCha20Poly1305::new(key.into())
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plain,
                    aad: entropy,
                },
            )
            .map_err(|_| Error::other("encryption failed"))?;
        let mut sealed = Vec::with_capacity(ENCRYPTED.len() + NONCE_LEN + body.len());
        sealed.extend_from_slice(ENCRYPTED);
        sealed.extend_from_slice(&nonce);
        sealed.extend_from_slice(&body);
        Ok(sealed)
    }

    pub fn open(key: &[u8; 32], sealed: &[u8], entropy: &[u8]) -> Result<Vec<u8>> {
        let rest = sealed
            .strip_prefix(ENCRYPTED.as_slice())
            .filter(|rest| rest.len() > NONCE_LEN)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "not an encrypted VoiceBro file"))?;
        let (nonce, body) = rest.split_at(NONCE_LEN);
        XChaCha20Poly1305::new(key.into())
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: body,
                    aad: entropy,
                },
            )
            .map_err(|_| {
                Error::new(
                    ErrorKind::InvalidData,
                    "it was changed, or written with another key or on another machine",
                )
            })
    }

    pub fn plain(bytes: &[u8]) -> Vec<u8> {
        let mut plain = Vec::with_capacity(PLAIN.len() + bytes.len());
        plain.extend_from_slice(PLAIN);
        plain.extend_from_slice(bytes);
        plain
    }

    /// Encrypts for this user on this machine, or tags the plain bytes while
    /// `linux.encrypt_files` is off.
    #[cfg(not(windows))]
    pub fn protect(plain_bytes: &[u8], entropy: &[u8]) -> Result<Vec<u8>> {
        if !super::encryption() {
            return Ok(plain(plain_bytes));
        }
        seal(&super::key::encryption_key(true)?, plain_bytes, entropy)
    }

    /// Reads a file in either form; an encrypted one needs this user's key on this machine.
    #[cfg(not(windows))]
    pub fn unprotect(sealed: &[u8], entropy: &[u8]) -> Result<Vec<u8>> {
        match form(sealed) {
            Some(Form::Plain) => Ok(sealed[PLAIN.len()..].to_vec()),
            Some(Form::Encrypted) => open(&super::key::encryption_key(false)?, sealed, entropy),
            None => Err(Error::new(ErrorKind::InvalidData, "not a VoiceBro file")),
        }
    }
}

/// The Linux key: `${XDG_CONFIG_HOME:-~/.config}/voicebro/protect.key`, 32 random bytes,
/// created on the first encrypted write with the folder at 0700 and the file at 0600.
#[cfg(unix)]
mod key {
    use std::io::{Error, ErrorKind, Result};
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
    use std::path::{Path, PathBuf};

    /// Where the key lives: outside the data folder, as DPAPI's key lives in the profile.
    pub fn key_path() -> Result<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .filter(|value| !value.is_empty())
                    .map(|home| PathBuf::from(home).join(".config"))
            })
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::NotFound,
                    "neither XDG_CONFIG_HOME nor HOME is set, so there is no place for the key",
                )
            })?;
        Ok(base.join("voicebro").join("protect.key"))
    }

    /// The machine id that binds the key to this machine.
    fn machine_id() -> Result<Vec<u8>> {
        for path in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
            if let Ok(text) = std::fs::read_to_string(path) {
                let id = text.trim();
                if !id.is_empty() {
                    return Ok(id.as_bytes().to_vec());
                }
            }
        }
        Err(Error::new(
            ErrorKind::NotFound,
            "/etc/machine-id is missing, so the key cannot be bound to this machine",
        ))
    }

    fn read_key(path: &Path) -> Result<[u8; 32]> {
        let metadata = std::fs::metadata(path)?;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                format!(
                    "{} can be read by other users; allow only its owner (chmod 600)",
                    path.display()
                ),
            ));
        }
        std::fs::read(path)?.try_into().map_err(|_| {
            Error::new(
                ErrorKind::InvalidData,
                format!("{} is not a VoiceBro key", path.display()),
            )
        })
    }

    fn create_key(path: &Path) -> Result<[u8; 32]> {
        use std::io::Write;
        if let Some(folder) = path.parent() {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(folder)?;
        }
        let mut key = [0u8; 32];
        super::sealed::random(&mut key)?;
        // Created exclusively: two processes starting at once never write two keys.
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
        {
            Ok(mut file) => {
                file.write_all(&key)?;
                file.sync_all()?;
                Ok(key)
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => read_key(path),
            Err(error) => Err(error),
        }
    }

    /// The encryption key of this user on this machine; `create` makes the key file when
    /// it is missing (a write), a read without it fails.
    pub fn encryption_key(create: bool) -> Result<[u8; 32]> {
        let path = key_path()?;
        let key = match read_key(&path) {
            Ok(key) => key,
            Err(error) if error.kind() == ErrorKind::NotFound && create => create_key(&path)?,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Err(Error::new(
                    ErrorKind::NotFound,
                    format!("the key {} is missing", path.display()),
                ))
            }
            Err(error) => return Err(error),
        };
        Ok(super::sealed::derive(&key, &machine_id()?))
    }
}

/// The Linux key file's path, for the docs and the messages; `None` on Windows or without
/// a home folder.
pub fn key_path() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        key::key_path().ok()
    }
    #[cfg(not(unix))]
    {
        None
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn a_sealed_file_round_trips_and_needs_its_entropy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.bin");
        assert_eq!(read_sealed(&path, b"a").unwrap(), None, "no file");
        write_sealed(&path, b"hello", b"a").unwrap();
        assert!(!tmp_path(&path).exists());
        assert_eq!(
            read_sealed(&path, b"a").unwrap().as_deref(),
            Some(&b"hello"[..])
        );
        assert!(read_sealed(&path, b"b").is_err(), "another entropy");
        delete_sealed(&path).unwrap();
        assert!(!path.exists());
        delete_sealed(&path).expect("deleting nothing is no error");
    }
}
