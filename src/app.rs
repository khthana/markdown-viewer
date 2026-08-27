use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::search;
use crate::toc::TocEntry;

/// Whether keys are routed to normal pager navigation or captured as
/// search-query text input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
}

/// State needed to decide what a keypress means.
///
/// `pending_g` tracks whether the previous key was an unconsumed `g`,
/// so a following `g` completes the `gg` "scroll to top" sequence.
/// `toc_focused` routes Up/Down/Enter to the TOC sidebar instead of the
/// main pane while it's true. `mode` is checked first, ahead of
/// `toc_focused`: while typing a search query, keys are text input
/// regardless of what else has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppState {
    pub pending_g: bool,
    pub toc_focused: bool,
    pub mode: Mode,
    /// While the help overlay is up it swallows every key, so nothing
    /// else in this struct matters until it's dismissed.
    pub help_open: bool,
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
    ToggleToc,
    TocUp,
    TocDown,
    TocJump,
    EnterSearch,
    SearchInput(char),
    SearchBackspace,
    ConfirmSearch,
    ExitSearch,
    NextMatch,
    PrevMatch,
    Reload,
    OpenHelp,
    CloseHelp,
    /// The reader pressed `Esc` in normal mode. Which context that backs
    /// out of depends on what is open, which only `App` knows.
    Cancel,
}

impl Action {
    /// Whether this action is the user driving the search themselves, as
    /// opposed to navigating or reloading.
    fn is_search_step(self) -> bool {
        matches!(
            self,
            Action::EnterSearch
                | Action::ConfirmSearch
                | Action::ExitSearch
                | Action::NextMatch
                | Action::PrevMatch
        )
    }
}

/// Which focus a binding applies in — the same key means different
/// things depending on what has the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The document pane, which is where most keys act.
    Pager,
    /// The outline sidebar, once `Tab` has focused it.
    Outline,
    /// Typing a search query.
    Search,
}

/// One row of the help overlay.
///
/// `checks` is what keeps the overlay honest: every key the row
/// advertises, paired with the action it must produce. Two tests hold the
/// table and `handle_key` to each other — one that every advertised key
/// does what it says, and one that every key the app honours is
/// advertised — so neither can drift from the other.
#[derive(Debug, Clone, Copy)]
pub struct Binding {
    pub keys: &'static str,
    pub description: &'static str,
    pub focus: Focus,
    /// Read only by those tests: a table can't be the dispatch itself,
    /// since `handle_key` also has to answer key sequences (`gg`) and
    /// free text, which don't fit one key to one action.
    #[cfg_attr(not(test), allow(dead_code))]
    checks: &'static [(KeyCode, KeyModifiers, Action)],
}

/// Every key the app honours, in the order the help overlay lists them.
/// The single source of truth for `?`, so what the overlay promises and
/// what the keys do can't drift apart.
pub const KEYBINDINGS: &[Binding] = &[
    Binding {
        keys: "j / Down",
        description: "Scroll down one line",
        focus: Focus::Pager,
        checks: &[
            (KeyCode::Char('j'), KeyModifiers::NONE, Action::LineDown),
            (KeyCode::Down, KeyModifiers::NONE, Action::LineDown),
        ],
    },
    Binding {
        keys: "k / Up",
        description: "Scroll up one line",
        focus: Focus::Pager,
        checks: &[
            (KeyCode::Char('k'), KeyModifiers::NONE, Action::LineUp),
            (KeyCode::Up, KeyModifiers::NONE, Action::LineUp),
        ],
    },
    Binding {
        keys: "Space / PgDn / Ctrl-d",
        description: "Page down",
        focus: Focus::Pager,
        checks: &[
            (KeyCode::Char(' '), KeyModifiers::NONE, Action::PageDown),
            (KeyCode::PageDown, KeyModifiers::NONE, Action::PageDown),
            (KeyCode::Char('d'), KeyModifiers::CONTROL, Action::PageDown),
        ],
    },
    Binding {
        keys: "Ctrl-u / PgUp",
        description: "Half a page up",
        focus: Focus::Pager,
        checks: &[
            (
                KeyCode::Char('u'),
                KeyModifiers::CONTROL,
                Action::HalfPageUp,
            ),
            (KeyCode::PageUp, KeyModifiers::NONE, Action::HalfPageUp),
        ],
    },
    Binding {
        keys: "gg / Home",
        description: "Jump to the top",
        focus: Focus::Pager,
        checks: &[(KeyCode::Home, KeyModifiers::NONE, Action::Top)],
    },
    Binding {
        keys: "G / End",
        description: "Jump to the bottom",
        focus: Focus::Pager,
        checks: &[
            (KeyCode::Char('G'), KeyModifiers::NONE, Action::Bottom),
            (KeyCode::End, KeyModifiers::NONE, Action::Bottom),
        ],
    },
    Binding {
        keys: "Tab",
        description: "Open the outline, or close it once focused",
        focus: Focus::Pager,
        checks: &[(KeyCode::Tab, KeyModifiers::NONE, Action::ToggleToc)],
    },
    Binding {
        keys: "j / k / Up / Down",
        description: "Move through the outline",
        focus: Focus::Outline,
        checks: &[
            (KeyCode::Char('j'), KeyModifiers::NONE, Action::TocDown),
            (KeyCode::Char('k'), KeyModifiers::NONE, Action::TocUp),
            (KeyCode::Down, KeyModifiers::NONE, Action::TocDown),
            (KeyCode::Up, KeyModifiers::NONE, Action::TocUp),
        ],
    },
    Binding {
        keys: "Enter",
        description: "Jump to the selected heading",
        focus: Focus::Outline,
        checks: &[(KeyCode::Enter, KeyModifiers::NONE, Action::TocJump)],
    },
    Binding {
        keys: "Esc",
        description: "Close the outline",
        focus: Focus::Outline,
        checks: &[(KeyCode::Esc, KeyModifiers::NONE, Action::Cancel)],
    },
    Binding {
        keys: "/",
        description: "Search the document",
        focus: Focus::Pager,
        checks: &[(KeyCode::Char('/'), KeyModifiers::NONE, Action::EnterSearch)],
    },
    Binding {
        keys: "Enter",
        description: "Run the query typed so far",
        focus: Focus::Search,
        checks: &[(KeyCode::Enter, KeyModifiers::NONE, Action::ConfirmSearch)],
    },
    Binding {
        keys: "Backspace",
        description: "Delete the last character typed",
        focus: Focus::Search,
        checks: &[(
            KeyCode::Backspace,
            KeyModifiers::NONE,
            Action::SearchBackspace,
        )],
    },
    Binding {
        keys: "Esc",
        description: "Abandon the search",
        focus: Focus::Search,
        checks: &[(KeyCode::Esc, KeyModifiers::NONE, Action::ExitSearch)],
    },
    Binding {
        keys: "Esc",
        description: "Clear the search, or close the outline",
        focus: Focus::Pager,
        checks: &[(KeyCode::Esc, KeyModifiers::NONE, Action::Cancel)],
    },
    Binding {
        keys: "n / N",
        description: "Next / previous match",
        focus: Focus::Pager,
        checks: &[
            (KeyCode::Char('n'), KeyModifiers::NONE, Action::NextMatch),
            (KeyCode::Char('N'), KeyModifiers::NONE, Action::PrevMatch),
        ],
    },
    Binding {
        keys: "r",
        description: "Reload the file now",
        focus: Focus::Pager,
        checks: &[(KeyCode::Char('r'), KeyModifiers::NONE, Action::Reload)],
    },
    Binding {
        keys: "?",
        description: "Show this help; any key closes it",
        focus: Focus::Pager,
        checks: &[(KeyCode::Char('?'), KeyModifiers::NONE, Action::OpenHelp)],
    },
    Binding {
        keys: "q / Ctrl-c",
        description: "Quit",
        focus: Focus::Pager,
        checks: &[
            (KeyCode::Char('q'), KeyModifiers::NONE, Action::Quit),
            (KeyCode::Char('c'), KeyModifiers::CONTROL, Action::Quit),
        ],
    },
];

/// Pure decision function: given the current state and a keypress, what
/// action should the app take? Contains no side effects, so it's testable
/// without a terminal.
pub fn handle_key(state: &AppState, key: KeyEvent) -> Action {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // Ctrl-C is a universal escape hatch, even mid-search.
    if key.code == KeyCode::Char('c') && ctrl {
        return Action::Quit;
    }

    // The overlay covers the document, so the next key is the reader
    // asking for the document back — whichever key it happens to be.
    if state.help_open {
        return Action::CloseHelp;
    }

    // While typing a search query, almost every key is text input rather
    // than a navigation shortcut — otherwise a query containing "q" would
    // quit the app instead of being typed.
    if state.mode == Mode::Search {
        return match key.code {
            KeyCode::Enter => Action::ConfirmSearch,
            KeyCode::Esc => Action::ExitSearch,
            KeyCode::Backspace => Action::SearchBackspace,
            KeyCode::Char(c) => Action::SearchInput(c),
            _ => Action::None,
        };
    }

    // Tab, quit, reload and help are global, regardless of TOC focus.
    match key.code {
        KeyCode::Tab => return Action::ToggleToc,
        KeyCode::Char('q') => return Action::Quit,
        KeyCode::Char('r') => return Action::Reload,
        KeyCode::Char('?') => return Action::OpenHelp,
        KeyCode::Esc => return Action::Cancel,
        _ => {}
    }

    if state.toc_focused {
        return match key.code {
            KeyCode::Up | KeyCode::Char('k') => Action::TocUp,
            KeyCode::Down | KeyCode::Char('j') => Action::TocDown,
            KeyCode::Enter => Action::TocJump,
            _ => Action::None,
        };
    }

    // Matched on the key alone: a terminal reports `G` and `N` as
    // SHIFT-modified, so insisting on no modifiers would break them.
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
        KeyCode::Char('/') => Action::EnterSearch,
        KeyCode::Char('n') => Action::NextMatch,
        KeyCode::Char('N') => Action::PrevMatch,
        _ => Action::None,
    }
}

/// The deepest scroll offset that still leaves the viewport full. Shared
/// with `reload` so the clamp rule can't drift between the pager and the
/// position restored after a reload.
pub fn max_scroll(total_rows: usize, viewport_height: usize) -> usize {
    total_rows.saturating_sub(viewport_height.max(1))
}

/// What the main loop should do after a keypress. `Reload` exists
/// because re-reading the file is I/O the pure `App` state deliberately
/// doesn't own — the caller performs it and hands back a fresh document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOutcome {
    Continue,
    Quit,
    Reload,
}

/// Owns the mutable pager state: current scroll offset, viewport height,
/// the document's total row count, the `gg`-sequence flag, and TOC
/// sidebar state.
///
/// Content-agnostic on purpose: it only knows row counts (and, for the
/// TOC, entries handed in per-call), not what's in them, so the same
/// scroll/quit model works for both the raw-text scaffold and the real
/// rendered document.
pub struct App {
    pub scroll: usize,
    /// Whether the keybinding overlay is covering the document.
    pub help_open: bool,
    pub viewport_height: usize,
    pub total_rows: usize,
    pub toc_open: bool,
    pub toc_focused: bool,
    pub toc_selected: usize,
    pub mode: Mode,
    pub search_query: String,
    /// Whether `Enter` has confirmed a search (as opposed to a query still
    /// being typed, or no search having been started). Drives whether the
    /// UI shows highlights/a match-count vs. nothing.
    pub search_active: bool,
    pub current_match: Option<usize>,
    /// Set when a reload couldn't find the previously selected match and
    /// selection fell back to the first one, so the status line can say
    /// so. Cleared as soon as the user moves the search on.
    pub search_fell_back: bool,
    pending_g: bool,
}

impl App {
    pub fn new(total_rows: usize) -> Self {
        Self {
            scroll: 0,
            help_open: false,
            viewport_height: 0,
            total_rows,
            toc_open: false,
            toc_focused: false,
            toc_selected: 0,
            mode: Mode::Normal,
            search_query: String::new(),
            search_active: false,
            current_match: None,
            search_fell_back: false,
            pending_g: false,
        }
    }

    /// Applies the outcome of re-running the active query after a reload.
    pub fn apply_reselection(&mut self, reselection: search::Reselection) {
        match reselection {
            search::Reselection::Preserved(index) => {
                self.current_match = Some(index);
                self.search_fell_back = false;
            }
            search::Reselection::SelectedFirst => {
                self.current_match = Some(0);
                self.search_fell_back = false;
            }
            search::Reselection::FellBackToFirst => {
                self.current_match = Some(0);
                self.search_fell_back = true;
            }
            search::Reselection::NoMatches => {
                self.current_match = None;
                self.search_fell_back = false;
            }
        }
    }

    fn max_scroll(&self) -> usize {
        max_scroll(self.total_rows, self.viewport_height)
    }

    /// Drops the query, its highlights and its selection, which releases
    /// the status row back to the document. Shared by `Esc` while a query
    /// is being typed and `Esc` once one has been confirmed, so the two
    /// ways out of a search can't leave different state behind.
    ///
    /// It clears `search_fell_back` on a different rule from
    /// `is_search_step`'s: that one says a deliberate step supersedes the
    /// note, this one that the note can't outlive the search it describes.
    /// `Cancel` is deliberately not a search step, so closing the outline
    /// over a fallback-flagged search leaves the note standing.
    fn clear_search(&mut self) {
        self.mode = Mode::Normal;
        self.search_query.clear();
        self.search_active = false;
        self.current_match = None;
        self.search_fell_back = false;
    }

    /// Handles a keypress: decides the action, applies it, and updates the
    /// `gg`-sequence flag. `toc` is the currently resolved TOC entries (for
    /// selection bounds and jump targets); `matches` is the current
    /// search's resolved match list (for jump targets and next/prev
    /// navigation). Returns what the main loop should do next.
    pub fn on_key(
        &mut self,
        key: KeyEvent,
        toc: &[TocEntry],
        matches: &[search::Match],
    ) -> KeyOutcome {
        let state = AppState {
            pending_g: self.pending_g,
            toc_focused: self.toc_focused,
            mode: self.mode,
            help_open: self.help_open,
        };
        let action = handle_key(&state, key);
        self.pending_g = matches!(key.code, KeyCode::Char('g')) && !self.pending_g;
        // Any deliberate search step supersedes the note about a reload
        // having moved the selection.
        if action.is_search_step() {
            self.search_fell_back = false;
        }

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
            Action::OpenHelp => self.help_open = true,
            Action::CloseHelp => self.help_open = false,
            Action::Quit => return KeyOutcome::Quit,
            Action::Reload => return KeyOutcome::Reload,
            // Closed -> open+focused; open+focused -> closed; open+unfocused
            // (right after a jump) -> re-focused. This is what makes "peek,
            // jump, peek again" work without needing to close and reopen.
            Action::ToggleToc => {
                if !self.toc_open {
                    self.toc_open = true;
                    self.toc_focused = true;
                    self.toc_selected = self.toc_selected.min(toc.len().saturating_sub(1));
                } else if self.toc_focused {
                    self.toc_open = false;
                    self.toc_focused = false;
                } else {
                    self.toc_focused = true;
                }
            }
            Action::TocUp => self.toc_selected = self.toc_selected.saturating_sub(1),
            Action::TocDown => {
                if !toc.is_empty() {
                    self.toc_selected = (self.toc_selected + 1).min(toc.len() - 1);
                }
            }
            Action::TocJump => {
                if let Some(entry) = toc.get(self.toc_selected) {
                    self.scroll = entry.row.min(self.max_scroll());
                }
                self.toc_focused = false;
            }
            Action::EnterSearch => self.mode = Mode::Search,
            Action::SearchInput(c) => self.search_query.push(c),
            Action::SearchBackspace => {
                self.search_query.pop();
            }
            Action::ConfirmSearch => {
                self.mode = Mode::Normal;
                self.search_active = true;
                self.current_match = matches.first().map(|_| 0);
                if let Some(first) = matches.first() {
                    self.scroll = first.row.min(self.max_scroll());
                }
            }
            Action::ExitSearch => self.clear_search(),
            Action::NextMatch => {
                if !matches.is_empty() {
                    self.current_match = search::next_match(self.current_match, matches.len());
                    if let Some(idx) = self.current_match {
                        self.scroll = matches[idx].row.min(self.max_scroll());
                    }
                }
            }
            Action::PrevMatch => {
                if !matches.is_empty() {
                    self.current_match = search::prev_match(self.current_match, matches.len());
                    if let Some(idx) = self.current_match {
                        self.scroll = matches[idx].row.min(self.max_scroll());
                    }
                }
            }
            // One context per press, innermost first: whatever holds
            // the keyboard goes before what merely marks up the
            // document, and an outline left open by a jump goes last.
            // With nothing open this is deliberately inert — an
            // accidental Esc must never cost the reader their place.
            Action::Cancel => {
                if self.toc_focused {
                    self.toc_open = false;
                    self.toc_focused = false;
                } else if self.search_active {
                    self.clear_search();
                } else if self.toc_open {
                    self.toc_open = false;
                }
            }
            Action::None => {}
        }
        KeyOutcome::Continue
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
        AppState {
            pending_g,
            toc_focused: false,
            mode: Mode::Normal,
            help_open: false,
        }
    }

    fn toc_entry(text: &str, row: usize) -> TocEntry {
        TocEntry {
            level: 1,
            text: text.to_string(),
            row,
        }
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
    fn tab_is_toggle_toc_regardless_of_focus() {
        assert_eq!(
            handle_key(&state(false), key(KeyCode::Tab)),
            Action::ToggleToc
        );
        let focused = AppState {
            pending_g: false,
            toc_focused: true,
            mode: Mode::Normal,
            help_open: false,
        };
        assert_eq!(handle_key(&focused, key(KeyCode::Tab)), Action::ToggleToc);
    }

    #[test]
    fn slash_enters_search_mode() {
        assert_eq!(
            handle_key(&state(false), key(KeyCode::Char('/'))),
            Action::EnterSearch
        );
    }

    fn search_state() -> AppState {
        AppState {
            pending_g: false,
            toc_focused: false,
            mode: Mode::Search,
            help_open: false,
        }
    }

    #[test]
    fn typed_chars_while_searching_build_the_query_even_reserved_keys() {
        assert_eq!(
            handle_key(&search_state(), key(KeyCode::Char('a'))),
            Action::SearchInput('a')
        );
        // 'q' is a valid search character, not the global quit shortcut,
        // while a query is being typed.
        assert_eq!(
            handle_key(&search_state(), key(KeyCode::Char('q'))),
            Action::SearchInput('q')
        );
    }

    #[test]
    fn ctrl_c_still_quits_while_searching() {
        assert_eq!(
            handle_key(&search_state(), ctrl_key(KeyCode::Char('c'))),
            Action::Quit
        );
    }

    #[test]
    fn backspace_while_searching_removes_the_last_typed_character() {
        assert_eq!(
            handle_key(&search_state(), key(KeyCode::Backspace)),
            Action::SearchBackspace
        );
    }

    #[test]
    fn enter_confirms_search_and_esc_exits_it() {
        assert_eq!(
            handle_key(&search_state(), key(KeyCode::Enter)),
            Action::ConfirmSearch
        );
        assert_eq!(
            handle_key(&search_state(), key(KeyCode::Esc)),
            Action::ExitSearch
        );
    }

    #[test]
    fn n_and_shift_n_navigate_matches_in_normal_mode() {
        assert_eq!(
            handle_key(&state(false), key(KeyCode::Char('n'))),
            Action::NextMatch
        );
        assert_eq!(
            handle_key(&state(false), key(KeyCode::Char('N'))),
            Action::PrevMatch
        );
    }

    #[test]
    fn up_down_enter_route_to_toc_only_when_focused() {
        let focused = AppState {
            pending_g: false,
            toc_focused: true,
            mode: Mode::Normal,
            help_open: false,
        };
        assert_eq!(handle_key(&focused, key(KeyCode::Up)), Action::TocUp);
        assert_eq!(handle_key(&focused, key(KeyCode::Down)), Action::TocDown);
        assert_eq!(handle_key(&focused, key(KeyCode::Enter)), Action::TocJump);

        // Unfocused: same keys mean the ordinary pager actions.
        assert_eq!(handle_key(&state(false), key(KeyCode::Up)), Action::LineUp);
        assert_eq!(
            handle_key(&state(false), key(KeyCode::Down)),
            Action::LineDown
        );
    }

    #[test]
    fn app_gg_sequence_scrolls_to_top() {
        let mut app = App::new(100);
        app.viewport_height = 10;
        app.scroll = 50;

        assert_eq!(
            app.on_key(key(KeyCode::Char('g')), &[], &[]),
            KeyOutcome::Continue
        );
        assert_eq!(app.scroll, 50, "first g should not move the scroll yet");

        assert_eq!(
            app.on_key(key(KeyCode::Char('g')), &[], &[]),
            KeyOutcome::Continue
        );
        assert_eq!(app.scroll, 0, "second g completes gg and scrolls to top");
    }

    #[test]
    fn app_scroll_clamps_to_bounds() {
        let mut app = App::new(10);
        app.viewport_height = 5;

        app.on_key(key(KeyCode::Char('k')), &[], &[]);
        assert_eq!(app.scroll, 0, "scrolling up from the top stays at 0");

        app.on_key(key(KeyCode::Char('G')), &[], &[]);
        assert_eq!(app.scroll, app.max_scroll(), "G scrolls to the max offset");

        app.on_key(key(KeyCode::Char('j')), &[], &[]);
        assert_eq!(
            app.scroll,
            app.max_scroll(),
            "scrolling down from the bottom stays clamped"
        );
    }

    #[test]
    fn app_quit_returns_quit() {
        let mut app = App::new(1);
        assert_eq!(
            app.on_key(key(KeyCode::Char('q')), &[], &[]),
            KeyOutcome::Quit
        );
    }

    #[test]
    fn app_r_asks_the_caller_to_reload() {
        let mut app = App::new(10);
        app.viewport_height = 5;

        assert_eq!(
            app.on_key(key(KeyCode::Char('r')), &[], &[]),
            KeyOutcome::Reload
        );
    }

    #[test]
    fn tab_opens_focused_then_closes_from_focused_state() {
        let mut app = App::new(100);
        let toc = vec![toc_entry("A", 0), toc_entry("B", 10)];

        app.on_key(key(KeyCode::Tab), &toc, &[]);
        assert!(app.toc_open);
        assert!(app.toc_focused);

        app.on_key(key(KeyCode::Tab), &toc, &[]);
        assert!(!app.toc_open, "Tab from focused state closes the sidebar");
        assert!(!app.toc_focused);
    }

    #[test]
    fn toc_selection_moves_with_up_down_and_clamps() {
        let mut app = App::new(100);
        let toc = vec![toc_entry("A", 0), toc_entry("B", 5), toc_entry("C", 10)];
        app.on_key(key(KeyCode::Tab), &toc, &[]); // open + focus

        app.on_key(key(KeyCode::Up), &toc, &[]);
        assert_eq!(
            app.toc_selected, 0,
            "selection can't go above the first entry"
        );

        app.on_key(key(KeyCode::Down), &toc, &[]);
        app.on_key(key(KeyCode::Down), &toc, &[]);
        app.on_key(key(KeyCode::Down), &toc, &[]);
        assert_eq!(
            app.toc_selected, 2,
            "selection clamps at the last entry, not past it"
        );
    }

    #[test]
    fn enter_jumps_to_selected_heading_and_unfocuses_but_stays_open() {
        let mut app = App::new(100);
        app.viewport_height = 10;
        let toc = vec![toc_entry("A", 0), toc_entry("B", 5)];
        app.on_key(key(KeyCode::Tab), &toc, &[]); // open + focus
        app.on_key(key(KeyCode::Down), &toc, &[]); // select "B"

        app.on_key(key(KeyCode::Enter), &toc, &[]);

        assert_eq!(app.scroll, 5, "jumped to the selected heading's row");
        assert!(app.toc_open, "sidebar stays open after a jump");
        assert!(!app.toc_focused, "focus returns to the main pane");
    }

    #[test]
    fn enter_clamps_jump_target_near_the_end_of_the_document() {
        // total_rows=20, viewport_height=10 => max_scroll=10, but the
        // heading is at row 18 (near the end) — the jump must clamp.
        let mut app = App::new(20);
        app.viewport_height = 10;
        let toc = vec![toc_entry("Near the end", 18)];
        app.on_key(key(KeyCode::Tab), &toc, &[]);

        app.on_key(key(KeyCode::Enter), &toc, &[]);

        assert_eq!(app.scroll, 10, "jump target clamps to max_scroll");
    }

    #[test]
    fn tab_refocuses_an_open_but_unfocused_toc_without_closing_it() {
        // "peek, jump, and peek again without reopening it"
        let mut app = App::new(100);
        app.viewport_height = 10;
        let toc = vec![toc_entry("A", 0), toc_entry("B", 5)];
        app.on_key(key(KeyCode::Tab), &toc, &[]); // open + focus
        app.on_key(key(KeyCode::Enter), &toc, &[]); // jump: open, unfocused

        app.on_key(key(KeyCode::Tab), &toc, &[]);

        assert!(app.toc_open, "still open");
        assert!(app.toc_focused, "Tab re-focused it instead of closing it");
    }

    fn a_match(row: usize, start: usize, end: usize) -> search::Match {
        search::Match { row, start, end }
    }

    #[test]
    fn confirming_a_search_jumps_to_the_first_match_and_activates_it() {
        let mut app = App::new(100);
        app.viewport_height = 10;
        app.mode = Mode::Search;
        app.search_query = "fox".to_string();
        let matches = vec![a_match(20, 0, 3), a_match(40, 0, 3)];

        app.on_key(key(KeyCode::Enter), &[], &matches);

        assert_eq!(app.mode, Mode::Normal, "confirming returns to normal mode");
        assert!(app.search_active);
        assert_eq!(app.current_match, Some(0));
        assert_eq!(app.scroll, 20, "jumped to the first match's row");
    }

    #[test]
    fn confirming_a_search_with_no_matches_leaves_scroll_untouched() {
        let mut app = App::new(100);
        app.viewport_height = 10;
        app.mode = Mode::Search;
        app.search_query = "elephant".to_string();
        app.scroll = 5;

        app.on_key(key(KeyCode::Enter), &[], &[]);

        assert!(
            app.search_active,
            "a confirmed search with zero results is still active"
        );
        assert_eq!(app.current_match, None);
        assert_eq!(app.scroll, 5, "no match to jump to, so scroll is unchanged");
    }

    #[test]
    fn n_advances_to_the_next_match_and_wraps_to_the_first() {
        let mut app = App::new(100);
        app.viewport_height = 10;
        app.search_active = true;
        app.current_match = Some(0);
        let matches = vec![a_match(10, 0, 3), a_match(30, 0, 3), a_match(50, 0, 3)];

        app.on_key(key(KeyCode::Char('n')), &[], &matches);
        assert_eq!(app.current_match, Some(1));
        assert_eq!(app.scroll, 30);

        app.on_key(key(KeyCode::Char('n')), &[], &matches);
        assert_eq!(app.current_match, Some(2));
        assert_eq!(app.scroll, 50);

        app.on_key(key(KeyCode::Char('n')), &[], &matches);
        assert_eq!(app.current_match, Some(0), "wraps back to the first match");
        assert_eq!(app.scroll, 10);
    }

    #[test]
    fn shift_n_retreats_to_the_previous_match_and_wraps_to_the_last() {
        let mut app = App::new(100);
        app.viewport_height = 10;
        app.search_active = true;
        app.current_match = Some(0);
        let matches = vec![a_match(10, 0, 3), a_match(30, 0, 3), a_match(50, 0, 3)];

        app.on_key(key(KeyCode::Char('N')), &[], &matches);
        assert_eq!(app.current_match, Some(2), "wraps back to the last match");
        assert_eq!(app.scroll, 50);
    }

    #[test]
    fn esc_clears_the_query_and_deactivates_the_search() {
        let mut app = App::new(100);
        app.mode = Mode::Search;
        app.search_query = "fox".to_string();
        app.search_active = true;
        app.current_match = Some(0);

        app.on_key(key(KeyCode::Esc), &[], &[a_match(0, 0, 3)]);

        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.search_query, "");
        assert!(!app.search_active);
        assert_eq!(app.current_match, None);
    }

    #[test]
    fn r_reloads_the_document() {
        assert_eq!(
            handle_key(&state(false), key(KeyCode::Char('r'))),
            Action::Reload
        );
    }

    #[test]
    fn r_typed_into_a_search_query_is_text_not_a_reload() {
        let searching = AppState {
            pending_g: false,
            toc_focused: false,
            mode: Mode::Search,
            help_open: false,
        };
        assert_eq!(
            handle_key(&searching, key(KeyCode::Char('r'))),
            Action::SearchInput('r')
        );
    }

    #[test]
    fn r_reloads_even_while_the_toc_is_focused() {
        let focused = AppState {
            pending_g: false,
            toc_focused: true,
            mode: Mode::Normal,
            help_open: false,
        };
        assert_eq!(
            handle_key(&focused, key(KeyCode::Char('r'))),
            Action::Reload
        );
    }

    #[test]
    fn a_preserved_match_stays_selected_without_a_note() {
        let mut app = App::new(10);
        app.search_active = true;
        app.current_match = Some(1);

        app.apply_reselection(search::Reselection::Preserved(2));

        assert_eq!(app.current_match, Some(2));
        assert!(!app.search_fell_back);
    }

    #[test]
    fn a_lost_match_selects_the_first_and_notes_the_fallback() {
        let mut app = App::new(10);
        app.search_active = true;
        app.current_match = Some(1);

        app.apply_reselection(search::Reselection::FellBackToFirst);

        assert_eq!(app.current_match, Some(0));
        assert!(app.search_fell_back);
    }

    #[test]
    fn a_query_that_stops_matching_clears_the_selection() {
        let mut app = App::new(10);
        app.search_active = true;
        app.current_match = Some(1);

        app.apply_reselection(search::Reselection::NoMatches);

        assert_eq!(app.current_match, None);
        assert!(!app.search_fell_back);
    }

    #[test]
    fn moving_to_another_match_clears_the_fallback_note() {
        let mut app = App::new(10);
        app.viewport_height = 5;
        app.search_active = true;
        app.apply_reselection(search::Reselection::FellBackToFirst);

        app.on_key(
            key(KeyCode::Char('n')),
            &[],
            &[a_match(0, 0, 3), a_match(1, 0, 3)],
        );

        assert!(!app.search_fell_back, "n moves on from the fallback");
    }

    #[test]
    fn a_first_selection_after_a_reload_carries_no_fallback_note() {
        let mut app = App::new(10);
        app.search_active = true;
        app.current_match = None;

        app.apply_reselection(search::Reselection::SelectedFirst);

        assert_eq!(app.current_match, Some(0));
        assert!(!app.search_fell_back);
    }

    #[test]
    fn question_mark_opens_the_help_overlay_and_the_next_key_closes_it() {
        let mut app = App::new(100);
        app.viewport_height = 10;

        app.on_key(key(KeyCode::Char('?')), &[], &[]);
        assert!(app.help_open);

        // A key that would normally scroll: while the overlay is up it
        // only dismisses it, so the reader can't lose their place by
        // pressing something to get rid of it.
        app.on_key(key(KeyCode::Char('j')), &[], &[]);
        assert!(!app.help_open);
        assert_eq!(app.scroll, 0, "the dismissing key also scrolled");
    }

    #[test]
    fn ctrl_c_still_quits_from_under_the_help_overlay() {
        let mut app = App::new(100);
        app.on_key(key(KeyCode::Char('?')), &[], &[]);

        let outcome = app.on_key(ctrl_key(KeyCode::Char('c')), &[], &[]);

        assert_eq!(outcome, KeyOutcome::Quit);
    }

    #[test]
    fn the_help_overlay_swallows_reload_and_quit_too() {
        let mut app = App::new(100);
        app.on_key(key(KeyCode::Char('?')), &[], &[]);
        assert_eq!(
            app.on_key(key(KeyCode::Char('q')), &[], &[]),
            KeyOutcome::Continue
        );
        assert!(!app.help_open);

        app.on_key(key(KeyCode::Char('?')), &[], &[]);
        assert_eq!(
            app.on_key(key(KeyCode::Char('r')), &[], &[]),
            KeyOutcome::Continue
        );
    }

    #[test]
    fn a_query_containing_a_question_mark_is_typed_not_treated_as_help() {
        let mut app = App::new(100);
        app.on_key(key(KeyCode::Char('/')), &[], &[]);
        app.on_key(key(KeyCode::Char('?')), &[], &[]);

        assert!(!app.help_open);
        assert_eq!(app.search_query, "?");
    }

    #[test]
    fn esc_in_normal_mode_asks_to_cancel_the_current_context() {
        // The key map only knows the reader pressed the cancel key;
        // which context that cancels is `App::on_key`'s decision, since
        // it is the only thing that knows what is currently open.
        assert_eq!(
            handle_key(&focus_state(Focus::Pager), key(KeyCode::Esc)),
            Action::Cancel
        );
        assert_eq!(
            handle_key(&focus_state(Focus::Outline), key(KeyCode::Esc)),
            Action::Cancel
        );
    }

    /// A reader partway down a document with a search confirmed. The
    /// precedence cases build on this, opening the outline themselves
    /// when that is what they are about.
    fn reader_mid_search() -> App {
        let mut app = App::new(100);
        app.viewport_height = 10;
        app.scroll = 40;
        app.search_query = "fox".to_string();
        app.search_active = true;
        app.current_match = Some(1);
        app
    }

    #[test]
    fn esc_with_a_confirmed_search_clears_it_without_moving_the_reader() {
        let mut app = reader_mid_search();
        app.search_fell_back = true;

        let outcome = app.on_key(key(KeyCode::Esc), &[], &[a_match(0, 0, 3)]);

        assert_eq!(outcome, KeyOutcome::Continue);
        assert_eq!(app.search_query, "");
        assert!(!app.search_active);
        assert_eq!(app.current_match, None);
        assert!(!app.search_fell_back);
        assert_eq!(app.scroll, 40, "clearing highlights must not scroll");
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn esc_closes_the_focused_outline_before_touching_the_search() {
        // Esc dismisses whatever has the keyboard first; the highlights
        // are still there for a second press.
        let mut app = reader_mid_search();
        app.toc_open = true;
        app.toc_focused = true;

        app.on_key(key(KeyCode::Esc), &[], &[a_match(0, 0, 3)]);

        assert!(!app.toc_open);
        assert!(!app.toc_focused);
        assert!(app.search_active, "the search survives the first Esc");
        assert_eq!(app.search_query, "fox");
        assert_eq!(app.scroll, 40);
    }

    #[test]
    fn esc_closes_an_unfocused_outline_once_the_search_is_clear() {
        // Jumping from the outline leaves it open but unfocused. The
        // search is the inner context, so it goes first.
        let mut app = reader_mid_search();
        app.toc_open = true;

        app.on_key(key(KeyCode::Esc), &[], &[a_match(0, 0, 3)]);

        assert!(!app.search_active);
        assert!(app.toc_open, "the outline outlives the search");

        app.on_key(key(KeyCode::Esc), &[], &[]);

        assert!(!app.toc_open);
        assert_eq!(app.scroll, 40);
    }

    #[test]
    fn esc_abandons_a_query_even_while_the_outline_is_focused() {
        // A query being typed holds the keyboard ahead of everything
        // else, the outline included, so it is the innermost context of
        // all — `focus_state` can't express this pairing, hence the
        // hand-built state.
        let state = AppState {
            pending_g: false,
            toc_focused: true,
            mode: Mode::Search,
            help_open: false,
        };

        assert_eq!(handle_key(&state, key(KeyCode::Esc)), Action::ExitSearch);
    }

    #[test]
    fn esc_closes_the_outline_when_no_search_was_ever_run() {
        // The plainest path there is: Tab, then Esc. The outline branch
        // must not quietly depend on a search being active.
        let toc = [toc_entry("Heading", 0)];
        let mut app = App::new(100);
        app.viewport_height = 10;
        app.on_key(key(KeyCode::Tab), &toc, &[]);
        assert!(app.toc_open && app.toc_focused, "Tab opens and focuses");

        app.on_key(key(KeyCode::Esc), &toc, &[]);

        assert!(!app.toc_open);
        assert!(!app.toc_focused);
    }

    #[test]
    fn esc_with_nothing_active_does_nothing_at_all() {
        // Notably it does not quit: an accidental Esc must never cost the
        // reader their place in the document.
        let mut app = App::new(100);
        app.viewport_height = 10;
        app.scroll = 40;

        let outcome = app.on_key(key(KeyCode::Esc), &[], &[]);

        assert_eq!(outcome, KeyOutcome::Continue);
        assert_eq!(app.scroll, 40);
        assert!(!app.toc_open);
        assert!(!app.search_active);
        assert_eq!(app.mode, Mode::Normal);
    }

    /// The state a binding's focus describes.
    fn focus_state(focus: Focus) -> AppState {
        AppState {
            pending_g: false,
            toc_focused: focus == Focus::Outline,
            mode: match focus {
                Focus::Search => Mode::Search,
                _ => Mode::Normal,
            },
            help_open: false,
        }
    }

    #[test]
    fn every_advertised_keybinding_is_one_the_app_actually_honours() {
        for binding in KEYBINDINGS {
            let state = focus_state(binding.focus);
            for (code, modifiers, action) in binding.checks {
                assert_eq!(
                    handle_key(&state, KeyEvent::new(*code, *modifiers)),
                    *action,
                    "the overlay promises {:?} ({:?}) does {:?}",
                    binding.keys,
                    code,
                    binding.description
                );
            }
        }
    }

    #[test]
    fn every_key_the_app_honours_is_one_the_overlay_advertises() {
        // The other direction: a key wired up in `handle_key` but left
        // out of the table would be undiscoverable, which is the whole
        // problem the overlay exists to solve.
        let mut candidates: Vec<(KeyCode, KeyModifiers)> = Vec::new();
        for c in ' '..='~' {
            candidates.push((KeyCode::Char(c), KeyModifiers::NONE));
            candidates.push((KeyCode::Char(c), KeyModifiers::CONTROL));
        }
        for code in [
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Home,
            KeyCode::End,
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::Enter,
            KeyCode::Esc,
            KeyCode::Tab,
            KeyCode::BackTab,
            KeyCode::Backspace,
            KeyCode::Delete,
            KeyCode::Insert,
        ] {
            candidates.push((code, KeyModifiers::NONE));
        }

        for focus in [Focus::Pager, Focus::Outline, Focus::Search] {
            let state = focus_state(focus);
            for (code, modifiers) in &candidates {
                let action = handle_key(&state, KeyEvent::new(*code, *modifiers));
                match action {
                    // Not a binding: typing a query is what the `/` row
                    // already describes, and a lone `g` is half of `gg`.
                    Action::None | Action::SearchInput(_) => continue,
                    _ if code == &KeyCode::Char('g') && modifiers.is_empty() => continue,
                    _ => {}
                }

                // Matched on key and action, not modifiers: most arms
                // deliberately ignore modifiers, because a terminal
                // reports `G` and `N` as SHIFT-modified and they would
                // otherwise stop working.
                let advertised = KEYBINDINGS.iter().any(|binding| {
                    binding
                        .checks
                        .iter()
                        .any(|(c, _, a)| (c, a) == (code, &action))
                });
                assert!(
                    advertised,
                    "{code:?} with {modifiers:?} does {action:?} in {focus:?}, \
                     but the help overlay never mentions it"
                );
            }
        }
    }
}
