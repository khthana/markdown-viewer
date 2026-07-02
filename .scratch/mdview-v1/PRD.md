Status: ready-for-agent

# PRD: `mdview` — Terminal Markdown Viewer (v1)

## Problem Statement

The user wants to read Markdown files from the Windows command line, but existing terminal Markdown viewers (e.g. `glow`) are hard to read in practice — headers don't stand out enough from body text, so long documents feel flat and hard to scan. There's no lightweight, fast, native Windows-CLI tool that gives Markdown files the kind of visual structure an editor's preview pane gives them, without leaving the terminal or opening a GUI window.

## Solution

A single-binary Rust CLI, `mdview`, that opens one Markdown file as an interactive terminal pager. It renders full CommonMark + GitHub Flavored Markdown (tables, task lists, strikethrough, syntax-highlighted code blocks) with a strong visual hierarchy for headers (bold, distinct ANSI colors per level, box-drawing rules) built entirely from terminal styling — no GUI, no webview. It adds the navigation an editor preview gives you that a one-shot `print`-style tool can't: a jump-to-heading outline sidebar, in-document search, and automatic reload when the file changes on disk. Where the terminal supports it, embedded images render as real graphics (Sixel/Kitty/iTerm2 protocol), degrading gracefully through colored half-block glyphs down to an alt-text placeholder on terminals with no graphics support at all (the common case on default Windows consoles). The tool is cross-platform (Windows/Mac/Linux) via `crossterm`, distributed both as prebuilt GitHub Release binaries and via `cargo install`.

## User Stories

1. As a Windows developer, I want to run `mdview file.md` from cmd.exe or PowerShell, so that I can read a Markdown file without opening an editor or browser.
2. As a reader of long documents, I want headers to be visually distinct (bold, colored, ruled) at every level (H1–H6), so that I can scan document structure at a glance.
3. As a reader, I want tables rendered as aligned columns, so that tabular data stays readable in a monospace terminal.
4. As a reader, I want task list items (`- [x]` / `- [ ]`) rendered as checked/unchecked boxes, so that I can see completion status at a glance.
5. As a reader, I want strikethrough text (`~~text~~`) rendered visually struck through, so that GFM documents render faithfully.
6. As a reader, I want footnotes rendered and referenceable, so that annotated documents remain readable.
7. As a reader, I want fenced code blocks syntax-highlighted by language, so that code embedded in documentation is as readable as it is in an editor.
8. As a reader of a long document, I want to scroll line-by-line (`j`/`k`/arrow keys), so that I can read at my own pace.
9. As a reader, I want to page up/down and half-page up/down (`Space`/`Ctrl-d`/`Ctrl-u`/`PageUp`/`PageDown`), so that I can move through long documents quickly.
10. As a reader, I want to jump straight to the top or bottom of the document (`gg`/`G`/`Home`/`End`), so that I don't have to scroll through the whole file to reach an end.
11. As a reader of a long document, I want a toggleable outline sidebar (`Tab`) listing every heading in the file, so that I can see the document's structure without scrolling through it.
12. As a reader, I want to select a heading in the outline sidebar and jump directly to it (`Enter`), so that I can navigate long documents without manual scrolling.
13. As a reader, I want the outline sidebar to stay open after a jump (until I close it explicitly), so that I can peek at the outline, jump, and jump again without reopening it each time.
14. As a reader, I want to search the document's text (`/`), so that I can find a specific term without scrolling manually.
15. As a reader, I want to jump between search matches (`n`/`N`), so that I can review every occurrence of a term.
16. As a writer actively editing a Markdown file, I want the viewer to automatically detect when I save the file and re-render it, so that I get a live-preview experience without switching windows or re-running the command.
17. As a writer using auto-reload, I want my scroll position preserved across a reload (anchored to the nearest heading), so that a small edit doesn't throw me back to the top of the document.
18. As a writer using auto-reload, I want my active search query and match position preserved across a reload where possible, so that I don't lose my place mid-search.
19. As a user, I want a manual reload key (`r`) in addition to auto-reload, so that I can force a refresh immediately if the debounce hasn't fired yet or after a terminal resize.
20. As a reader on a terminal that supports Sixel, Kitty, or iTerm2 graphics protocols, I want images referenced in the Markdown (`![alt](path)`) to render as real images, so that documents with diagrams or screenshots are genuinely readable.
21. As a reader on a terminal without graphics protocol support but with 16-color support (the default on most Windows consoles), I want images to render as colored half-block glyphs, so that I still get a visual approximation of the image rather than nothing.
22. As a reader on a terminal with no color/graphics support at all, or when I've passed `--no-images`, I want images to fall back to an alt-text placeholder (e.g. `🖼 [alt text]`), so that the document remains readable and scriptable.
23. As a reader with many images in a document, I want images to decode lazily (only once scrolled into view) on a background thread, so that opening a large document doesn't stall the UI.
24. As a user piping output or working in an accessibility context, I want a `--no-color` flag that disables ANSI styling, so that output stays usable in plain-text contexts.
25. As a user on an unusual terminal background, I want a `--theme <dark|light>` flag to override the default color assumption, so that text stays legible if auto-detection guesses wrong.
26. As a new user, I want `mdview --help` and `mdview --version` to work, so that I can discover usage and confirm the installed version.
27. As a user who quits the viewer, I want `q` or `Ctrl-C` to exit cleanly and restore my terminal to its normal state, so that my shell isn't left in a broken state after quitting.
28. As a new user, I want a `?` help overlay listing all keybindings, so that I don't have to memorize or look up the controls elsewhere.
29. As a user who points `mdview` at a missing or unreadable file, I want a clear error message (not a panic or raw stack trace), so that I understand what went wrong.
30. As a Mac or Linux user, I want `mdview` to run identically to the Windows version, so that the same tool works across every machine I use.
31. As a user with Rust already installed, I want to install via `cargo install mdview`, so that I don't need to download a separate binary.
32. As a user without a Rust toolchain, I want to download a prebuilt binary from GitHub Releases, so that I can use the tool without installing Rust.
33. As a maintainer, I want CI to run `cargo fmt --check`, `cargo clippy -D warnings`, and `cargo test` across Windows/Mac/Linux on every PR, so that regressions are caught before merge.
34. As a maintainer, I want tagging a release to automatically build and attach binaries for all supported platforms to a GitHub Release, so that distribution doesn't require manual builds.
35. As a maintainer, I want a separate `cargo publish` step for crates.io, so that the two distribution channels (binaries vs. crates.io) can be released independently if needed.

## Implementation Decisions

- **Markdown parsing**: `pulldown-cmark`, with GFM extensions enabled (`ENABLE_TABLES`, `ENABLE_TASKLISTS`, `ENABLE_STRIKETHROUGH`, `ENABLE_FOOTNOTES`). Chosen over `comrak` for its lighter weight and flat `Event`-stream API, which lowers directly into this app's owned block IR without arena-lifetime bookkeeping. It's also what the closest prior art (`mdcat`) is built on.
- **TUI framework**: `ratatui` 0.30.x + `crossterm` backend. `crossterm` is used explicitly so the app is cross-platform (Windows/Mac/Linux), even though Windows is the primary target.
- **Image rendering**: `ratatui-image` (pinned compatible with `ratatui = "^0.30.1"`), which provides terminal-capability auto-detection (`Picker::detect`) and protocol-aware widgets (`Image`/`StatefulImage`/`ThreadProtocol`) purpose-built for embedding inside a ratatui buffer. `viuer` was considered and rejected as not ratatui-aware.
- **Syntax highlighting**: `syntect` (same engine `bat` uses) paired with `syntect-tui` to convert highlight ranges directly into ratatui `Span`s.
- **File watching**: `notify-debouncer-full` (not raw `notify`). The watcher targets the **parent directory** (non-recursive), filtering events by the canonicalized target filename, and debounces (~150–300ms) before firing a single reload event. This is required because most editors save via write-temp-then-rename or delete+recreate, which breaks a watch registered directly on the file path after the first save.
- **CLI parsing**: `clap` (derive API).
- **Error handling**: `anyhow` throughout, since this is a binary rather than a library — `.context()` on file I/O, terminal init, and watcher setup.
- **Supporting crates**: `unicode-width` (display-width-aware layout math for box-drawing and indentation), `image` (raster decoding, already a transitive `ratatui-image` dependency).
- **Distribution/CI**: `cargo-dist` generates the release workflow (build → host → publish per target). Two separate GitHub Actions workflows: `ci.yml` (fmt/clippy/test/build matrix on every PR, including headless snapshot tests) and `release.yml` (tag-triggered, builds and attaches binaries for `x86_64-pc-windows-msvc`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`). `cargo publish` to crates.io is a distinct job/step, not conflated with binary release.

### Module map

- `cli.rs` — clap `Args` struct (`FILE`, `--no-color`, `--no-images`, `--theme`).
- `app.rs` — `App` state, `Mode` enum, pure `handle_key(state, KeyEvent) -> Action` mapping, `apply(Action)`.
- `event.rs` — unified `Event` enum over one `mpsc` channel; owns the input thread, the file watcher, and image-decode-worker completions. All three async sources funnel into one channel consumed by a single `recv → apply → draw` loop in `main.rs`.
- `markdown/parser.rs` — `pulldown-cmark` wrapper with GFM options.
- `markdown/blocks.rs` — lowers the parser's `Event` stream into an owned `Vec<Block>`, collecting the `Vec<TocEntry>` in the same pass (headings are just another block kind).
- `markdown/layout.rs` — `Vec<Block>` + viewport width → `LayoutDoc { rows, block_row_index, total_height }`, assigning each block a virtual row range and reserving blank rows for images (sized from the image picker's font-cell metrics).
- `highlight.rs` — lazy-loaded `syntect` `SyntaxSet`/`ThemeSet`, `highlight_code()` via `syntect-tui`.
- `toc.rs` — heading navigation only: resolves a `TocEntry` to a clamped scroll offset against the current `LayoutDoc`.
- `search.rs` — match-finding over `LayoutDoc`, next/prev navigation with wraparound.
- `watch.rs` — debounced directory watch, described above.
- `image.rs` — `Picker::detect()` at startup (env-var guess → terminal query → halfblocks fallback), lazy per-block decode on a worker thread via `ThreadProtocol`, results delivered as `Event::ImageReady{block_id, protocol}`.
- `theme.rs` — hardcoded ANSI-16 palette: bold+magenta H1 with a box-drawing rule, bold+cyan H2 with a lighter rule, bold+yellow H3, bold+indent for H4–6. Restricted to 16-color (not RGB/256-color) so it stays legible on both light and dark backgrounds without per-terminal calibration or a config file.
- `ui.rs` — `draw(frame, &App)`: two render passes per frame — text into the ratatui buffer first, then any image blocks whose row range intersects the viewport rendered into directly-computed `Rect`s via `StatefulImage`. This two-pass split exists because graphics-protocol output is raw escape sequences at live cursor positions, not part of ratatui's `Buffer` — it cannot be composited the way styled text can.

### Data flow

Parse → lower to `Vec<Block>` (+ `Vec<TocEntry>` in the same pass) → layout pass assigns virtual rows → `App` holds `LayoutDoc` + `scroll` + `Mode` + search/TOC sub-state + detected `ProtocolKind` → each frame renders the visible row slice, then composites any in-viewport images.

**Reload-without-losing-position**: on `Event::FileChanged`, before discarding the old `LayoutDoc`, compute an anchor (nearest heading at or above current `scroll`, or block content if no heading exists yet). Re-parse, re-lower, re-layout, then resolve the anchor against the new blocks by heading level+text match (or content hash), and set `scroll` to its new position. If the anchor was deleted, clamp to the new `total_height` rather than snapping to 0. Re-run the active search query against the new layout, preserving match index where possible.

### Image fallback tiering (explicit decision)

Three tiers, not two: **protocol (Sixel/Kitty/iTerm2) → colored half-block glyphs → alt-text placeholder**. `ratatui-image` provides the half-block tier for free, and it will be the tier most Windows users actually see day-to-day, since default Windows consoles support neither Sixel nor Kitty protocols reliably. Alt-text is reserved for `--no-images`, `NO_COLOR`, non-tty/redirected output, or an actual decode failure. Partial-image-crop-on-scroll (an image half-scrolled off-screen re-encoding live) is explicitly out of scope for v1 — the placeholder renders until an image block is fully in view, avoiding per-tick re-encode cost.

### Keybindings

`j`/`↓` `k`/`↑` scroll · `Space`/`Ctrl-d`/`PageDown` page down · `Ctrl-u`/`PageUp` half-page up · `gg`/`Home` top · `G`/`End` bottom · `Tab` toggle TOC · `↑`/`↓`/`Enter` navigate/jump within TOC · `/` search · `n`/`N` next/prev match · `Esc` exit search/close TOC/clear highlight · `r` manual reload · `q`/`Ctrl-C` quit · `?` help overlay.

## Testing Decisions

Good tests here assert external behavior (parsed structure, computed layout rows, resolved scroll offsets, action mappings) rather than internal rendering mechanics — the goal is that these tests keep passing across TUI/rendering refactors as long as the observable behavior is unchanged.

- **Unit tests** (no terminal required):
  - `markdown::blocks` — table-driven tests asserting `Vec<Block>` shape for CommonMark/GFM sample inputs (tables, task lists, strikethrough, images).
  - `markdown::layout` — row-assignment and wrap-height math at various widths.
  - `toc` — heading collection, row resolution, jump clamping.
  - `search` — match-finding, case-insensitivity, next/prev wraparound.
  - `app::handle_key` — every keybinding table-tested as a pure `(Mode, KeyEvent) -> Action` mapping, no real terminal needed.
- **Reload-anchor resolution tests**: simulate an old `Vec<Block>` + scroll position, mutate the blocks (rename/delete/add a heading), assert the anchor resolves sanely (including the deleted-anchor clamp case). This is called out explicitly since scroll-position loss on reload was flagged as the most user-visible failure mode of the live-reload feature.
- **Snapshot tests**: feed a small `App` state through `ui::draw` using `ratatui::TestBackend`, assert/diff the resulting buffer for plain view, TOC-open, and search-highlighted states. Runs headlessly in CI across all three OSes, catching layout regressions cheaply. No prior art in this repo (greenfield project) — pattern follows ratatui's own documented `TestBackend` testing approach.
- **Manual/exploratory only** (not automated, tracked as a checklist rather than code):
  - Actual terminal graphics protocol rendering, across Windows Terminal (Sixel), iTerm2, Kitty, WezTerm, and plain cmd.exe/PowerShell 5.1 as the must-degrade-gracefully baseline.
  - Real editor atomic-save behavior against the watcher (VS Code, Notepad, vim swap files).
  - Palette contrast on real dark vs. light terminal backgrounds.

## Out of Scope

- Directory/multi-file browsing (file-tree sidebar to switch between Markdown files) — v1 opens exactly one file per invocation.
- Any config file or theme customization — v1 ships one hardcoded ANSI-16 theme; `--theme <dark|light>` is a binary override, not a customization system.
- GUI/webview rendering mode — explicitly rejected during design; this is a terminal-only tool.
- Partial-image-crop-on-scroll (re-encoding an image that's half-scrolled off the viewport edge).
- Any non-Markdown input formats.

## Further Notes

- Windows is the primary target platform, but will mostly exercise the half-block/alt-text fallback tiers rather than true graphics protocols — real Sixel/Kitty rendering will be most visible when testing on Mac (iTerm2/WezTerm) or Linux (Kitty/WezTerm/xterm+Sixel). This should not be read as a bug during Windows manual testing.
- Crate versions to pin at implementation start: `ratatui = "0.30.1"`, `ratatui-image = "11.0.6"`. `clap` v4 currency should get a quick confirmation at implementation time (not independently re-verified during design).
- Binary name proposed as `mdview`; confirm availability on crates.io before publishing.
