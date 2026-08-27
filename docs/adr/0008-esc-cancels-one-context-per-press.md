# ADR-0008: `Esc` backs out of one context per press

Status: Accepted
Implements: `.scratch/mdview-v1/issues/15-esc-cancels-in-normal-mode.md`

## Context

The PRD's keybinding table has always advertised `Esc` as "exit search /
close TOC / clear highlight", but only the first of those was ever built:
ADR-0001 scoped `Esc` to search input for issue #6 and deferred the rest.
In normal mode the key did nothing at all.

Issue #12 turned that deferral into a visible defect. The `?` overlay is
generated from `app::KEYBINDINGS`, and two tests hold the table and
`handle_key` to each other in both directions — so the overlay is exactly
as complete as the keymap, and the keymap was a row short of what the PRD
promised.

`Esc` also has three plausible jobs at once (clear the search, close the
outline, do nothing), and unlike every other key in the table, which one
it does depends on what is currently open.

## Decisions

- **`Esc` maps to a single `Action::Cancel`, and `App::on_key` decides
  what that cancels.** The alternative — returning `ClearSearch` or
  `CloseToc` from `handle_key` depending on state — is unrepresentable in
  the design ADR-0007 established: a `Binding` pairs each key with *the
  one* action it must produce, so a state-dependent key would make its
  row unsatisfiable. Resolving it in `App` also keeps `AppState` free of
  `toc_open` and `search_active`; the keymap stays a pure key-to-intent
  function that knows nothing about what is on screen.
- **Precedence is focus-first, one context per press.** The outline goes
  while it holds the keyboard, then the search, then an outline left open
  by a jump:

  ```text
  toc_focused    -> close the outline
  search_active  -> clear the query, highlights and selection
  toc_open       -> close the outline
  otherwise      -> nothing
  ```

  Two alternatives were rejected. *Search first* would let `Esc` reach
  past the pane that currently has the keyboard, which is the one thing a
  reader expects `Esc` to act on. *One key, one job* — `Esc` clears the
  search and `Tab` alone owns the outline — is tidier, but it contradicts
  the PRD's own table, and the PRD is the spec.
- **`Esc` with nothing active is inert, and never a quit.** `q` and
  `Ctrl-C` are the only ways out. An accidental `Esc` must not cost the
  reader their place, so `Cancel` also never touches `scroll`; the tests
  assert the scroll offset across every precedence case.
- **Dropping a search is one function.** `App::clear_search` is shared by
  `Esc` while a query is being typed (issue #6's behaviour, unchanged)
  and `Esc` once one has been confirmed, so the two ways out of a search
  can't leave different state behind. It also clears `search_fell_back`,
  on a rule of its own: ADR-0003 says a deliberate search step supersedes
  the note that a reload moved the selection, whereas this says the note
  can't outlive the search it describes. `Cancel` is deliberately *not* a
  search step, so closing the outline over a fallback-flagged search
  leaves the note standing.
- **The overlay lists `Esc` under both focuses.** One row under Document
  ("Clear the search, or close the outline") and one under Outline
  ("Close the outline"), because a reader looking at the outline section
  would otherwise never learn the key applies there.

## Consequences

- ADR-0001's deferral is closed, and its note now points here rather than
  at the open issue.
- Closing the outline with `Esc` reflows the document at the wider
  width, exactly as closing it with `Tab` does — `scroll` is a row index,
  so a reflow can move what is under it. That behaviour is inherited, not
  introduced here.
- `Esc` still cannot undo a TOC jump or restore a previous scroll
  position. It cancels contexts, it is not an undo stack.
- While the help overlay is up, `Esc` still means "close the help": the
  overlay is checked ahead of the normal-mode key map and swallows every
  key but `Ctrl-C` (ADR-0007).
