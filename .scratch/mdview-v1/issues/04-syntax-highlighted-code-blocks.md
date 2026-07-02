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

## Comments

Implemented without the `syntect-tui` dependency. Its latest published version (3.0.6) pins `ratatui ^0.29.0` as a real dependency, which is a different crate instance than this project's `ratatui 0.30.1` — `into_span`'s returned `Span` doesn't type-check against our pipeline, and there's no newer version targeting 0.30. Its default features (`deep-defaults`) also pull in `onig` (C/Oniguruma), which is a known slow/fragile build on `windows-latest` CI. Downgrading the project to ratatui 0.29 to keep it wasn't worth discarding completed work over.

Used `syntect` directly (`default-features = false, features = ["default-fancy"]` to keep the pure-Rust `fancy-regex` engine, avoiding `onig`) with a ~15-line local `syntect::highlighting::Style -> ratatui::style::Style` conversion in `highlight.rs` instead. Foreground color only — background is dropped since a theme's background fights the terminal's own and looks wrong in a pager.
