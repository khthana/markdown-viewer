# ADR-0006: Graphics protocol tier and the image worker

Status: Accepted
Implements: `.scratch/mdview-v1/issues/11-graphics-protocol-image-tier.md`

## Context

ADR-0005 drew images as half-blocks, decoded synchronously the first
time a picture came into view and cached per path. That's fine for
half-blocks, whose "encode" is a colour lookup per cell, but a real
graphics protocol (Kitty, Sixel, iTerm2) has to re-encode the whole
picture whenever its area changes, and both the decode and the encode are
slow enough to stall the render loop on a document full of pictures.

## Decisions

- **The detected protocol is used as detected.** `detect_picker` no
  longer forces `Halfblocks`; whatever `Picker::from_query_stdio` reports
  — Kitty, Sixel, iTerm2, or half-blocks — is what the viewer draws with.
  `MDVIEW_PROTOCOL=halfblocks|sixel|kitty|iterm2` overrides it, which is
  how the fallback tiers get exercised on a terminal that supports
  everything. An unrecognised value is ignored rather than fatal, and the
  variable stays out of `cli.rs`: issue #12 owns the CLI surface.
- **One worker thread, two kinds of job.** `image::Job::Decode` reads and
  unpacks a file into a `StatefulProtocol`; `image::Job::Resize` re-encodes
  one for a new area via `ratatui-image`'s `ResizeRequest`. Both answer on
  the same `mpsc` channel every other event source uses, as
  `Event::ImageReady { block_id, protocol }` and
  `Event::ImageResized { block_id, response }`. The render loop never
  waits for either.
- **`ThreadProtocol`'s outbox is drained after each frame, not by a
  thread.** A protocol posts its own resize requests while rendering, into
  a channel it was constructed with — and that channel's message type
  can't carry our `ImageId`, so a shared worker couldn't tell whose
  request it was. Each image therefore gets its own outbox, and
  `Gallery::dispatch_resizes` (called right after `terminal.draw`) tags
  what it finds and forwards it. No extra threads, and no way to route a
  reply to the wrong picture.
- **An image is identified by `ImageId { generation, block }`.** The
  generation increments on every reload, so an answer computed for the
  old document is recognisably stale: block 3 of the old file is not
  block 3 of the new one, and a picture decoded from the old file must not
  be painted over the new one. Stale answers are dropped, not applied.
- **Decoding starts only when an image is fully on screen**, from the
  render pass itself (`Gallery::request`), and a slot is only ever
  requested once — a request per frame would queue a decode per
  keystroke. Opening a document with fifty pictures costs nothing until
  the reader scrolls to them.
- **Every state that isn't "drawable right now" shows the placeholder**:
  pending a decode, *pending an encode*, failed to decode, a path that
  isn't a local file, and an image only partly scrolled into view. The
  encode case is the subtle one — rendering a protocol that still needs
  encoding is what *posts* the encode job, and it paints nothing while
  doing so, so the widget goes out either way and `Gallery::paints_now`
  decides whether the placeholder goes out under it. Without that, every
  image would flash a hole in the document for a frame after decoding and
  after every terminal resize. A failure is never retried.
- **A resize request that can't be delivered fails its image.** The
  request carries the only copy of the decoded picture — the protocol
  hands it over rather than cloning it — so if the worker is gone, that
  image can't come back; its slot becomes `Failed` and shows the
  placeholder rather than staying blank forever.
- **`Event` lost its `Debug`/`Clone` derives.** The two new variants carry
  a decoded protocol and a re-encode result, neither of which is
  cloneable, and neither is worth a hand-written `Debug`.
- **`Gallery::disabled()` is test-only for now.** Production always has a
  picker (detection falls back to half-blocks), so the no-picker path
  exists for the tests — and for issue #12's `--no-images`, which is what
  will make it production code.

## Consequences

- A picture now appears a frame or two after it scrolls into view rather
  than in the same frame. That's the trade for never blocking the loop,
  and the placeholder covers the gap.
- Resizing the terminal re-encodes every visible image, which the worker
  absorbs. Only one request per image can be outstanding — the protocol
  hands its image over with the request and has nothing left to send
  again until the answer comes back — so a fast drag can't pile up work,
  but the picture is a placeholder for the whole round trip.
- **Slots are keyed per image *block*, not per path** (the issue's
  `block_id`), and each keeps its own `ThreadProtocol`, which is where
  ratatui-image holds the encoded picture. Two blocks pointing at the same
  file therefore decode it twice, and slots are only dropped by
  `forget_all` on reload — so memory grows with the number of distinct
  images the reader has actually scrolled past, not with the document's
  size. This reverses ADR-0005's path-keying, which existed to bound a
  cache that no longer exists in that form. If a document with dozens of
  large images ever makes that hurt, evicting slots far from the viewport
  is the obvious next step.
- The half-block tier is now one branch of the same machinery rather than
  a separate path, so ADR-0005's sizing rules (font metrics, the 15-row
  cap, the fully-visible rule) apply unchanged to every protocol.
- Issue #12's `--no-images` is a one-line change: build the `Gallery`
  with `disabled()` and the `Sizing` with `text_only()`.
