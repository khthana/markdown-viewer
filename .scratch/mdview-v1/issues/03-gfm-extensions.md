Status: ready-for-agent

# GFM extensions (tables, task lists, strikethrough, footnotes)

## Parent

.scratch/mdview-v1/PRD.md

## What to build

Extend parsing/layout/rendering to support GitHub Flavored Markdown: tables (rendered as aligned columns), task lists (rendered as checked/unchecked boxes), strikethrough, and footnotes (rendered and referenceable).

## Acceptance criteria

- [ ] `pulldown-cmark` `Options` extended with `ENABLE_TABLES | ENABLE_TASKLISTS | ENABLE_STRIKETHROUGH | ENABLE_FOOTNOTES`
- [ ] `markdown/blocks.rs` adds block variants for Table, TaskListItem, and footnote references/definitions
- [ ] Tables render as aligned columns in the TUI, readable at typical terminal widths
- [ ] Task list items render `- [x]` as a checked box glyph and `- [ ]` as an unchecked box glyph
- [ ] Strikethrough text (`~~text~~`) renders visually struck through
- [ ] Footnote references render inline with a visible marker, and footnote definitions render where pulldown-cmark places them, both readable
- [ ] Unit tests: table-driven tests asserting `Vec<Block>` shape for sample GFM input covering tables, task lists (checked and unchecked), strikethrough, and footnotes
- [ ] Snapshot test via `ratatui::TestBackend` for a sample document exercising all four GFM features

## Blocked by

- 02-core-markdown-rendering
