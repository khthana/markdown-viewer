Status: ready-for-agent

# Half-block image rendering tier with capability detection

## Parent

.scratch/mdview-v1/PRD.md

## What to build

Upgrade the placeholder tier with real (if approximate) image rendering via colored half-block Unicode glyphs on terminals that support 16-color output but no graphics protocol — the tier most Windows users will actually see, since default Windows consoles support neither Sixel nor Kitty.

## Acceptance criteria

- [ ] `image.rs`: integrates `ratatui-image` (pinned to a version compatible with `ratatui = "0.30.1"`, e.g. `11.0.6`), calling `Picker::detect()` once at startup
- [ ] When detection yields half-block capability, Image blocks decode the referenced file and render via `ratatui-image`'s half-block widget instead of the alt-text placeholder
- [ ] When the image file fails to decode (corrupt/unsupported format), falls back to the alt-text placeholder rather than crashing
- [ ] `layout.rs`'s reserved row height for Image blocks is sized from the picker's font-cell metrics (with a sensible cap, e.g. 15 rows) rather than the fixed placeholder height from the alt-text tier
- [ ] Manual verification: open a markdown file with a real image on a default Windows Terminal/PowerShell session and confirm a recognizable half-block rendering appears (not just a placeholder)

## Blocked by

- 09-image-alt-text-placeholder
