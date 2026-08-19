use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use notify_debouncer_full::notify::RecommendedWatcher;
use notify_debouncer_full::{Debouncer, RecommendedCache};
use ratatui::crossterm::event::{self, KeyEvent, KeyEventKind};

use crate::watch;

/// Unified event stream for the app's main loop.
///
/// Later slices add variants here (background image-decode completions)
/// without needing to rework the channel itself.
#[derive(Debug, Clone)]
pub enum Event {
    Key(KeyEvent),
    /// The terminal was resized; the app re-queries the current size on
    /// its next redraw rather than carrying the new size here.
    Resize,
    /// The watched file changed on disk (already debounced by `watch`).
    FileChanged,
}

/// Creates the single channel every event source feeds into. Sources get
/// a clone of the sender; the main loop owns the receiver.
pub fn channel() -> (mpsc::Sender<Event>, mpsc::Receiver<Event>) {
    mpsc::channel()
}

/// Every event source the main loop reads from, kept alive as one value:
/// dropping the file watcher stops auto-reload, so it lives here for as
/// long as the receiver does.
pub struct Sources {
    receiver: mpsc::Receiver<Event>,
    _watcher: Option<Debouncer<RecommendedWatcher, RecommendedCache>>,
}

impl Sources {
    /// Blocks until the next event from any source. `Err` means every
    /// sender is gone, which the main loop treats as "quit".
    pub fn recv(&self) -> Result<Event, mpsc::RecvError> {
        self.receiver.recv()
    }
}

/// Starts the input thread and the file watcher on the shared channel.
///
/// Call this *before* entering the alternate screen: a watcher that can't
/// start is reported on stderr and the viewer runs without auto-reload
/// (manual `r` still works), and that warning has to land somewhere the
/// user can actually see it.
pub fn spawn_sources(path: &Path) -> Sources {
    let (sender, receiver) = channel();
    let watcher = match watch::spawn(path, sender.clone()) {
        Ok(watcher) => Some(watcher),
        Err(error) => {
            eprintln!("mdview: auto-reload unavailable: {error:#}");
            None
        }
    };
    spawn_crossterm_source(sender);
    Sources {
        receiver,
        _watcher: watcher,
    }
}

/// Spawns a background thread polling crossterm for input and forwards it
/// as [`Event`]s over the shared channel.
fn spawn_crossterm_source(tx: mpsc::Sender<Event>) {
    thread::spawn(move || {
        loop {
            match event::poll(Duration::from_millis(250)) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(_) => break,
            }
            let sent = match event::read() {
                // Windows reports both press and release; only act on press
                // so a single keystroke doesn't trigger the action twice.
                Ok(event::Event::Key(key)) if key.kind == KeyEventKind::Press => {
                    tx.send(Event::Key(key)).is_ok()
                }
                Ok(event::Event::Key(_)) => true,
                Ok(event::Event::Resize(_, _)) => tx.send(Event::Resize).is_ok(),
                Ok(_) => true,
                Err(_) => false,
            };
            if !sent {
                break;
            }
        }
    });
}
