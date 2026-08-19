# ADR-0007: One palette, one keybinding table, and errors before the screen

Status: Accepted
Implements: `.scratch/mdview-v1/issues/12-cli-flags-help-error-handling.md`

## Context

Until now every renderer picked its own colours: `theme.rs` held the
heading and search styles, but `layout.rs` hardcoded the blockquote grey
and the footnote-marker cyan, and `highlight.rs` named its syntect theme
inline. `--no-color` and `--theme light` have no single place to be
honoured against that.

Two more gaps closed here have the same shape. The keybindings existed
only inside `handle_key`, so a `?` overlay would be a second, hand-copied
list of them. And a bad file path was reported by `anyhow`'s `Debug`
formatting *after* the alternate screen had been entered and restored, so
the reader saw a pager flash and then an `os error 2` chain.

## Decisions

- **`theme::Palette` owns every colour in the app.** `Dark` (the
  behaviour of every version so far), `Light`, and `Plain` for
  `--no-color`. Nothing outside `theme.rs` names a `Color` any more — the
  one exception is `highlight.rs` converting syntect's own RGB output,
  which is unreachable when the palette names no theme.
- **Palettes differ in colour and never in geometry.** Which heading
  levels draw a rule row, and how far each indents, are decided outside
  the per-palette match; `heading_style` picks the rule's *colour* inside
  the test for whether there is a rule at all, so a palette can't name a
  colour for a level that draws none. `every_palette_lays_out_the_same_rows`
  holds the whole document to this: switching palettes must not move a
  single row, or every scroll offset, TOC target and search match would
  shift with it.
- **`Plain` keeps attributes, and drops only colour.** Bold, italic and
  reverse video stay, because the TOC selection and the current search
  match are reverse-video and nothing else distinguishes them; a strictly
  SGR-free rendering would need textual markers, and those would change
  column offsets that ADR-0003's search anchors depend on.
- **Syntax highlighting is the documented exception to ANSI-16.**
  `Palette::code_theme` names an RGB syntect theme (`base16-ocean.dark` /
  `.light`) or `None`. Everywhere else the 16-colour restriction holds so
  the terminal's own theme can remap it, but code is the one place fine
  colour distinctions earn their keep, and `None` is what makes
  `--no-color` reach inside a fenced block.
- **`--no-color` implies `--no-images`.** A half-block picture is nothing
  but coloured cells and a protocol picture isn't text at all, so a run
  that promises no colour drops to ADR-0004's alt-text tier. `Rendering`
  in `cli.rs` is the single home for that policy, alongside `NO_COLOR`
  from the environment, which is honoured exactly like the flag.
- **`cli` depends on `theme`, never the reverse.** Resolving a
  `ThemeChoice` to a `Palette` lives in `cli.rs`, so the render layer
  knows nothing about argument parsing.
- **Redirected output is dumped as plain text.** A pager needs a terminal
  to page in, so when stdout isn't a tty the document is written to
  stdout one row per line and the process exits — reusing `LayoutDoc.rows`,
  which is the same rendered text with styling already stripped, so a pipe
  receives what the screen would have shown.
- **`app::KEYBINDINGS` is the only list of keys.** Each row carries the
  keys it advertises paired with the `Action` each must produce, and two
  tests hold the table and `handle_key` to each other in both directions:
  every advertised key does what the overlay says, and every key the app
  honours is advertised. The table is deliberately not the dispatch —
  `handle_key` also answers the `gg` sequence and free text, which don't
  fit one key to one action.
- **The overlay swallows the next key, whatever it is.** Except `Ctrl-C`,
  which is the app's universal escape hatch and is checked first. While
  it's up, images fall back to their placeholders: a picture painted by
  the terminal's own graphics protocol would sit on top of the overlay
  rather than under it.
- **A file is checked before the alternate screen is entered.**
  `reload::check_readable` opens the path up front so a bad one prints one
  plain line — `mdview: no such file: notes.md` — and exits non-zero,
  with no pager flash. Messages are phrased for the reader rather than
  chained from `io::Error`, and a directory is reported as a directory
  because Windows reports opening one as a permission error.

## Consequences

- Every function that renders now carries a `Palette`. Inside `layout.rs`
  it travels with the width and the image sizing as one private `Ctx`;
  `ui::Screen` carries it for the frame, and must be the palette its
  `LayoutDoc` was built with.
- ADR-0004's reference to a free `theme::image_placeholder_style()` is
  superseded: it's `Palette::image_placeholder_style` now. The decision it
  records — that the placeholder's styling lives in `theme.rs` rather than
  inline in the layout pass — is unchanged and stronger.
- `--theme light` is a palette, not a detection: nothing here inspects the
  terminal's background colour. A reader on a light terminal has to ask.
- The dump path renders at a fixed 80 columns and always uses the
  alt-text tier. It is not a formatter — no `--width`, no reflow to the
  pipe's consumer.
- `Esc` still does nothing in normal mode, though the PRD lists it as
  "exit search / close TOC / clear highlight". The overlay therefore
  documents a keymap that is itself incomplete; closing that gap is its
  own change.
