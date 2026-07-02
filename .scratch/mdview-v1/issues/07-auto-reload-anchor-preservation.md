Status: ready-for-agent

# Auto-reload with heading-anchored scroll preservation + manual reload

## Parent

.scratch/mdview-v1/PRD.md

## What to build

Watch the open file on disk and automatically re-render it when it changes, preserving the user's scroll position by anchoring to the nearest heading rather than resetting to the top. Also adds a manual reload key for cases where the debounce hasn't fired yet.

## Acceptance criteria

- [ ] `watch.rs`: uses `notify-debouncer-full`, watching the file's **parent directory** (non-recursive) and filtering events by the canonicalized target filename — NOT a watch registered directly on the file path (this breaks on editors that save via write-temp-then-rename or delete+recreate)
- [ ] File-change events are debounced (~150-300ms) into a single `Event::FileChanged` delivered through the same unified event channel as input events
- [ ] On `Event::FileChanged`: before discarding the old layout, compute an anchor (nearest heading at or above current scroll, or block content if no heading exists above the current position)
- [ ] Re-read, re-parse, re-lower, re-layout the file, then resolve the anchor against the new blocks (by heading level+text match, or content match) and set scroll to the anchor's new position
- [ ] If the anchored heading/content was deleted in the edit, scroll clamps to the new document's total height rather than jumping to 0
- [ ] `r` triggers an immediate manual reload using the same anchor-preserving logic, independent of the debounce timer
- [ ] Manually editing and saving the open file in a real editor (VS Code or Notepad) while mdview is running updates the view automatically without losing scroll position
- [ ] Unit tests: reload-anchor resolution — simulate an old `Vec<Block>` + scroll position, mutate the blocks (rename a heading, delete a heading, add a heading above/below), assert the anchor resolves to a sane position in each case, including the deleted-anchor clamp case

## Blocked by

- 02-core-markdown-rendering
