# mdview

A terminal Markdown viewer: syntax-highlighted code, an outline sidebar,
in-document search, live reload as you edit, and images drawn with your
terminal's own graphics protocol when it has one.

Reads one file, renders it in place, and gets out of the way.

## Install

### From crates.io

```sh
cargo install mdview-term
```

The package is `mdview-term` because `mdview` was taken on crates.io in
2020 by an unrelated tool. The command it installs is `mdview`.

### Prebuilt binaries

Every tagged release attaches binaries for Windows, macOS (Intel and
Apple silicon) and Linux to the [GitHub Releases
page](https://github.com/khthana/markdown-viewer/releases). Download the
archive for your platform, extract it, and put `mdview` somewhere on your
`PATH` — no Rust toolchain needed.

### From source

```sh
git clone https://github.com/khthana/markdown-viewer
cd markdown-viewer
cargo install --path .
```

## Usage

```sh
mdview notes.md
```

| Flag | What it does |
| --- | --- |
| `--no-color` | Plain text plus bold/italic/reverse only. Implies `--no-images`. |
| `--no-images` | Every image shows its alt text instead of being drawn. |
| `--theme <dark\|light>` | Which palette to render with. Defaults to `dark`. |
| `--help` / `--version` | Usage, and which build you have. |

`NO_COLOR` in the environment is honoured exactly like `--no-color`.

Piping or redirecting the output dumps the rendered document as plain
text at 80 columns instead of starting the pager, so
`mdview notes.md | less` and `mdview notes.md > out.txt` both do
something sensible.

### Keys

`j`/`k` scroll, `Space`/`Ctrl-d` page down, `gg`/`G` jump to the ends,
`Tab` toggles the outline, `/` searches, `n`/`N` walk the matches, `Esc`
backs out of whatever is open, `r` reloads, `q` quits.

**Press `?` for the full list.** The overlay is built from the keymap
table, and tests check the two against each other in both directions, so
a key the app honours can't go undocumented — which is also why this
README doesn't try to reproduce the list.

## Images

`mdview` draws pictures with the best tier your terminal supports:

1. **The terminal's own graphics protocol** (Kitty, iTerm2, Sixel) — a
   real image.
2. **Half-block characters** — a coloured approximation, for terminals
   without a protocol.
3. **A placeholder** — `🖼 [alt text]`, falling back to the file name
   when the image has no alt text. This is what you get when images are
   off, when the file can't be read or decoded, and when the reference
   points at a URL: remote images are never fetched.

Set `MDVIEW_PROTOCOL` to `kitty`, `iterm2`, `sixel` or `halfblocks` to
force a tier instead of detecting one.

## Live reload

The file is watched while you read it. Saving from an editor redraws the
document and keeps your place, anchored to the nearest heading rather
than to a raw line number, so an edit above your position doesn't move
the text out from under you. `r` reloads by hand.

## License

MIT OR Apache-2.0, at your option. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE).
