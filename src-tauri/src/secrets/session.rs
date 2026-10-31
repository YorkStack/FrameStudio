//! Explicit process lifetime credentials; never serialized or written to any store.
use anyhow::{Result, anyhow};
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
};
use zeroize::Zeroizing;

type Keys = BTreeMap<String, Zeroizing<String>>;
static KEYS: OnceLock<Mutex<Keys>> = OnceLock::new();
fn keys() -> Result<std::sync::MutexGuard<'static, Keys>> {
    KEYS.get_or_init(|| Mutex::new(Keys::new()))
        .lock()
        .map_err(|_| anyhow!("Sitzungsspeicher nicht verfügbar; App vollständig neu starten"))
}
pub(super) fn get(provider: &str) -> Result<Option<Zeroizing<String>>> {
    Ok(keys()?.get(provider).cloned())
}
pub(super) fn set(provider: &str, key: &str) -> Result<()> {
    keys()?.insert(provider.into(), Zeroizing::new(key.into()));
    Ok(())
}
pub(super) fn remove(provider: &str) -> Result<()> {
    keys()?.remove(provider);
    Ok(())
}
pub(super) fn presence() -> Result<BTreeMap<String, bool>> {
    let keys = keys()?;
    Ok(["gemini", "openai", "anthropic"]
        .into_iter()
        .map(|provider| (provider.into(), keys.contains_key(provider)))
        .collect())
}
