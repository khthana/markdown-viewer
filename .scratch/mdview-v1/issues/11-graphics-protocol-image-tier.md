Status: ready-for-agent

# True graphics protocol image tier (Sixel/Kitty/iTerm2) + lazy background decode

## Parent

.scratch/mdview-v1/PRD.md

## What to build

The top image tier — real image rendering via Sixel/Kitty/iTerm2 graphics protocols on terminals that support them, decoded lazily on a background thread so opening a document with many images doesn't stall the UI.

## Acceptance criteria

- [ ] `Picker::detect()` capability result is used to select the protocol tier (Kitty/Sixel/iTerm2) over the half-block tier when available
- [ ] Image blocks decode lazily — only once their row range first scrolls fully into the viewport, not eagerly for the whole document at open time
- [ ] Decode + protocol-encode work happens on a worker thread via `ratatui-image`'s `ThreadProtocol`, with completion delivered as `Event::ImageReady{block_id, protocol}` through the unified event channel — the render loop must not block waiting for a decode
- [ ] While an image's decode is pending, its block shows the placeholder as a loading stand-in
- [ ] An image block that is only partially scrolled into view (not fully visible) continues showing its placeholder rather than attempting a partial/cropped render (explicit v1 scope cut — avoids re-encoding on every scroll tick)
- [ ] Manual verification checklist (not automated): confirm real protocol rendering on at least one of iTerm2/Kitty/WezTerm, and confirm graceful degradation to the half-block tier on Windows Terminal without Sixel enabled, and on plain cmd.exe/PowerShell 5.1

## Blocked by

- 10-halfblock-image-rendering
