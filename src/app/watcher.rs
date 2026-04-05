use std::path::Path;
use std::sync::mpsc;

use notify::Watcher as _;

/// Create a file watcher that monitors `target_path` for changes.
///
/// Watches the parent directory (to catch rename-based atomic saves) and
/// filters events to only those involving `target_path`.
///
/// If `repaint_ctx` is provided, requests an egui repaint on each event
/// so the UI loop wakes up even when the window is unfocused.
///
/// Returns the watcher handle and a receiver channel. The watcher must be
/// kept alive (not dropped) for monitoring to continue.
pub fn watch_file(
    target_path: &Path,
    repaint_ctx: Option<eframe::egui::Context>,
) -> Result<(notify::RecommendedWatcher, mpsc::Receiver<()>), notify::Error> {
    let (tx, rx) = mpsc::channel();
    let sender = std::sync::Mutex::new(tx);
    let filter_path = target_path.to_path_buf();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            use notify::EventKind::*;
            match event.kind {
                Modify(_) | Create(_) | Remove(_) => {
                    if event.paths.iter().any(|p| p == &filter_path) {
                        let _ = sender.lock().unwrap().send(());
                        if let Some(ref ctx) = repaint_ctx {
                            ctx.request_repaint();
                        }
                    }
                }
                _ => {}
            }
        }
    })?;

    let watch_dir = target_path
        .parent()
        .unwrap_or(Path::new("."));
    watcher.watch(watch_dir, notify::RecursiveMode::NonRecursive)?;

    Ok((watcher, rx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    /// Helper: wait for a message on the receiver with a timeout.
    fn recv_timeout(rx: &mpsc::Receiver<()>, timeout: Duration) -> bool {
        rx.recv_timeout(timeout).is_ok()
    }

    #[test]
    fn watch_file_detects_modification() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.json");
        fs::write(&file_path, r#"{"key": "value1"}"#).unwrap();

        let (_watcher, rx) = watch_file(&file_path, None).unwrap();

        // Small delay to let the watcher initialize
        std::thread::sleep(Duration::from_millis(50));

        // Modify the file
        fs::write(&file_path, r#"{"key": "value2"}"#).unwrap();

        assert!(
            recv_timeout(&rx, Duration::from_secs(2)),
            "expected notification after file modification"
        );
    }

    #[test]
    fn watch_file_ignores_other_files_in_same_dir() {
        let dir = tempfile::tempdir().unwrap();
        let watched = dir.path().join("config.json");
        let other = dir.path().join("other.txt");
        fs::write(&watched, "{}").unwrap();
        fs::write(&other, "hello").unwrap();

        let (_watcher, rx) = watch_file(&watched, None).unwrap();

        std::thread::sleep(Duration::from_millis(50));

        // Modify the OTHER file (should NOT trigger)
        fs::write(&other, "world").unwrap();

        assert!(
            !recv_timeout(&rx, Duration::from_millis(500)),
            "should not receive notification for unrelated file"
        );
    }

    #[test]
    fn watch_file_detects_atomic_save() {
        // Simulates VS Code's atomic save: write to temp, then rename over target.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("config.json");
        fs::write(&target, r#"{"v":1}"#).unwrap();

        let (_watcher, rx) = watch_file(&target, None).unwrap();

        std::thread::sleep(Duration::from_millis(50));

        // Atomic save: write to temp file, then rename
        let tmp = dir.path().join("config.json.tmp");
        fs::write(&tmp, r#"{"v":2}"#).unwrap();
        fs::rename(&tmp, &target).unwrap();

        assert!(
            recv_timeout(&rx, Duration::from_secs(2)),
            "expected notification after atomic save (rename)"
        );
    }
}
