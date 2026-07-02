Status: ready-for-agent

# In-document search

## Parent

.scratch/mdview-v1/PRD.md

## What to build

In-document text search with next/prev match navigation.

## Acceptance criteria

- [ ] `search.rs`: `search(query, LayoutDoc) -> match list`, case-insensitive
- [ ] `/` enters search mode; typed characters build the query; `Enter` confirms and jumps to the first match
- [ ] `n`/`N` jump to the next/previous match, wrapping around at the start/end of the document
- [ ] Matches are visually highlighted in the rendered text
- [ ] `Esc` exits search mode and clears the highlight
- [ ] A query with zero matches shows a clear "no matches" indication rather than silently doing nothing
- [ ] Unit tests: match-finding correctness, case-insensitivity, next/prev wraparound at document boundaries, zero-match case
- [ ] Snapshot test via `ratatui::TestBackend` showing a search-highlighted state

## Blocked by

- 02-core-markdown-rendering
