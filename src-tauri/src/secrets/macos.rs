//! Native macOS credential operations, with authentication UI disabled for this process.
//! Uses the existing classic-keychain service/accounts; does not migrate or broaden ACLs.
use anyhow::{Context, Result, anyhow, ensure};
use core_foundation::{
    base::{CFType, TCFType},
    boolean::CFBoolean,
    data::CFData,
    dictionary::CFDictionary,
    string::{CFString, CFStringRef},
};
use security_framework_sys::{
    base::{errSecDuplicateItem, errSecItemNotFound},
    item::*,
    keychain::SecKeychainSetUserInteractionAllowed,
    keychain_item::{SecItemAdd, SecItemCopyMatching, SecItemDelete, SecItemUpdate},
};
use std::sync::OnceLock;

type Query = CFDictionary<CFType, CFType>;
static NONINTERACTIVE: OnceLock<i32> = OnceLock::new();
// Exported by the system SDK, omitted by the pinned sys crate.
unsafe extern "C" {
    static kSecUseAuthenticationUIFail: CFStringRef;
}

/// UIFail alone does not cover every classic-keychain operation. Disable optional
/// keychain dialogs in this process before any operation; never weaken item ACLs.
pub(super) fn initialize_noninteractive() -> Result<()> {
    let code = *NONINTERACTIVE.get_or_init(|| {
        // SAFETY: a Boolean argument; this changes only this application's policy.
        unsafe { SecKeychainSetUserInteractionAllowed(0) }
    });
    check(code)
}
fn constant(value: CFStringRef) -> CFType {
    // SAFETY: callers only supply non-null, process-lifetime Security constants.
    unsafe { CFString::wrap_under_get_rule(value).into_CFType() }
}
fn query(service: &str, account: &str) -> Vec<(CFType, CFType)> {
    // SAFETY: Security framework string constants are immutable and retained below.
    unsafe {
        vec![
            (constant(kSecClass), constant(kSecClassGenericPassword)),
            (
                constant(kSecAttrService),
                CFString::new(service).into_CFType(),
            ),
            (
                constant(kSecAttrAccount),
                CFString::new(account).into_CFType(),
            ),
            (
                constant(kSecUseAuthenticationUI),
                constant(kSecUseAuthenticationUIFail),
            ),
        ]
    }
}
fn result_query(service: &str, account: &str, secret: bool) -> Query {
    let mut pairs = query(service, account);
    // SAFETY: all keys are immutable framework constants; booleans are retained.
    unsafe {
        pairs.extend([
            (
                constant(kSecReturnAttributes),
                CFBoolean::from(!secret).into_CFType(),
            ),
            (
                constant(kSecReturnData),
                CFBoolean::from(secret).into_CFType(),
            ),
            (
                constant(kSecReturnRef),
                CFBoolean::false_value().into_CFType(),
            ),
        ]);
    }
    Query::from_CFType_pairs(&pairs)
}
fn copy(query: &Query) -> (i32, Option<CFType>) {
    let mut result = std::ptr::null();
    // SAFETY: query retains every value for the call. A non-null returned object
    // follows the Create Rule and is released by the owned CFType, even on error.
    let code = unsafe { SecItemCopyMatching(query.as_concrete_TypeRef(), &mut result) };
    let result = if result.is_null() {
        None
    } else {
        Some(unsafe { CFType::wrap_under_create_rule(result) })
    };
    (code, result)
}
pub(super) fn status(service: &str, account: &str) -> Result<bool> {
    initialize_noninteractive()?;
    let (code, _) = copy(&result_query(service, account, false));
    if code == errSecItemNotFound {
        return Ok(false);
    }
    check(code)?;
    Ok(true)
}
pub(super) fn read(service: &str, account: &str) -> Result<String> {
    initialize_noninteractive()?;
    let (code, value) = copy(&result_query(service, account, true));
    check(code)?;
    let value = value.context("Schlüsselbund lieferte keine Schlüsseldaten")?;
    let data = value
        .downcast::<CFData>()
        .context("Ungültige Schlüsseldaten im Schlüsselbund")?;
    ensure!(
        (12..=4096).contains(&data.bytes().len()),
        "Ungültige Schlüssellänge im Schlüsselbund"
    );
    String::from_utf8(data.bytes().to_vec())
        .map_err(|_| anyhow!("Ungültige Textkodierung im Schlüsselbund"))
}
pub(super) fn write(service: &str, account: &str, key: &str) -> Result<()> {
    initialize_noninteractive()?;
    let pairs = query(service, account);
    let query = Query::from_CFType_pairs(&pairs);
    // SAFETY: immutable framework key and owned CFData are retained by attributes.
    let attributes = unsafe {
        [
            (
                constant(kSecValueData),
                CFData::from_buffer(key.as_bytes()).into_CFType(),
            ),
            (
                constant(kSecAttrLabel),
                CFString::new(&format!(
                    "FrameStudio · {}",
                    match account {
                        "gemini-api-key" => "Google Gemini API-Schlüssel",
                        "openai-api-key" => "OpenAI API-Schlüssel",
                        "anthropic-api-key" => "Anthropic API-Schlüssel",
                        _ => "Schlüsselspeicher-Test",
                    }
                ))
                .into_CFType(),
            ),
        ]
    };
    let changes = Query::from_CFType_pairs(&attributes);
    // SAFETY: both dictionaries remain alive; query has no return-result keys.
    let code = unsafe { SecItemUpdate(query.as_concrete_TypeRef(), changes.as_concrete_TypeRef()) };
    if code != errSecItemNotFound {
        return check(code);
    }
    let mut addition = pairs;
    addition.extend(attributes);
    let addition = Query::from_CFType_pairs(&addition);
    // SAFETY: dictionary remains alive; no item/reference result is requested.
    let code = unsafe { SecItemAdd(addition.as_concrete_TypeRef(), std::ptr::null_mut()) };
    if code == errSecDuplicateItem {
        // Another authorized local process may have created the same item meanwhile.
        return check(unsafe {
            SecItemUpdate(query.as_concrete_TypeRef(), changes.as_concrete_TypeRef())
        });
    }
    check(code)
}
pub(super) fn delete(service: &str, account: &str) -> Result<()> {
    initialize_noninteractive()?;
    let query = Query::from_CFType_pairs(&query(service, account));
    // SAFETY: a bounded service/account query with no return-result attributes.
    let code = unsafe { SecItemDelete(query.as_concrete_TypeRef()) };
    if code == errSecItemNotFound {
        return Ok(());
    }
    check(code)
}
fn check(code: i32) -> Result<()> {
    match code {
        0 => Ok(()),
        -25300 => Err(anyhow!("Kein Schlüssel hinterlegt")),
        -25308 => Err(anyhow!(
            "macOS benötigt eine Zugriffsfreigabe (Code -25308). Das bedeutet nicht automatisch, dass der Schlüsselbund gesperrt oder dein Passwort falsch ist: Auch ein entsperrter Schlüsselbund kann dieser App den Zugriff auf den Eintrag verweigern. In „Schlüsselbundverwaltung“ (nicht „Passwörter“) die Zugriffssteuerung des Eintrags com.yorkstack.framestudio für die aktuelle FrameStudio-App prüfen. Alternativ in FrameStudio → Einstellungen → Schlüssel verwenden → Nur für diese Sitzung den API-Schlüssel erneut eingeben. Es wurde kein Passwortdialog geöffnet."
        )),
        -25293 => Err(anyhow!(
            "macOS hat die Schlüsselbund-Autorisierung abgelehnt (Code -25293). Die App-Freigabe in der Schlüsselbundverwaltung prüfen oder den API-Schlüssel in FrameStudio nur für diese Sitzung eingeben. Es wurde kein Passwortdialog geöffnet."
        )),
        -25291 => Err(anyhow!(
            "Der macOS-Schlüsselbunddienst ist nicht verfügbar (Code -25291). Später erneut prüfen oder den API-Schlüssel in FrameStudio nur für diese Sitzung eingeben. Es wurde kein Passwortdialog geöffnet."
        )),
        -128 => Err(anyhow!(
            "macOS hat den Schlüsselbundzugriff abgebrochen (Code -128). Erneut gezielt prüfen oder den API-Schlüssel in FrameStudio nur für diese Sitzung eingeben. Es wurde kein Passwortdialog geöffnet."
        )),
        _ => Err(anyhow!(
            "Sicherer Schlüsselbundzugriff fehlgeschlagen (macOS-Code {code}); keine Klartextablage"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn query_has_exact_identity_and_never_requests_secrets_for_status() {
        let status = result_query("synthetic-service", "synthetic-account", false);
        let read = result_query("synthetic-service", "synthetic-account", true);
        unsafe {
            assert_eq!(
                *status.get(constant(kSecAttrService)),
                CFString::new("synthetic-service").into_CFType()
            );
            assert_eq!(
                *status.get(constant(kSecAttrAccount)),
                CFString::new("synthetic-account").into_CFType()
            );
            assert_eq!(
                *status.get(constant(kSecUseAuthenticationUI)),
                constant(kSecUseAuthenticationUIFail)
            );
            assert_eq!(
                *status.get(constant(kSecReturnData)),
                CFBoolean::false_value().into_CFType()
            );
            assert_eq!(
                *status.get(constant(kSecReturnAttributes)),
                CFBoolean::true_value().into_CFType()
            );
            assert_eq!(
                *status.get(constant(kSecReturnRef)),
                CFBoolean::false_value().into_CFType()
            );
            assert_eq!(
                *read.get(constant(kSecReturnData)),
                CFBoolean::true_value().into_CFType()
            );
            assert_eq!(
                *read.get(constant(kSecReturnAttributes)),
                CFBoolean::false_value().into_CFType()
            );
            let mutation =
                Query::from_CFType_pairs(&query("synthetic-service", "synthetic-account"));
            assert!(!mutation.contains_key(&constant(kSecReturnData)));
            assert!(!mutation.contains_key(&constant(kSecReturnAttributes)));
            assert!(!mutation.contains_key(&constant(kSecReturnRef)));
        }
    }
    #[test]
    fn errors_are_classified_without_native_secret_access() {
        assert!(check(0).is_ok());
        assert!(
            check(errSecItemNotFound)
                .unwrap_err()
                .to_string()
                .contains("Kein Schlüssel")
        );
        for code in [-25308, -25293, -25291, -128] {
            assert!(
                check(code)
                    .unwrap_err()
                    .to_string()
                    .contains(&format!("Code {code}"))
            );
        }
        assert!(
            check(-50)
                .unwrap_err()
                .to_string()
                .contains("macOS-Code -50")
        );
    }
    #[test]
    #[ignore = "Explicit native acceptance only; isolated synthetic service; may require an unlocked OS keychain"]
    fn synthetic_native_roundtrip_without_authentication_ui() {
        let service = format!("com.yorkstack.framestudio.test-{}", uuid::Uuid::new_v4());
        let account = "synthetic-key";
        assert!(!status(&service, account).unwrap());
        write(&service, account, "synthetic-keychain-test-value").unwrap();
        assert!(status(&service, account).unwrap());
        assert_eq!(
            read(&service, account).unwrap(),
            "synthetic-keychain-test-value"
        );
        write(&service, account, "synthetic-keychain-test-replaced").unwrap();
        assert_eq!(
            read(&service, account).unwrap(),
            "synthetic-keychain-test-replaced"
        );
        delete(&service, account).unwrap();
        assert!(!status(&service, account).unwrap());
        delete(&service, account).unwrap();
        assert!(read(&service, account).is_err());
    }
}
