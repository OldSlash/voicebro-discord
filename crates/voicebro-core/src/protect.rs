//! Files only the current Windows user can read (change `voice-consent`, design D2): DPAPI's
//! `CryptProtectData` for the current user, with a constant per file as the optional
//! entropy, written through `<file>.tmp` and a rename. The owner link (`owner-link.bin`) and
//! the consent records (`consents.bin`) use it. The entropy is no secret (the source is
//! public); it keeps a program that calls DPAPI without it from opening a file by chance.

use std::path::{Path, PathBuf};

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
    std::fs::write(&tmp, sealed)
        .map_err(|error| format!("cannot write {}: {error}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .map_err(|error| format!("cannot replace {}: {error}", path.display()))
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
pub use unsupported::{protect, unprotect};

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

#[cfg(not(windows))]
mod unsupported {
    fn unsupported() -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::Unsupported, "DPAPI is Windows only")
    }
    pub fn protect(_: &[u8], _: &[u8]) -> std::io::Result<Vec<u8>> {
        Err(unsupported())
    }
    pub fn unprotect(_: &[u8], _: &[u8]) -> std::io::Result<Vec<u8>> {
        Err(unsupported())
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
