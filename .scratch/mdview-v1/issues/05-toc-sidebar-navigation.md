Status: ready-for-agent

# TOC sidebar navigation

## Parent

.scratch/mdview-v1/PRD.md

## What to build

A toggleable outline sidebar listing every heading in the currently open file, letting the user jump directly to a heading. The sidebar stays open after a jump so the user can peek, jump, and peek again without reopening it.

## Acceptance criteria

- [ ] `toc.rs`: `TocEntry{level, text, row}`, populated from headings collected during the blocks-lowering pass (no second document traversal) and resolved to layout rows once `LayoutDoc` is computed
- [ ] `Tab` toggles the TOC sidebar open/closed
- [ ] While the TOC pane is focused, `Up`/`Down` moves the selection between headings
- [ ] `Enter` jumps the main pane's scroll to the selected heading's row (clamped to valid range) and returns focus to the main pane
- [ ] The TOC sidebar remains open after a jump (only `Tab` closes it)
- [ ] TOC entries are visually indented by heading level
- [ ] Unit tests: heading collection from a sample document with nested heading levels, row resolution, and jump-target clamping when a heading is near the end of the document
- [ ] Snapshot test via `ratatui::TestBackend` showing the TOC-open state

## Blocked by

- 02-core-markdown-rendering
