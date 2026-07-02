use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ratatui::crossterm::event::{self, KeyEvent, KeyEventKind};

/// Unified event stream for the app's main loop.
///
/// Later slices add variants here (file-watch reload notifications,
/// background image-decode completions) without needing to rework the
/// channel itself.
#[derive(Debug, Clone)]
pub enum Event {
    Key(KeyEvent),
    /// The terminal was resized; the app re-queries the current size on
    /// its next redraw rather than carrying the new size here.
    Resize,
}

/// Spawns a background thread polling crossterm for input and forwards it
/// as [`Event`]s over an mpsc channel.
pub fn spawn_crossterm_source() -> mpsc::Receiver<Event> {
    let (tx, rx) = mpsc::channel();
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
    rx
}
