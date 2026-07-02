Status: ready-for-agent

# Core Markdown rendering

## Parent

.scratch/mdview-v1/PRD.md

## What to build

Replace the raw-text dump from the scaffold with real CommonMark parsing and styled rendering — headers, paragraphs, bold/italic, ordered/unordered (nested) lists, blockquotes, horizontal rules, links, and plain (unhighlighted) fenced code blocks — rendered with a hardcoded ANSI-16 theme that gives headers strong visual hierarchy. This is the core motivating feature of the whole project: headers must be visually distinct from body text without relying on font size, since a terminal can't vary font size.

## Acceptance criteria

- [ ] `markdown/parser.rs` wraps `pulldown-cmark` (CommonMark only, no GFM extensions yet — those come in a later slice)
- [ ] `markdown/blocks.rs` lowers the `Event` stream into an owned `Vec<Block>` covering: Heading{level,text}, Paragraph, List (ordered/unordered, nested), Blockquote, HorizontalRule, Link, CodeBlock (plain, no syntax highlighting)
- [ ] `markdown/layout.rs` computes virtual row ranges for each block at a given viewport width, including text wrapping
- [ ] `theme.rs`: hardcoded ANSI-16 palette — H1 bold+magenta with a box-drawing rule underneath, H2 bold+cyan with a lighter rule, H3 bold+yellow, H4-6 bold with increasing indent and no rule; restricted to 16-color (not RGB/256-color) so it stays legible on both light and dark terminal backgrounds
- [ ] `ui.rs` renders the real `Vec<Block>` (via `LayoutDoc`) instead of the raw text lines from the scaffold slice, reusing the existing scroll/quit interaction model unchanged
- [ ] Nested lists render with visible indentation per level
- [ ] Links render as underlined/styled text
- [ ] Unit tests: table-driven tests asserting `Vec<Block>` shape for sample CommonMark input covering every block type above
- [ ] Unit tests: layout row-assignment/wrap-height math at at least two different viewport widths
- [ ] Snapshot test via `ratatui::TestBackend` asserting the rendered buffer for a sample document with all block types
- [ ] Manual check: open a markdown file with H1-H6 and confirm each level is visually distinguishable at a glance

## Blocked by

- 01-project-scaffold-raw-pager-ci
