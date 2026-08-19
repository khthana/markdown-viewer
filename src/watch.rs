use std::ffi::OsStr;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::Duration;

use anyhow::Context;
use notify_debouncer_full::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

use crate::event::Event;

/// How long to wait for a burst of filesystem events to settle before
/// reporting a single change. Editors write, rename, and touch metadata
/// in quick succession on one save.
const DEBOUNCE: Duration = Duration::from_millis(200);

/// Watches `path` for changes, sending a debounced [`Event::FileChanged`]
/// down `tx` for each save.
///
/// The watch is registered on the file's **parent directory**
/// (non-recursive) rather than the file itself: most editors save via
/// write-temp-then-rename or delete-and-recreate, which detaches a
/// file-level watch after the first save. Events are then filtered back
/// down to the target by file name.
///
/// The returned debouncer must be kept alive — dropping it stops the
/// watch.
pub fn spawn(
    path: &Path,
    tx: Sender<Event>,
) -> anyhow::Result<Debouncer<RecommendedWatcher, RecommendedCache>> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("could not resolve path: {}", path.display()))?;
    let file_name = canonical
        .file_name()
        .map(ToOwned::to_owned)
        .with_context(|| format!("not a file: {}", path.display()))?;
    let directory = canonical
        .parent()
        .map(Path::to_path_buf)
        .with_context(|| format!("no parent directory for: {}", path.display()))?;

    let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
        // Watch errors (e.g. a transiently missing directory) are dropped
        // rather than killing the viewer — the file is still readable and
        // `r` remains available as a manual fallback.
        let Ok(events) = result else { return };
        let touched = events.iter().any(|debounced| {
            debounced
                .paths
                .iter()
                .any(|p| event_matches_target(p, &file_name))
        });
        if touched {
            let _ = tx.send(Event::FileChanged);
        }
    })
    .context("could not start the file watcher")?;

    debouncer
        .watch(&directory, RecursiveMode::NonRecursive)
        .with_context(|| format!("could not watch directory: {}", directory.display()))?;

    Ok(debouncer)
}

/// Whether a file-watch event refers to the file being viewed.
pub fn event_matches_target(event_path: &Path, target_file_name: &OsStr) -> bool {
    event_path.file_name() == Some(target_file_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn matches_an_event_for_the_watched_file() {
        assert!(event_matches_target(
            &PathBuf::from(r"C:\notes\readme.md"),
            OsStr::new("readme.md")
        ));
    }

    #[test]
    fn ignores_events_for_other_files_in_the_watched_directory() {
        // The watch is registered on the parent directory, so sibling
        // files (including editors' atomic-save temp files) show up here
        // and must not trigger a reload.
        assert!(!event_matches_target(
            &PathBuf::from(r"C:\notes\other.md"),
            OsStr::new("readme.md")
        ));
        assert!(!event_matches_target(
            &PathBuf::from(r"C:\notes\readme.md~RF1a2b3.TMP"),
            OsStr::new("readme.md")
        ));
    }

    #[test]
    fn matches_regardless_of_how_the_event_path_is_spelled() {
        // Editors that delete-and-recreate produce paths that can't be
        // canonicalized at event time, so matching is by file name.
        assert!(event_matches_target(
            &PathBuf::from("readme.md"),
            OsStr::new("readme.md")
        ));
        assert!(event_matches_target(
            &PathBuf::from(r".\sub\readme.md"),
            OsStr::new("readme.md")
        ));
    }

    /// Not part of the normal suite: it touches the real filesystem and
    /// waits on OS watch events, which are timing-dependent and vary by
    /// platform. Run with `cargo test -- --ignored` to check the watcher
    /// end to end.
    #[test]
    #[ignore = "filesystem timing; run manually with --ignored"]
    fn reports_a_save_to_the_watched_file_and_ignores_its_siblings() {
        use std::sync::mpsc::RecvTimeoutError;

        let dir = std::env::temp_dir().join(format!("mdview-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("readme.md");
        std::fs::write(&target, "# One\n").unwrap();

        let (tx, rx) = crate::event::channel();
        let _debouncer = spawn(&target, tx).unwrap();

        std::fs::write(&target, "# One\n\nEdited.\n").unwrap();
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(5)),
            Ok(Event::FileChanged)
        ));

        std::fs::write(dir.join("other.md"), "# Unrelated\n").unwrap();
        assert!(
            matches!(
                rx.recv_timeout(Duration::from_secs(2)),
                Err(RecvTimeoutError::Timeout)
            ),
            "a sibling file's save must not trigger a reload"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
