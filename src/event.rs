use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use notify_debouncer_full::notify::RecommendedWatcher;
use notify_debouncer_full::{Debouncer, RecommendedCache};
use ratatui::crossterm::event::{self, KeyEvent, KeyEventKind};
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::thread::ResizeResponse;

use crate::image::ImageId;
use crate::watch;

/// Unified event stream for the app's main loop.
///
/// Later slices add variants here (background image-decode completions)
/// without needing to rework the channel itself.
pub enum Event {
    Key(KeyEvent),
    /// The terminal was resized; the app re-queries the current size on
    /// its next redraw rather than carrying the new size here.
    Resize,
    /// The watched file changed on disk (already debounced by `watch`).
    FileChanged,
    /// The image worker finished decoding an image — or found it
    /// undecodable, in which case `protocol` is `None` and the block
    /// keeps its placeholder.
    ImageReady {
        block_id: ImageId,
        protocol: Option<Box<StatefulProtocol>>,
    },
    /// The image worker finished re-encoding an image for a new area.
    ImageResized {
        block_id: ImageId,
        response: Box<ResizeResponse>,
    },
}

/// Creates the single channel every event source feeds into. Sources get
/// a clone of the sender; the main loop owns the receiver.
pub fn channel() -> (mpsc::Sender<Event>, mpsc::Receiver<Event>) {
    mpsc::channel()
}

/// A file watcher running on a channel nobody reads from yet.
///
/// The two halves are separate types so the startup order can't be got
/// wrong: the watcher has to start before the alternate screen (its
/// failure warning must reach the real terminal), while the keyboard
/// reader has to start *after* anything that queries the terminal and
/// waits for a reply on stdin — image capability detection — or it will
/// swallow the reply. Only [`Sources`] can be read from, and the only way
/// to get one is to start the input thread.
pub struct PendingSources {
    sender: mpsc::Sender<Event>,
    receiver: mpsc::Receiver<Event>,
    watcher: Option<Debouncer<RecommendedWatcher, RecommendedCache>>,
}

impl PendingSources {
    /// Starts watching `path`. A watcher that can't start is reported on
    /// stderr and the viewer runs without auto-reload — manual `r` still
    /// works.
    pub fn watching(path: &Path) -> Self {
        let (sender, receiver) = channel();
        let watcher = match watch::spawn(path, sender.clone()) {
            Ok(watcher) => Some(watcher),
            Err(error) => {
                eprintln!("mdview: auto-reload unavailable: {error:#}");
                None
            }
        };
        Self {
            sender,
            receiver,
            watcher,
        }
    }

    /// A handle for another source — the image worker — to answer on.
    pub fn sender(&self) -> mpsc::Sender<Event> {
        self.sender.clone()
    }

    /// Starts reading the keyboard, yielding the sources the main loop
    /// consumes.
    pub fn start_input(self) -> Sources {
        spawn_crossterm_source(self.sender);
        Sources {
            receiver: self.receiver,
            _watcher: self.watcher,
        }
    }
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
