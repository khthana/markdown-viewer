Status: ready-for-agent

# Image alt-text placeholder tier

## Parent

.scratch/mdview-v1/PRD.md

## What to build

`![alt](path)` image references are recognized during parsing/layout and rendered as an alt-text placeholder, reserving vertical space in the layout, without crashing on missing or malformed image paths. This is the baseline image tier that later slices upgrade with half-block and graphics-protocol rendering.

## Acceptance criteria

- [ ] `markdown/blocks.rs` recognizes Image blocks with alt text and path
- [ ] `layout.rs` reserves a fixed placeholder height (e.g. 1-2 rows) for each Image block
- [ ] `ui.rs` renders `🖼 [alt text]` (or equivalent) for every Image block in the viewport
- [ ] An image with no alt text renders a sensible fallback label (e.g. the filename) instead of an empty string
- [ ] A reference to a nonexistent image file does not crash or panic — it renders the same alt-text placeholder
- [ ] Unit test: blocks lowering produces an Image block with correct alt text and path for sample input including one image with alt text and one without
- [ ] Unit test: layout reserves the expected row count for an Image block

## Blocked by

- 02-core-markdown-rendering
