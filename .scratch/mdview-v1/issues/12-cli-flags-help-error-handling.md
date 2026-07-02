Status: ready-for-agent

# CLI flags, help overlay, and file-error handling

## Parent

.scratch/mdview-v1/PRD.md

## What to build

Round out the CLI surface with `--no-color`, `--no-images`, `--theme <dark|light>`, `--help`, `--version`, an in-app `?` help overlay listing all keybindings, and confirm error handling for missing/unreadable files is clear and non-panicking across the now-complete feature set.

## Acceptance criteria

- [ ] `cli.rs`: clap `Args` extended with `--no-color` (flag), `--no-images` (flag), `--theme <dark|light>` (optional, defaults to the existing dark-safe palette)
- [ ] `--no-color` disables all ANSI styling (headers, syntax highlighting, search highlight) and produces plain-text output suitable for piping
- [ ] `--no-images` forces the alt-text placeholder tier regardless of detected terminal capability, skipping half-block/protocol decode entirely
- [ ] `--theme light` adjusts the `theme.rs` palette to a variant legible on light terminal backgrounds; `--theme dark` (or omitted) keeps the current default
- [ ] `mdview --help` prints usage covering the FILE argument and all flags
- [ ] `mdview --version` prints the crate version
- [ ] `?` opens a help overlay listing every keybinding (scroll, TOC, search, reload, quit); any key closes the overlay
- [ ] A missing file path produces a clear, specific error message (not a panic, not a generic Rust error dump) and a non-zero exit code
- [ ] An unreadable file (permissions error) produces an equivalently clear error message
- [ ] Unit tests for CLI arg parsing covering each flag combination

## Blocked by

- 04-syntax-highlighted-code-blocks
- 09-image-alt-text-placeholder
