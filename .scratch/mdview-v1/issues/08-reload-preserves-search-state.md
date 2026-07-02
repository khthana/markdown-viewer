Status: ready-for-agent

# Reload preserves active search match state

## Parent

.scratch/mdview-v1/PRD.md

## What to build

When a reload happens while a search is active, re-run the active query against the new document and preserve the current match position where possible.

## Acceptance criteria

- [ ] After a reload (auto or manual) with an active search query, the query is automatically re-run against the new layout
- [ ] If the previously-selected match still exists (same text at a resolvable position) after the edit, the selection stays on the equivalent match
- [ ] If the match count changed such that the previous match no longer resolves, selection falls back to the first match and the status line notes the change
- [ ] If the query now has zero matches after the edit, search state shows "no matches" rather than erroring or crashing
- [ ] Unit tests covering: match preserved after an unrelated edit elsewhere in the document, match position shifts correctly after an edit above it, fallback behavior when the matched text was removed

## Blocked by

- 06-in-document-search
- 07-auto-reload-anchor-preservation
