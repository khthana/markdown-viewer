Status: ready-for-agent

# Project scaffold + raw-text pager + CI

## Parent

.scratch/mdview-v1/PRD.md

## What to build

Cargo project skeleton for `mdview`, wired to a real (if primitive) end-to-end path: `mdview <FILE>` opens a ratatui+crossterm terminal session, displays the raw text content of FILE in a scrollable pane, supports the full scroll keyset, quits cleanly restoring the terminal, and is backed by a CI workflow running fmt/clippy/test/build across Windows/Mac/Linux. This slice deliberately does not parse Markdown yet — it exists to prove the terminal-init/scroll/quit/CI scaffolding end-to-end before any rendering logic is added on top in later slices.

## Acceptance criteria

- [ ] Cargo.toml created with pinned deps: `ratatui = "0.30.1"`, `crossterm` (ratatui's default backend), `pulldown-cmark`, `clap` (derive), `anyhow` — pulldown-cmark/clap aren't fully exercised by app logic yet, but are present so later slices don't need a dependency-adding PR
- [ ] `cli.rs`: clap `Args` struct with a required `FILE` positional argument
- [ ] `main.rs`: reads FILE, initializes crossterm+ratatui terminal (raw mode, alternate screen), installs a panic hook that restores the terminal before propagating, tears down cleanly on exit
- [ ] `event.rs`: unified `Event` enum + mpsc channel with a crossterm input source (file-watch and image-decode sources are added in later slices — the channel shape should not require a breaking rework when they're added)
- [ ] `app.rs`: `App` state holding raw text lines + `scroll: usize`; pure `handle_key(state, KeyEvent) -> Action` function
- [ ] Scroll keys implemented and functional: `j`/`↓`, `k`/`↑` (1 line), `Space`/`Ctrl-d`/`PageDown` (page down), `Ctrl-u`/`PageUp` (half-page up), `gg`/`Home` (top), `G`/`End` (bottom)
- [ ] `q` and `Ctrl-C` quit cleanly; terminal is fully restored (no leftover alternate screen / raw mode / broken cursor) after exit
- [ ] A missing/unreadable file path prints a clear error message to stderr and exits non-zero, without panicking
- [ ] Unit tests for `app::handle_key` covering every scroll key and quit
- [ ] `.github/workflows/ci.yml`: runs `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `cargo build` on a `windows-latest`/`macos-latest`/`ubuntu-latest` matrix, triggered on every PR and push
- [ ] `cargo build`, `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check` all pass locally on Windows

## Blocked by

None - can start immediately
