use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// State needed to decide what a keypress means.
///
/// `pending_g` tracks whether the previous key was an unconsumed `g`,
/// so a following `g` completes the `gg` "scroll to top" sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppState {
    pub pending_g: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    LineDown,
    LineUp,
    PageDown,
    HalfPageUp,
    Top,
    Bottom,
    Quit,
    None,
}

/// Pure decision function: given the current state and a keypress, what
/// action should the app take? Contains no side effects, so it's testable
/// without a terminal.
pub fn handle_key(state: &AppState, key: KeyEvent) -> Action {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Action::LineDown,
        KeyCode::Char('k') | KeyCode::Up => Action::LineUp,
        KeyCode::Char(' ') | KeyCode::PageDown => Action::PageDown,
        KeyCode::Char('d') if ctrl => Action::PageDown,
        KeyCode::Char('u') if ctrl => Action::HalfPageUp,
        KeyCode::PageUp => Action::HalfPageUp,
        KeyCode::Char('G') => Action::Bottom,
        KeyCode::End => Action::Bottom,
        KeyCode::Char('g') if state.pending_g => Action::Top,
        KeyCode::Char('g') => Action::None,
        KeyCode::Home => Action::Top,
        KeyCode::Char('c') if ctrl => Action::Quit,
        KeyCode::Char('q') => Action::Quit,
        _ => Action::None,
    }
}

/// Owns the mutable pager state: current scroll offset, viewport height,
/// the document's total row count, and the `gg`-sequence flag.
///
/// Content-agnostic on purpose: it only knows row counts, not what's in
/// them, so the same scroll/quit model works for both the raw-text
/// scaffold and the real rendered document.
pub struct App {
    pub scroll: usize,
    pub viewport_height: usize,
    pub total_rows: usize,
    pending_g: bool,
}

impl App {
    pub fn new(total_rows: usize) -> Self {
        Self {
            scroll: 0,
            viewport_height: 0,
            total_rows,
            pending_g: false,
        }
    }

    fn max_scroll(&self) -> usize {
        self.total_rows.saturating_sub(self.viewport_height.max(1))
    }

    /// Handles a keypress: decides the action, applies it, and updates the
    /// `gg`-sequence flag. Returns `true` if the app should quit.
    pub fn on_key(&mut self, key: KeyEvent) -> bool {
        let state = AppState {
            pending_g: self.pending_g,
        };
        let action = handle_key(&state, key);
        self.pending_g = matches!(key.code, KeyCode::Char('g')) && !self.pending_g;

        match action {
            Action::LineDown => self.scroll = (self.scroll + 1).min(self.max_scroll()),
            Action::LineUp => self.scroll = self.scroll.saturating_sub(1),
            Action::PageDown => {
                self.scroll = (self.scroll + self.viewport_height.max(1)).min(self.max_scroll())
            }
            Action::HalfPageUp => {
                self.scroll = self.scroll.saturating_sub(self.viewport_height.max(1) / 2)
            }
            Action::Top => self.scroll = 0,
            Action::Bottom => self.scroll = self.max_scroll(),
            Action::Quit => return true,
            Action::None => {}
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    fn state(pending_g: bool) -> AppState {
        AppState { pending_g }
    }

    #[test]
    fn line_down_on_j_and_down_arrow() {
        assert_eq!(
            handle_key(&state(false), key(KeyCode::Char('j'))),
            Action::LineDown
        );
        assert_eq!(
            handle_key(&state(false), key(KeyCode::Down)),
            Action::LineDown
        );
    }

    #[test]
    fn line_up_on_k_and_up_arrow() {
        assert_eq!(
            handle_key(&state(false), key(KeyCode::Char('k'))),
            Action::LineUp
        );
        assert_eq!(handle_key(&state(false), key(KeyCode::Up)), Action::LineUp);
    }

    #[test]
    fn page_down_on_space_ctrl_d_and_pagedown() {
        assert_eq!(
            handle_key(&state(false), key(KeyCode::Char(' '))),
            Action::PageDown
        );
        assert_eq!(
            handle_key(&state(false), ctrl_key(KeyCode::Char('d'))),
            Action::PageDown
        );
        assert_eq!(
            handle_key(&state(false), key(KeyCode::PageDown)),
            Action::PageDown
        );
    }

    #[test]
    fn half_page_up_on_ctrl_u_and_pageup() {
        assert_eq!(
            handle_key(&state(false), ctrl_key(KeyCode::Char('u'))),
            Action::HalfPageUp
        );
        assert_eq!(
            handle_key(&state(false), key(KeyCode::PageUp)),
            Action::HalfPageUp
        );
    }

    #[test]
    fn top_on_gg_and_home() {
        // First 'g' with no pending sequence is consumed silently.
        assert_eq!(
            handle_key(&state(false), key(KeyCode::Char('g'))),
            Action::None
        );
        // Second 'g' with a pending sequence completes "gg".
        assert_eq!(
            handle_key(&state(true), key(KeyCode::Char('g'))),
            Action::Top
        );
        assert_eq!(handle_key(&state(false), key(KeyCode::Home)), Action::Top);
    }

    #[test]
    fn bottom_on_shift_g_and_end() {
        assert_eq!(
            handle_key(&state(false), key(KeyCode::Char('G'))),
            Action::Bottom
        );
        assert_eq!(handle_key(&state(false), key(KeyCode::End)), Action::Bottom);
    }

    #[test]
    fn quit_on_q_and_ctrl_c() {
        assert_eq!(
            handle_key(&state(false), key(KeyCode::Char('q'))),
            Action::Quit
        );
        assert_eq!(
            handle_key(&state(false), ctrl_key(KeyCode::Char('c'))),
            Action::Quit
        );
    }

    #[test]
    fn unrecognized_key_is_a_no_op() {
        assert_eq!(
            handle_key(&state(false), key(KeyCode::Char('z'))),
            Action::None
        );
    }

    #[test]
    fn app_gg_sequence_scrolls_to_top() {
        let mut app = App::new(100);
        app.viewport_height = 10;
        app.scroll = 50;

        assert!(!app.on_key(key(KeyCode::Char('g'))));
        assert_eq!(app.scroll, 50, "first g should not move the scroll yet");

        assert!(!app.on_key(key(KeyCode::Char('g'))));
        assert_eq!(app.scroll, 0, "second g completes gg and scrolls to top");
    }

    #[test]
    fn app_scroll_clamps_to_bounds() {
        let mut app = App::new(10);
        app.viewport_height = 5;

        app.on_key(key(KeyCode::Char('k')));
        assert_eq!(app.scroll, 0, "scrolling up from the top stays at 0");

        app.on_key(key(KeyCode::Char('G')));
        assert_eq!(app.scroll, app.max_scroll(), "G scrolls to the max offset");

        app.on_key(key(KeyCode::Char('j')));
        assert_eq!(
            app.scroll,
            app.max_scroll(),
            "scrolling down from the bottom stays clamped"
        );
    }

    #[test]
    fn app_quit_returns_true() {
        let mut app = App::new(1);
        assert!(app.on_key(key(KeyCode::Char('q'))));
    }
}
