//! Windows Credential Manager adapter.

#![allow(unsafe_code)]

use std::ptr;

use explorer_automation::{
    AutomationError, AutomationErrorKind, AutomationFuture, CredentialStore,
};
use windows::{
    Win32::{
        Foundation::ERROR_NOT_FOUND,
        Security::Credentials::{
            CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
            CredReadW, CredWriteW,
        },
    },
    core::{PCWSTR, PWSTR},
};

const TARGET_PREFIX: &str = "ExplorerAutomation/";

/// Stores automation provider credentials encrypted by the current Windows account.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowsCredentialStore;

impl CredentialStore for WindowsCredentialStore {
    fn load(&self, key: String) -> AutomationFuture<Option<String>> {
        Box::pin(async move { load_credential(&key) })
    }

    fn store(&self, key: String, secret: String) -> AutomationFuture<()> {
        Box::pin(async move { store_credential(&key, secret) })
    }

    fn remove(&self, key: String) -> AutomationFuture<()> {
        Box::pin(async move { remove_credential(&key) })
    }
}

fn load_credential(key: &str) -> Result<Option<String>, AutomationError> {
    let target = wide_target(key)?;
    let mut raw: *mut CREDENTIALW = ptr::null_mut();
    // SAFETY: target is NUL-terminated, raw is an out pointer, and successful memory is freed below.
    let result = unsafe {
        CredReadW(
            PCWSTR(target.as_ptr()),
            CRED_TYPE_GENERIC,
            None,
            &raw mut raw,
        )
    };
    if let Err(error) = result {
        if error.code() == ERROR_NOT_FOUND.to_hresult() {
            return Ok(None);
        }
        return Err(credential_error("credential.load"));
    }
    if raw.is_null() {
        return Err(credential_error("credential.load"));
    }
    // SAFETY: CredReadW returned a valid CREDENTIALW allocation until CredFree.
    let bytes = unsafe {
        std::slice::from_raw_parts(
            (*raw).CredentialBlob.cast_const(),
            (*raw).CredentialBlobSize as usize,
        )
        .to_vec()
    };
    // SAFETY: raw was allocated by CredReadW and has not yet been freed.
    unsafe { CredFree(raw.cast()) };
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| credential_error("credential.load"))
}

fn store_credential(key: &str, mut secret: String) -> Result<(), AutomationError> {
    let mut target = wide_target(key)?;
    let size = u32::try_from(secret.len()).map_err(|_| credential_error("credential.store"))?;
    let mut credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_mut_ptr()),
        CredentialBlobSize: size,
        CredentialBlob: secret.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    // SAFETY: all pointers remain valid for the duration of the synchronous API call.
    unsafe { CredWriteW(&raw mut credential, 0) }.map_err(|_| credential_error("credential.store"))
}

fn remove_credential(key: &str) -> Result<(), AutomationError> {
    let target = wide_target(key)?;
    // SAFETY: target is a valid NUL-terminated UTF-16 string.
    match unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None) } {
        Ok(()) => Ok(()),
        Err(error) if error.code() == ERROR_NOT_FOUND.to_hresult() => Ok(()),
        Err(_) => Err(credential_error("credential.remove")),
    }
}

fn wide_target(key: &str) -> Result<Vec<u16>, AutomationError> {
    if key.trim().is_empty() || key.contains('\0') {
        return Err(AutomationError::new(
            AutomationErrorKind::InvalidInput,
            "credential.key",
            false,
            "The credential key is invalid",
        ));
    }
    Ok(format!("{TARGET_PREFIX}{key}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect())
}

fn credential_error(operation: &'static str) -> AutomationError {
    AutomationError::new(
        AutomationErrorKind::Authorization,
        operation,
        true,
        "Windows Credential Manager could not complete the operation",
    )
}

#[cfg(test)]
mod tests {
    use super::WindowsCredentialStore;

    #[test]
    fn debug_output_has_no_secret_state() {
        assert_eq!(
            format!("{WindowsCredentialStore:?}"),
            "WindowsCredentialStore"
        );
    }
}
