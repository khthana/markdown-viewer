# ADR-0005: Half-block image tier and how sizing reaches the layout

Status: Accepted
Implements: `.scratch/mdview-v1/issues/10-halfblock-image-rendering.md`

## Context

ADR-0004 gave every image its own block and a fixed one-row placeholder.
Drawing the picture instead needs three things the placeholder tier
didn't: the terminal's cell size (to turn pixels into rows), the image's
pixel size (to know how many), and somewhere to keep the decoded,
protocol-encoded result between frames. `layout` is a pure function of
`(blocks, width)` called on every frame, so none of that could live
inside it.

## Decisions

- **`image::Sizing` is measured once per document load and travels with
  it.** It holds the terminal's `FontSize`, the document's directory, and
  the pixel dimensions of each referenced image, read from file *headers*
  only (`ImageReader::into_dimensions`) — no pixels are decoded to lay
  out a page. `layout`/`render_lines` take `&Sizing` as a third argument
  rather than reaching for global state, so tests stay pure and the
  text-only tier is just `Sizing::text_only()`.
- **Image paths resolve against the document's own directory**, not the
  process's working directory — `![](diagram.png)` next to `notes.md`
  means `notes.md`'s neighbour. Anything containing `://` is left alone:
  nothing in this viewer fetches over the network, so remote images stay
  placeholders.
- **A drawable image reserves blank rows; everything else keeps the
  one-row placeholder.** `layout` asks `Sizing::draws(path)`, so an image
  that couldn't be measured — missing, corrupt, remote, or on a terminal
  with no picker — degrades to exactly the ADR-0004 behavior. The
  reserved rows are left empty by the *text* pass, since anything written
  there would show through the picture; `ui`'s image pass is what fills
  them. Both passes run through the same `Sizing`, which is what keeps
  `render_lines` and `layout` agreeing on the document's height — the
  invariant `render_lines` documents.
- **Only top-level images are drawable.** `blocks::image_paths` ignores
  images nested in a blockquote or list item: those rows carry the
  container's own prefix (`\u{2502} `, a bullet) down their left edge,
  and the picture pass paints whole rectangles over rows it assumes are
  blank. They keep the placeholder rather than becoming a gap with a
  picture painted over the quote marks.
- **Row count comes from the terminal's font metrics, capped at 15**
  (`image::rows_for`): cells are about twice as tall as they are wide, so
  the picture's row count depends on the terminal's cell size, not on the
  image's aspect ratio alone. An image too wide for the pane has its
  height reduced by the same factor its width must shrink by. The cap
  exists so one large picture can't bury the text around it.
- **`image::Gallery` keeps one decoded image per path, failures
  included, replacing it when the pane changes shape.** Decoding and
  fitting an image is far too slow to redo per keystroke, and a corrupt
  file must not be reopened sixty times a second just to fail again;
  keying by path alone (rather than by path *and* area) also means a
  session of resizing can't pile up stale copies. Dropping the cache on
  reload is `reload_preserving_position`'s own job — the same path may be
  a different picture now, and pairing that with the re-measure by hand
  at each call site was a bug waiting to happen.
- **`ui::render` takes a `Screen` struct** (app, blocks, layout, toc,
  matches, sizing) plus `&mut Gallery`. Six positional parameters that
  always travel together and are always built at the same width are worth
  one named type.
- **An image is painted only when its rows are *fully* on screen, and
  shows its placeholder otherwise.** A partially scrolled image would
  need re-fitting — a decode — on every scroll tick; that's the PRD's
  explicit v1 scope cut, applied a tier early because it costs nothing to
  honour here. The placeholder matters: the rows are blank until
  something is drawn into them, so without it a half-scrolled picture
  would read as a hole in the document.
- **The protocol is clamped to half-blocks even when the terminal
  supports better.** `image::detect_picker` runs `Picker::from_query_stdio`
  and then forces `ProtocolType::Halfblocks`; Sixel/Kitty/iTerm2 output
  is issue #11's job, and shipping an untested protocol path early would
  be worse than shipping the tier that was actually asked for. A terminal
  that doesn't answer the query still gets half-blocks with the picker's
  default cell size, since half-blocks are just coloured characters.
  (The issue names `Picker::detect()`; ratatui-image 11 has no such
  function — `from_query_stdio` is its equivalent.)
- **Detection runs after the alternate screen is entered but before the
  input thread starts.** The query writes an escape sequence and waits for
  the reply on stdin, so a reader already polling stdin would swallow it.
  The ordering is enforced by types, not by comments:
  `event::PendingSources::watching` starts the watcher (before the screen,
  so its warning is visible) and only `start_input` — consuming it —
  yields the `Sources` the loop can `recv` from.
- **The no-picker path is kept even though detection always succeeds.**
  `detect_picker` falls back to `Picker::halfblocks()`, so
  `Sizing::text_only()` and `Gallery::new(None)` are reachable only from
  tests today. They stay because issue #12's `--no-images` and
  `--no-color` need exactly this seam, and because every test that isn't
  about images wants the text-only tier.
- **`ratatui-image` is built without default features.** The default
  `chafa-dyn` feature links libchafa through `pkg-config`, which isn't
  available on the primary target (Windows); only `crossterm` and
  `image-defaults` are enabled.

## Consequences

- A drawn image's alt text is no longer in `LayoutDoc.rows`, so `/` can't
  find it and a reload can't anchor to it (ADR-0001, ADR-0003). That's the
  honest consequence of the rows being blank — the reader can't see that
  text either. Images that fall back to the placeholder stay searchable.
- Documents are re-measured on every reload, which re-reads image headers.
  That's one small read per referenced image per save, and it's what
  makes a picture swapped on disk show up at its new size.
- Issue #11 lifts the half-block clamp and moves the decode to a worker
  thread; the `Gallery` cache key and the fully-visible rule are the
  seams it plugs into.
