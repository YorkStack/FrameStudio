//! Bounded, single-flight access to a potentially stalled OS credential service.
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use std::{
    fs::{File, OpenOptions},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

#[derive(Default)]
struct Gate {
    active: AtomicBool,
}
struct Permit(Arc<Gate>);
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.active.store(false, Ordering::Release);
    }
}
static GATE: OnceLock<Arc<Gate>> = OnceLock::new();

pub(super) fn run<T: Send + 'static>(
    timeout: Duration,
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    bounded(
        GATE.get_or_init(|| Arc::new(Gate::default())).clone(),
        timeout,
        move || {
            let _lock = process_lock()?;
            operation()
        },
    )
}
fn bounded<T: Send + 'static>(
    gate: Arc<Gate>,
    timeout: Duration,
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    ensure!(
        gate.active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok(),
        "Ein Schlüsselbundzugriff läuft noch. Keine weiteren Versuche starten; nach einer Zeitüberschreitung FrameStudio vollständig beenden und neu öffnen."
    );
    let permit = Permit(gate);
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("credential-access".into())
        .spawn(move || {
            let result = operation();
            drop(permit);
            let _ = sender.send(result);
        })
        .context("Schlüsselbundzugriff konnte nicht gestartet werden")?;
    match receiver.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => bail!(
            "Der Betriebssystem-Schlüsselspeicher antwortet nicht rechtzeitig. Ausgang des Zugriffs unklar; nicht erneut speichern. FrameStudio vollständig beenden und den Betriebssystem-Schlüsselspeicher prüfen. Es gibt keinen Klartext-Ersatzspeicher."
        ),
        Err(_) => bail!("Schlüsselbundzugriff wurde unterbrochen; Ergebnis nicht bestätigt"),
    }
}
fn process_lock() -> Result<File> {
    let dirs = directories::ProjectDirs::from("com", "YorkStack", "FrameStudio")
        .context("Lokales Zugriffsverzeichnis fehlt")?;
    lock_at(&dirs.cache_dir().join("credential-access"))
}
fn lock_at(root: &std::path::Path) -> Result<File> {
    std::fs::create_dir_all(root).context("Schlüsselbund-Zugriffssperre nicht verfügbar")?;
    ensure!(
        !std::fs::symlink_metadata(root)?.file_type().is_symlink(),
        "Unsichere Schlüsselbund-Zugriffssperre"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))?;
    }
    let path = root.join("operation.lock");
    ensure!(
        !std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()),
        "Unsichere Schlüsselbund-Zugriffssperre"
    );
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock_exclusive().context("Eine andere FrameStudio-Instanz greift auf den Schlüsselbund zu. Erst diesen Zugriff abschließen oder die betroffene Instanz beenden.")?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stalled_access_times_out_without_spawning_repeated_operations() {
        let gate = Arc::new(Gate::default());
        let (release, wait) = mpsc::channel();
        let started = std::time::Instant::now();
        let result = bounded(gate.clone(), Duration::from_millis(30), move || {
            wait.recv().unwrap();
            Ok(())
        });
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
        let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        for _ in 0..3 {
            let attempts = attempts.clone();
            assert!(
                bounded(gate.clone(), Duration::from_secs(1), move || {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .is_err()
            );
        }
        assert_eq!(attempts.load(Ordering::SeqCst), 0);
        release.send(()).unwrap();
        for _ in 0..100 {
            if !gate.active.load(Ordering::Acquire) {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(bounded(gate, Duration::from_secs(1), || Ok(())).is_ok());
    }
    #[test]
    fn operation_errors_release_gate_without_losing_the_error() {
        let gate = Arc::new(Gate::default());
        let err = bounded::<()>(gate.clone(), Duration::from_secs(1), || {
            bail!("Sicherer Speicher gesperrt")
        })
        .unwrap_err();
        assert_eq!(err.to_string(), "Sicherer Speicher gesperrt");
        assert!(bounded(gate, Duration::from_secs(1), || Ok(())).is_ok());
    }
    #[test]
    fn cross_process_lock_rejects_second_owner_and_releases_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let file = lock_at(dir.path()).unwrap();
        assert!(lock_at(dir.path()).is_err());
        drop(file);
        assert!(lock_at(dir.path()).is_ok());
    }
}
