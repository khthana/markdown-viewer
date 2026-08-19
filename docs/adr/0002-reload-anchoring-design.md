# ADR-0002: Auto-reload and scroll anchoring design

Status: Accepted
Implements: `.scratch/mdview-v1/issues/07-auto-reload-anchor-preservation.md`

## Context

Issue #7 needed the viewer to re-render when the open file changes on
disk, without throwing the reader back to the top of the document. The
PRD's module map names `watch.rs` for the debounced directory watch but
doesn't say where the anchor-and-restore logic lives, nor what an anchor
is made of. `main.rs` also parsed the file exactly once at startup and
handed the resulting blocks to the render loop as immutable data, so
there was no place for a new document to go.

## Decisions

- **A new `reload.rs` module** owns the document lifecycle: `Document`
  (blocks + headings), `load`, `reload_preserving_position`, and the
  anchor rules (`Anchor`, `compute_anchor`, `resolve_anchor`). This is an
  addition to the PRD's module map, made because the logic is neither
  watching (`watch.rs` is I/O and platform behavior) nor heading
  navigation (`toc.rs` resolves a chosen `TocEntry` to a scroll offset;
  it doesn't decide *which* position to restore). Keeping it separate
  leaves both those modules unchanged and makes the anchor rules
  unit-testable without a filesystem or a terminal.
- **An anchor identifies a heading by `level` + `text` + `occurrence`,
  plus the row `offset` the viewport had scrolled past it** — not by
  block index or row number, both of which shift the moment anything
  above is edited. `occurrence` disambiguates repeated identical headings
  (e.g. several `## Notes` sections).
- **Only the same occurrence counts.** If that copy is gone after the
  edit, the anchor is treated as lost rather than resolving to another
  copy: scrolling the reader backwards into a different section is worse
  than holding position. The known limitation is the mirror case —
  inserting an *identical* heading above shifts which occurrence matches,
  landing the reader a section early.
- **When no heading sits above the viewport, the anchor is the nearest
  non-blank rendered row's text** (`AnchorKind::Content`), resolved by
  matching that text in the new layout's rows, with the same offset rule
  as a heading. Blank rows are skipped because they'd match anywhere.
  `AnchorKind::Unanchored` covers only a document with nothing rendered
  at or above the position (an empty file).
- **A lost anchor clamps the previous scroll offset into the new
  document's range** (`app::max_scroll`), never 0. Snapping to the top is
  the failure mode the PRD explicitly calls out as most user-visible.
  `max_scroll` is a shared function so the pager and the reload path
  can't drift apart on the clamp rule.
- **`App::on_key` returns `KeyOutcome { Continue, Quit, Reload }`**
  instead of `bool`. Reloading is file I/O, which `App` deliberately
  doesn't own (it holds row counts and offsets, not the document), so
  `r` reports a request and `main.rs` performs it — the same path
  `Event::FileChanged` takes. `r` is handled alongside `Tab` and `q` as a
  global key, so it works while the TOC sidebar has focus.
- **`event.rs` owns both event sources**, per the PRD's module map: it
  starts the input thread and the watcher and returns a `Sources` value
  that keeps the debouncer alive for as long as the receiver — dropping
  the debouncer silently stops the watch.
- **The watch is registered on the parent directory, non-recursive, and
  filtered by file name** (`watch::event_matches_target`), per the PRD.
  Matching is on the file name alone rather than a canonicalized event
  path, because a delete-and-recreate save leaves the event path
  unresolvable at the moment the event arrives; the directory scope
  already bounds which events can be seen. The *target* is canonicalized
  once at startup to derive that directory and name.
- **A watcher that fails to start is non-fatal but not silent**: the
  `anyhow` context chain is printed to stderr before the alternate screen
  is entered, and the viewer runs with manual `r` reload only.
- **A failed reload keeps the current document on screen.** A save in
  progress can leave the file missing or empty for an instant; erroring
  out of the loop there would be worse than rendering slightly stale
  content, and the next event (or `r`) picks up the real content.

## Consequences

- `main.rs` shrinks back to argument parsing plus the render loop; the
  shared `event::channel()` is the shape issue #11's `Event::ImageReady`
  will plug into.
- Search state is *not* re-anchored yet: matches are recomputed each frame
  from the new layout, so a reload can leave `current_match` pointing at a
  different occurrence. That's issue #8's scope.
- Anchoring by rendered text means a reload after an edit that only
  changes styling (not text) still resolves exactly, but an edit that
  rewords the anchored line falls back to the clamp.
