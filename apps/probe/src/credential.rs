//! P0.5: Windows Credential Manager round trip through `windows-sys`.
//!
//! Pass criterion: a generic credential `VoiceBro/test` is written with local-machine
//! persistence, read back byte for byte, and deleted. With `--keep` the entry stays so the
//! user can reboot and run `probe credential --read-only` to confirm it survived.

use crate::Outcome;
use anyhow::{anyhow, bail};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows_sys::Win32::Foundation::{FILETIME, TRUE};
use windows_sys::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
    CRED_TYPE_GENERIC,
};

const TARGET: &str = "VoiceBro/test";
const USER: &str = "voicebro";

fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Writes `value` (stored as UTF-16LE, the convention the Windows tools expect) under
/// `target`, persisted for this user across reboots.
pub fn write(target: &str, value: &str) -> anyhow::Result<()> {
    let mut target_w = wide(target);
    let mut user_w = wide(USER);
    let mut blob: Vec<u8> = value
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    if blob.len() > 2560 {
        bail!("credential blob is {} bytes; the limit is 2560", blob.len());
    }
    let credential = CREDENTIALW {
        Flags: 0,
        Type: CRED_TYPE_GENERIC,
        TargetName: target_w.as_mut_ptr(),
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
        UserName: user_w.as_mut_ptr(),
    };
    // SAFETY: every pointer in `credential` points into a live local buffer for the call.
    let ok = unsafe { CredWriteW(&credential, 0) };
    if ok != TRUE {
        return Err(anyhow!("CredWriteW: {}", std::io::Error::last_os_error()));
    }
    Ok(())
}

/// Reads the credential back as a string, or `Ok(None)` when it does not exist.
pub fn read(target: &str) -> anyhow::Result<Option<String>> {
    let target_w = wide(target);
    let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: `credential` receives a pointer owned by the system; freed with CredFree.
    let ok = unsafe { CredReadW(target_w.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
    if ok != TRUE {
        let error = std::io::Error::last_os_error();
        // ERROR_NOT_FOUND
        if error.raw_os_error() == Some(1168) {
            return Ok(None);
        }
        return Err(anyhow!("CredReadW: {error}"));
    }
    // SAFETY: on success `credential` is a valid CREDENTIALW with a blob of the given size.
    let value = unsafe {
        let data = std::slice::from_raw_parts(
            (*credential).CredentialBlob,
            (*credential).CredentialBlobSize as usize,
        );
        let units: Vec<u16> = data
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let text = String::from_utf16_lossy(&units);
        CredFree(credential as *const _);
        text
    };
    Ok(Some(value))
}

pub fn delete(target: &str) -> anyhow::Result<()> {
    let target_w = wide(target);
    // SAFETY: plain call with a valid null-terminated wide string.
    let ok = unsafe { CredDeleteW(target_w.as_ptr(), CRED_TYPE_GENERIC, 0) };
    if ok != TRUE {
        return Err(anyhow!("CredDeleteW: {}", std::io::Error::last_os_error()));
    }
    Ok(())
}

pub fn roundtrip(args: &[String]) -> anyhow::Result<Outcome> {
    let keep = args.iter().any(|arg| arg == "--keep");
    let read_only = args.iter().any(|arg| arg == "--read-only");
    let mut details = Vec::new();

    if read_only {
        return Ok(match read(TARGET)? {
            Some(value) => {
                details.push(format!(
                    "{TARGET} present after restart, {} chars",
                    value.len()
                ));
                let pass = value.starts_with("probe-token-");
                details.push(format!("value has the probe prefix: {pass}"));
                if pass {
                    delete(TARGET)?;
                    details.push("deleted".to_owned());
                }
                Outcome { pass, details }
            }
            None => {
                details.push(format!("{TARGET} not found"));
                Outcome::fail(details)
            }
        });
    }

    let value = format!(
        "probe-token-{:x}-{}",
        rand::random::<u64>(),
        "x".repeat(60) // about the length of a Discord bot token
    );
    write(TARGET, &value)?;
    details.push(format!(
        "CredWriteW {TARGET}: generic, persist=LOCAL_MACHINE, {} UTF-16 bytes",
        value.len() * 2
    ));
    let back = read(TARGET)?;
    let matches = back.as_deref() == Some(value.as_str());
    details.push(format!(
        "CredReadW: {}",
        if matches {
            "same value"
        } else {
            "DIFFERENT value"
        }
    ));
    details.push(
        "check by hand: Control Panel → Credential Manager → Windows Credentials → Generic → VoiceBro/test"
            .to_owned(),
    );
    if keep {
        details.push(
            "kept for the reboot check: run `probe credential --read-only` after a reboot"
                .to_owned(),
        );
    } else {
        delete(TARGET)?;
        let gone = read(TARGET)?.is_none();
        details.push(format!("CredDeleteW: entry gone={gone}"));
        return Ok(Outcome {
            pass: matches && gone,
            details,
        });
    }
    Ok(Outcome {
        pass: matches,
        details,
    })
}
