# ADR-0003: Preserving search state across a reload

Status: Accepted
Implements: `.scratch/mdview-v1/issues/08-reload-preserves-search-state.md`

## Context

Matches are recomputed every frame from the current `LayoutDoc` and
`App::search_query` (ADR-0001), and `App::current_match` is an *index*
into that list. That's stable while the document is, but issue #7 made
the document change underneath it: after a reload the same index can
point at a different occurrence, or at nothing.

## Decisions

- **A match is re-found by what the reader sees, not by index**:
  `search::MatchAnchor` records the text of the row the match sits in,
  its column, and which of the identical (row text, column) pairs it is.
  Row *numbers* shift with any edit above; row *text* doesn't. This is
  the same identity strategy `reload`'s heading anchor uses, for the same
  reason.
- **`search::resolve_match` returns a `Reselection` enum**
  (`Preserved(index)` / `SelectedFirst` / `FellBackToFirst` /
  `NoMatches`) rather than an `Option<usize>`, because the outcomes drive
  different UI: `NoMatches` is the existing "no matches" indicator, and
  `FellBackToFirst` has to be *announced* — the issue requires the status
  line to note that the selection moved — while `SelectedFirst` (a query
  that matched nothing before the edit and matches now) must not be
  announced as a loss. Only `Preserved` carries an index; the other two
  "first match" cases mean index 0 by definition, since the match list is
  always in document order.
- **`anchor_match`/`resolve_match` recompute the match list themselves**
  from `(query, layout_doc)` instead of taking the frame's resolved list.
  The reload path runs between frames and has no match list in hand, and
  recomputation is the same cheap pass the render loop already does each
  frame.
- **A reload does not scroll to the re-selected match.** The document
  anchor (ADR-0002) decides where the viewport lands; the reader's place
  in the text outranks the selection, which `n` steps back to.
- **`App::search_fell_back` is a flag on `App`, cleared by the next
  deliberate search action** (`/`, `Enter`, `Esc`, `n`, `N`). It's state
  about the last reload, not about the query, so it doesn't belong in
  `search.rs`; and it must not stick around once the user has moved on.
- **The fallback status line states the count**
  (`Match 1/3 for "fox" (previous match gone)`) rather than a bare
  warning: the document just changed under the reader, so how many
  matches there are now is the more useful half of the message.
- **An active query is re-run even with nothing selected**: if
  `current_match` was `None` before the reload (a confirmed query that
  matched nothing) and the edit introduced a match, the first one is
  selected — the issue asks for the query to be re-run after every
  reload, and a selection is what `n`/`N` navigate from. The whole step is
  gated on `search_active`, so a viewer with no search runs untouched.

## Known limitation

Because a match is identified by its whole rendered row, an edit *inside
the same wrapped paragraph* (a typo fix, a word added earlier in the
paragraph) rewrites that row's text and drops the anchor, even though the
matched text itself survives — the reader lands on match 1 with the
"previous match gone" note. Resolving that needs a coarser identity (the
match's ordinal within its section or block) and is deliberately left out
of this issue.

## Consequences

- `search.rs` stays terminal-agnostic and fully unit-testable; the only
  new coupling is `reload` calling into it, mirroring how it already
  calls `toc`.
- A rewritten line loses its anchor even if the query still matches
  inside it — the selection falls back to the first match and says so,
  which is the issue's specified behavior for an unresolvable match.
