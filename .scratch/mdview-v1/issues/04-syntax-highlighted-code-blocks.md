Status: ready-for-agent

# Syntax-highlighted code blocks

## Parent

.scratch/mdview-v1/PRD.md

## What to build

Fenced code blocks (` ```lang `) render with syntax highlighting by language, using `syntect` + `syntect-tui`, replacing the plain monospace rendering from the core-rendering slice.

## Acceptance criteria

- [ ] `highlight.rs`: lazy-loaded `syntect` `SyntaxSet`/`ThemeSet`, `highlight_code(text, lang) -> Vec<Span>` via `syntect-tui`
- [ ] The blocks/rendering pipeline uses `highlight.rs` for CodeBlock rendering instead of plain text
- [ ] At least Rust, Python, and JavaScript fenced code blocks render with visibly distinct token colors (keywords, strings, comments) when manually inspected
- [ ] A code block with no language annotation (bare ` ``` `) still renders as plain monospace without erroring
- [ ] An unrecognized/unsupported language annotation falls back to plain monospace without erroring
- [ ] Unit test asserting `highlight_code` returns more than one distinct `Style` across a sample snippet containing keywords, strings, and comments (asserts highlighting had an effect, not exact colors)
- [ ] Snapshot test via `ratatui::TestBackend` for a document containing a highlighted code block

## Blocked by

- 02-core-markdown-rendering
