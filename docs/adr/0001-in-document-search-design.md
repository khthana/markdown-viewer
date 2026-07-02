# ADR-0001: In-document search design

Status: Accepted
Implements: `.scratch/mdview-v1/issues/06-in-document-search.md`

## Context

Issue #6 needed case-insensitive search over the rendered document, with
`/`-to-enter, `n`/`N` wraparound navigation, visible highlighting, and a
"no matches" indication. Several shapes were possible for where match
text comes from, how key capture interacts with the existing TOC-focus
model, and how highlighting composes with already-styled text.

## Decisions

- **`LayoutDoc` gained a `rows: Vec<String>` field** (plain text per rendered
  row), populated in the same per-block render pass `layout()` already
  used to measure row counts. Search matches against this instead of the
  raw Markdown source, so it searches what the reader actually sees
  (wrapped, GFM-rendered text), and row-count/search-text can never drift
  apart, per the same principle already documented on `render_lines`.
- **Search matches are recomputed every frame** from `LayoutDoc` +
  `App::search_query` (in `main.rs`, mirroring `toc::resolve`), not cached
  on `App`. Keeps `App` a thin state container; the match list is cheap
  to recompute and must already be redone on resize/reload anyway.
- **`App::on_key` gained a `matches: &[search::Match]` parameter**,
  mirroring the existing `toc: &[TocEntry]` parameter — actions like
  "confirm search" or "jump to next match" need read access to the
  currently resolved list to pick a scroll target, same as `TocJump`
  already does with `toc`.
- **A `Mode` enum (`Normal` / `Search`) was added, scoped narrowly to
  search** — it is not folded into the existing `toc_focused: bool`.
  `handle_key` checks `Mode::Search` first, before the TOC-focus branch,
  and while in that mode nearly every key becomes text input (`Char(c) =>
  SearchInput(c)`), including keys normally reserved for navigation (`q`
  no longer quits mid-query). `Ctrl-C` is checked before the mode
  dispatch as a universal escape hatch.
- **Highlighting is applied post-hoc**, not baked into block rendering:
  `ui::highlight_line` splits a `Line`'s spans at a match's char-offset
  boundaries and patches a highlight `Style` onto just that range, leaving
  the rest of the line's existing styling (bold, links, syntax highlight
  colors, ...) untouched. This keeps `markdown::layout` search-agnostic.
- **A one-row status line is reserved whenever `mode == Search` OR a
  search is active** (`ui::search_status_visible`), not conditionally on
  match count. This keeps the content pane's height — and therefore
  `App::viewport_height` / scroll math — stable across a search's
  lifecycle, instead of the pane resizing by a row the moment a query's
  result count changes.
- **`Esc` was scoped to only clear/exit search** for this issue. The
  PRD's keybinding table groups `Esc` as also closing the TOC sidebar,
  but issue #6's acceptance criteria only covers search — unifying that
  behavior was deferred rather than bundled in here.

## Consequences

- `search.rs` stays a pure, terminal-agnostic module (`Match`, `search`,
  `next_match`/`prev_match`) — no ratatui types, fully unit-testable.
- Any future block type that changes what's user-visible (e.g. an image
  alt-text placeholder) automatically becomes searchable for free, since
  search reads `LayoutDoc.rows` rather than walking `Block` again.
- If a later issue unifies `Esc` to also close the TOC, that change is
  additive in `app::handle_key`'s Normal-mode branch, not a rework of the
  search state machine.
