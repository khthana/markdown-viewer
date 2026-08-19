Status: ready-for-agent

# Esc cancels the current context in normal mode

## Parent

.scratch/mdview-v1/PRD.md

## What to build

The PRD's keybinding table lists `Esc` as "exit search / close TOC / clear
highlight", but in normal mode `Esc` currently does nothing at all
(`app::handle_key` matches it only inside `Mode::Search`, where it
abandons a query being typed). A reader who has confirmed a search with
`Enter` has no way to clear the highlights except searching for something
that doesn't match, and a reader with the outline open has to press `Tab`
until it closes rather than the key the PRD advertises.

This became visible when issue #12 built the `?` overlay from
`app::KEYBINDINGS`: the overlay is only as complete as the keymap it
documents, and the keymap is missing this row. ADR-0001 records the
narrow `Esc` handling as a deliberate scope cut for issue #6 ("why `Esc`
doesn't also close the TOC yet"), so this issue is what re-opens it.

Deciding the precedence is part of this issue. `Esc` has three plausible
jobs at once and they need an order — the candidates worth weighing are
"innermost context first" (clear a confirmed search, else unfocus/close
the outline) versus "one key, one job" (clear the search only, leaving
`Tab` to own the outline). Whichever is chosen, `Esc` with nothing active
must stay a no-op rather than quitting: an accidental `Esc` should never
lose the reader's place.

## Acceptance criteria

- [ ] `Esc` in normal mode with a confirmed search active clears the
      query, the highlights, and the match selection, and the status row
      is released — without moving the scroll position
- [ ] `Esc` with the outline focused behaves per the chosen precedence,
      and that choice is recorded in an ADR alongside why the alternative
      was rejected
- [ ] `Esc` with nothing active does nothing at all (no quit, no scroll)
- [ ] `Esc` while a query is still being typed keeps issue #6's existing
      behaviour: abandon the query, return to normal mode
- [ ] `app::KEYBINDINGS` gains the row, so `?` documents it — the
      both-directions tests added by issue #12 must pass unchanged
- [ ] ADR-0001's note that `Esc` deliberately doesn't close the TOC is
      updated to point at the new ADR
- [ ] Unit tests at the `handle_key`/`App::on_key` seam covering each
      precedence case above

## Notes

Found by the spec-axis code review of issue #12 (2026-08-19), which
observed that the `?` overlay lists every key the app honours but that
the keymap itself is short of what the PRD advertises. Recorded in
ADR-0007's consequences.

## Blocked by

- 05-toc-sidebar-navigation
- 06-in-document-search
- 12-cli-flags-help-error-handling
