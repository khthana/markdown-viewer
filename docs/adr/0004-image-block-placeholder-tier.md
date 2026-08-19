# ADR-0004: Images as blocks, and the alt-text placeholder tier

Status: Accepted
Implements: `.scratch/mdview-v1/issues/09-image-alt-text-placeholder.md`

## Context

Markdown treats `![alt](path)` as *inline* content: pulldown-cmark emits
it inside whatever paragraph, heading, link, or table cell it appears in,
and until now its alt text simply fell through as ordinary text. The PRD,
though, plans for images to occupy their own row range so later tiers can
paint half-block glyphs (#10) or graphics-protocol output (#11) into a
rectangle. Escape sequences and half-block art can't be composited into
the middle of a line of styled text, so the layout has to give an image
rows of its own — where the surrounding structure allows it.

## Decisions

- **An image becomes a block only where a block can legally go.** The
  lowering pass tags every open inline frame as `Interruptible` (a
  paragraph, or a tight list item's own text) or `InlineOnly` (heading
  text, a link's label, a table cell, an emphasis span). At the image's
  end event, an `Interruptible` frame yields a top-level
  `Block::Image { alt, path }`; anything else yields
  `Inline::Image { alt, path }`, which renders as the same placeholder
  inline with the text. Hoisting unconditionally emptied headings (and
  their TOC entries), dropped the label out of `[![badge](b.svg)](url)`,
  and pushed table-cell images out of the table entirely.
- **Inside a paragraph, an image splits it** into
  `[Paragraph(before), Image, Paragraph(after)]`, preserving reading
  order while giving the image its own row range. A paragraph holding
  nothing but an image produces no empty `Paragraph` block —
  `End(Paragraph)` skips an empty inline buffer. (Empty paragraphs were
  already reachable for other reasons and already occupied zero rows, so
  this changes no existing layout.)
- **`alt` is stored exactly as the document wrote it**, empty string
  included; the displayed label is derived at render time. Lowering stays
  a faithful translation of the source, and the fallback rule can change
  in a later tier without touching the parser.
- **`blocks::image_label` is the single source of the label**: alt text
  if it has any non-whitespace content, else the file's name
  (`photos/holiday.jpg` -> `holiday.jpg`), else the raw path, else the
  word `image`. Whitespace-only alt is treated as absent, and the last
  fallback exists so `![]()` can't render an empty `[]`. Both the layout
  pass and `flatten_plain_text` (TOC entries, reload anchors) go through
  it, so an image is called the same thing everywhere.
- **An image reserves exactly one row, at any width.** The placeholder is
  rendered as a single unwrapped line: a wrapping placeholder would make
  the document's total height depend on the terminal's width in a way
  that shifts every anchor below it (ADR-0002, ADR-0003) whenever the
  window is resized. #10 replaces this with a height derived from the
  picker's font-cell metrics.
- **The placeholder is built in the layout pass, not in `ui.rs`.** The
  PRD's module map gives `layout.rs` "blank rows for images" and `ui.rs`
  the image pass, which is right for #10/#11, where images are escape
  sequences painted into computed `Rect`s. In this tier the placeholder
  *is* text, so it belongs with all the other text rendering — that keeps
  `LayoutDoc.rows` truthful (see below) and avoids a second copy of the
  row-positioning logic. #11's real two-pass render still lands in
  `ui.rs`.
- **This tier never opens the file.** Nothing here can fail on a missing,
  unreadable, or malformed image, so a broken reference renders the same
  placeholder as any other image — no error path, no `anyhow` context.
- **The placeholder text lands in `LayoutDoc.rows`** like any other
  rendered row, which makes an image's alt text searchable (`/`) for free
  — a direct consequence of ADR-0001's decision to search rendered rows
  rather than the Markdown source.
- **`theme::image_placeholder_style()`** (dim + italic) keeps the
  placeholder's colors in `theme.rs` with every other palette decision,
  rather than inline in the layout pass. (Superseded in form by ADR-0007:
  it is `Palette::image_placeholder_style` now, since the palette is
  chosen per run. The decision itself stands.)

## Consequences

- A document where an image sits mid-sentence now renders as three blocks
  instead of one paragraph, so the text before and after the image starts
  on its own row. That's the intended shape, not a wrapping artifact.
- Images in headings, links, table cells, and emphasis stay put and stay
  inline — they will *not* be upgraded to real graphics by #10/#11, since
  they never get rows of their own. That's the deliberate trade for not
  destroying the structure that contains them.
- `search` can match alt text, and `reload`'s content anchor can pin to a
  placeholder row — both fall out of the shared `rows` representation.
- #10 changes the reserved height and the render path but not the block
  shape: `Block::Image` already carries everything a decoder needs.
