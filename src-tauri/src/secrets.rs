//! OS credential storage or explicitly supplied process-only session keys. No file fallback.
#[cfg(not(target_os = "macos"))]
use anyhow::Context;
use anyhow::{Result, anyhow, bail, ensure};
use serde_json::{Value, json};
const SERVICE: &str = "com.yorkstack.framestudio";

fn account(provider: &str) -> Result<&'static str> {
    match provider {
        // Keep the existing Gemini account: upgrading must not lose the user's key.
        "gemini" => Ok("gemini-api-key"),
        "openai" => Ok("openai-api-key"),
        "anthropic" => Ok("anthropic-api-key"),
        _ => bail!("Unbekannter Schlüsselanbieter"),
    }
}
mod access;
#[cfg(target_os = "macos")]
mod macos;
mod session;
const WRITE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);
const STATUS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

#[cfg(not(target_os = "macos"))]
fn entry(service: &str, account: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(service, account).context("Sicherer Schlüsselspeicher nicht verfügbar")
}
fn read_at(service: String, account: String) -> Result<String> {
    access::run(WRITE_TIMEOUT, move || {
        #[cfg(target_os = "macos")]
        {
            macos::read(&service, &account)
        }
        #[cfg(not(target_os = "macos"))]
        {
            entry(&service, &account)?.get_password().context(
                "Anbieterschlüssel fehlt oder Betriebssystem-Schlüsselspeicher ist gesperrt",
            )
        }
    })
}
fn write_at(service: String, account: String, key: String) -> Result<()> {
    access::run(WRITE_TIMEOUT, move || {
        #[cfg(target_os = "macos")]
        {
            macos::write(&service, &account, &key)?;
            let stored = zeroize::Zeroizing::new(macos::read(&service, &account).map_err(|e| anyhow!("Schreibvorgang gemeldet, aber anschließende Leseprüfung fehlgeschlagen: {e}. Speicherung nicht als nutzbar bestätigt."))?);
            ensure!(
                *stored == key,
                "Schreibvorgang gemeldet, aber Leseprüfung stimmt nicht überein; Speicherung nicht als nutzbar bestätigt"
            );
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            let entry = entry(&service, &account)?;
            entry
                .set_password(&key)
                .context("Schlüssel konnte nicht sicher gespeichert werden")?;
            let stored = zeroize::Zeroizing::new(entry.get_password().context(
                "Schreibvorgang gemeldet, aber anschließende Leseprüfung fehlgeschlagen",
            )?);
            ensure!(
                *stored == key,
                "Schreibvorgang gemeldet, aber Leseprüfung stimmt nicht überein"
            );
            Ok(())
        }
    })
}
fn delete_at(service: String, account: String) -> Result<()> {
    access::run(WRITE_TIMEOUT, move || {
        #[cfg(target_os = "macos")]
        {
            macos::delete(&service, &account)
        }
        #[cfg(not(target_os = "macos"))]
        {
            match entry(&service, &account)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(_) => bail!("Schlüssel konnte nicht gelöscht werden"),
            }
        }
    })
}
pub fn provider_key_for(provider: &str) -> Result<zeroize::Zeroizing<String>> {
    account(provider)?;
    if let Some(key) = session::get(provider)? {
        return Ok(key);
    }
    read_at(SERVICE.into(), account(provider)?.into()).map(zeroize::Zeroizing::new)
}
/// A queued session-bound job must not fall back to the OS store even if its
/// session key is removed between claiming the job and preparing the request.
pub(crate) fn provider_key_for_job(
    provider: &str,
    session_required: bool,
) -> Result<zeroize::Zeroizing<String>> {
    account(provider)?;
    if session_required {
        return session::get(provider)?.ok_or_else(|| anyhow!("Sitzungsschlüssel nicht mehr aktiv. In den Einstellungen erneut nur für diese Sitzung eingeben; kein Schlüsselbundzugriff ausgeführt."));
    }
    provider_key_for(provider)
}
/// Explicit local readiness check. The secret is zeroized and never returned to the GUI.
/// This does not contact the provider or assert that the key is valid with the provider.
pub fn check_provider_key_for(provider: &str) -> Result<Value> {
    let key = provider_key_for(provider)?;
    validate_key(provider, &key)?;
    Ok(json!({"provider":provider,"readable":true,"provider_contacted":false}))
}
/// Explicit OS-store diagnostic, even when an active session key takes precedence.
/// No credential is returned and no authorization dialog is allowed.
pub fn check_stored_provider_key_for(provider: &str) -> Result<Value> {
    let key = zeroize::Zeroizing::new(read_at(SERVICE.into(), account(provider)?.into())?);
    validate_key(provider, &key)?;
    Ok(json!({"provider":provider,"source":"keychain","readable":true,"provider_contacted":false}))
}
/// These explicit local-only operations never access the OS credential store.
pub fn set_session_key_for(provider: &str, key: &str) -> Result<Value> {
    validate_key(provider, key)?;
    session::set(provider, key)?;
    session_key_status()
}
pub fn remove_session_key_for(provider: &str) -> Result<Value> {
    account(provider)?;
    session::remove(provider)?;
    session_key_status()
}
pub fn session_key_status() -> Result<Value> {
    Ok(json!({"providers":session::presence()?}))
}
pub fn provider_key() -> Result<zeroize::Zeroizing<String>> {
    provider_key_for("gemini")
}
#[derive(Debug, PartialEq, Clone, Copy)]
enum CredentialStatus {
    Configured,
    Session,
    Missing,
    Unavailable,
}
impl CredentialStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Configured => "configured",
            Self::Session => "session",
            Self::Missing => "missing",
            Self::Unavailable => "unavailable",
        }
    }
}

// Only metadata is requested; the app never reads a secret merely to show status.
#[cfg(target_os = "macos")]
fn credential_status(service: &str, account: &str) -> CredentialStatus {
    match macos::status(service, account) {
        Ok(true) => CredentialStatus::Configured,
        Ok(false) => CredentialStatus::Missing,
        Err(_) => CredentialStatus::Unavailable,
    }
}
#[cfg(target_os = "windows")]
fn credential_status(service: &str, account: &str) -> CredentialStatus {
    // The Windows store implements get_attributes directly using CredReadW;
    // unlike the Apple store, this does not fall back to get_secret.
    match keyring::Entry::new(service, account).and_then(|entry| entry.inner.get_attributes()) {
        Ok(_) => CredentialStatus::Configured,
        Err(keyring::Error::NoEntry) => CredentialStatus::Missing,
        Err(_) => CredentialStatus::Unavailable,
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn credential_status(_service: &str, _account: &str) -> CredentialStatus {
    // Do not assume another store's default metadata implementation is secret-free.
    CredentialStatus::Unavailable
}
fn status_value(status: CredentialStatus) -> Value {
    json!({"configured":matches!(status, CredentialStatus::Configured | CredentialStatus::Session),"status":status.as_str(),"store":if status == CredentialStatus::Session { "Session memory" } else { "OS Keychain / Credential Manager" }})
}
pub fn key_status() -> Value {
    let present = session::presence().unwrap_or_default();
    let statuses = access::run(STATUS_TIMEOUT, move || {
        Ok([
            if present.get("gemini") == Some(&true) {
                CredentialStatus::Session
            } else {
                credential_status(SERVICE, account("gemini")?)
            },
            if present.get("openai") == Some(&true) {
                CredentialStatus::Session
            } else {
                credential_status(SERVICE, account("openai")?)
            },
            if present.get("anthropic") == Some(&true) {
                CredentialStatus::Session
            } else {
                credential_status(SERVICE, account("anthropic")?)
            },
        ])
    })
    .unwrap_or([
        CredentialStatus::Unavailable,
        CredentialStatus::Unavailable,
        CredentialStatus::Unavailable,
    ]);
    let current = session::presence().unwrap_or_default();
    let [gemini, openai, anthropic] = std::array::from_fn(|i| {
        let provider = ["gemini", "openai", "anthropic"][i];
        if current.get(provider) == Some(&true) {
            status_value(CredentialStatus::Session)
        } else {
            status_value(if statuses[i] == CredentialStatus::Session {
                CredentialStatus::Unavailable
            } else {
                statuses[i]
            })
        }
    });
    json!({"configured":gemini["configured"],"store":gemini["store"],"status":gemini["status"],"providers":{"gemini":gemini,"openai":openai,"anthropic":anthropic}})
}
fn validate_key(provider: &str, key: &str) -> Result<()> {
    account(provider)?;
    ensure!(
        (12..=4096).contains(&key.len()) && !key.chars().any(char::is_whitespace),
        "Ungültiges Schlüsselformat"
    );
    Ok(())
}
pub fn set_provider_key_for(provider: &str, key: &str) -> Result<()> {
    validate_key(provider, key)?;
    write_at(SERVICE.into(), account(provider)?.into(), key.into())
}
pub fn set_provider_key(key: &str) -> Result<()> {
    set_provider_key_for("gemini", key)
}
pub fn delete_provider_key_for(provider: &str) -> Result<()> {
    delete_at(SERVICE.into(), account(provider)?.into())
}
pub fn delete_provider_key() -> Result<()> {
    delete_provider_key_for("gemini")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "Explicit native acceptance; only a fresh synthetic entry is created and removed"]
    fn verified_write_roundtrip_in_os_store() {
        let service = format!(
            "com.yorkstack.framestudio.verified-test-{}",
            uuid::Uuid::new_v4()
        );
        let account = "synthetic-key".to_string();
        let result = write_at(
            service.clone(),
            account.clone(),
            "synthetic-verified-storage-value".into(),
        );
        let cleanup = delete_at(service, account);
        result.unwrap();
        cleanup.unwrap();
    }
    #[test]
    fn provider_accounts_are_distinct_and_gemini_remains_compatible() {
        assert_eq!(account("gemini").unwrap(), "gemini-api-key");
        assert_eq!(account("openai").unwrap(), "openai-api-key");
        assert_ne!(account("gemini").unwrap(), account("openai").unwrap());
        for provider in ["test", "", "../gemini", "https://evil.example"] {
            assert!(provider_key_for(provider).is_err());
            assert!(set_provider_key_for(provider, "synthetic-never-stored").is_err());
            assert!(delete_provider_key_for(provider).is_err());
        }
    }
    #[test]
    fn invalid_keys_fail_before_storage_and_never_echo_input() {
        for provider in ["gemini", "openai", "anthropic"] {
            for key in ["short", "synthetic secret with spaces", ""] {
                let error = set_provider_key_for(provider, key).unwrap_err().to_string();
                assert_eq!(error, "Ungültiges Schlüsselformat");
            }
        }
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn metadata_status_for_absent_synthetic_service_is_noninteractive() {
        let service = format!("{SERVICE}.absent-test-{}", uuid::Uuid::new_v4());
        let started = std::time::Instant::now();
        assert_eq!(
            access::run(STATUS_TIMEOUT, move || Ok(credential_status(
                &service,
                "synthetic-account"
            )))
            .unwrap(),
            CredentialStatus::Missing
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }
    #[test]
    #[ignore = "Requires native OS credential store; isolated synthetic service, never user credentials"]
    fn native_keychain_roundtrip() {
        let service = format!("{SERVICE}.test-{}", uuid::Uuid::new_v4());
        eprintln!("Isolated synthetic keychain service: {service}");
        for provider in ["gemini", "openai", "anthropic"] {
            let account = account(provider).unwrap().to_owned();
            write_at(
                service.clone(),
                account.clone(),
                "synthetic-not-a-provider-key".into(),
            )
            .unwrap();
            assert!(
                read_at(service.clone(), account.clone()).unwrap()
                    == "synthetic-not-a-provider-key"
            );
            write_at(
                service.clone(),
                account.clone(),
                "synthetic-replacement".into(),
            )
            .unwrap();
            assert!(read_at(service.clone(), account.clone()).unwrap() == "synthetic-replacement");
            delete_at(service.clone(), account.clone()).unwrap();
            assert!(read_at(service.clone(), account).is_err());
        }
    }
}
