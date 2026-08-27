# Handoff: mdview — issues #7–#12 and #15 complete

Date: 2026-08-27
Repo: `C:\Users\Terry\Desktop\Code\markdown-viewer`, branch `master`.

## Where the project stands

Issues 01–12 have shipped, plus #15. The viewer now covers the pager,
GFM, syntax highlighting, the TOC sidebar, in-document search,
auto-reload with anchored scroll restoration, search state across
reloads, all three image tiers, the full CLI surface, and an `Esc` that
backs out of one context per press.

Each issue landed as two commits — the implementation, then its ADR:

| Issue | Implementation | ADR commit |
| --- | --- | --- |
| #7 auto-reload + anchoring | `410ebb8` | `8b497dc` (ADR-0002) |
| #8 search across reload | `373201a` | `f0509d9` (ADR-0003) |
| #9 image alt-text tier | `6fb7f46` | `acbb868` (ADR-0004) |
| #10 half-block tier | `e2b9031` | `d0d0040` (ADR-0005) |
| #11 graphics protocol tier | `afb1764` | `8444fb9` (ADR-0006) |
| #12 CLI flags, help, errors | `75da31b` | `848875d` (ADR-0007) |
| #15 `Esc` cancels a context | `e16dc61` | `1ec4a9b` (ADR-0008) |

Test suite: 209 passing, 1 ignored (a filesystem watcher smoke test that
is run manually), `cargo fmt --check` clean, `cargo clippy --all-targets
-- -D warnings` clean.

Acceptance criteria, the module map, and testing decisions live in the
issue/PRD files and are not duplicated here:

- Issues: `.scratch/mdview-v1/issues/`
- PRD: `.scratch/mdview-v1/PRD.md`
- Design rationale: `docs/adr/`

## What is left

- **#13 distribution** (`cargo-dist` release workflow, crates.io, README).
  Blocked in practice: this repo has no git remote, and the issue's last
  acceptance criterion requires a CI run against a GitHub Release.
- **#14 search anchor survives paragraph reflow.** Filed during #8's
  review; blocked by nothing. The only open issue that isn't waiting on
  a git remote.

## Owed verification

Issue #11's manual checklist is half done. Real graphics-protocol
rendering was confirmed in WezTerm; **graceful degradation on Windows
Terminal and on plain conhost has not been checked**. Launch it with the
pausing wrapper written during that session:

```powershell
# The wrapper takes an optional MDVIEW_PROTOCOL value as its first argument
& "C:\Program Files\WezTerm\wezterm.exe" start -- <scratchpad>\run-mdview.cmd
wt.exe <scratchpad>\run-mdview.cmd sixel
conhost.exe <scratchpad>\run-mdview.cmd halfblocks
```

The wrapper and its fixture (`images.md` + `checker.png`) live in the
session scratchpad, which is temporary — recreate them if the directory is
gone. What matters: the picture is recognisable, no escape sequences leak
as visible junk, text below the image doesn't shift, and a missing image
keeps its `🖼 [alt]` placeholder.

## Repo conventions worth knowing before continuing

- Issues never get their acceptance-criteria checkboxes checked off or
  their `Status:` line changed after shipping (confirmed with the user —
  not a bug, don't "clean up" old issue files).
- `.scratch/mdview-v1/PRD.md`'s "Module map" and "Implementation
  Decisions" sections are unusually prescriptive — treat them as a strong
  prior on function signatures and module boundaries.
- Work goes: confirm the test seams with the user first, then red-green
  vertical slices, then the full suite plus clippy and fmt, then
  `/code-review` on both axes, then the fixes, then the ADR, then the two
  commits.
- ADRs are not optional and not decoration. #12's review flagged ADR-0004
  as stale because a function it named had been renamed; `docs/agents/domain.md`
  requires surfacing a conflict like that rather than quietly overriding it.

## Invariants that tests actively defend

Worth knowing before changing rendering, because each of these was a real
bug caught by review rather than a hypothetical:

- **`layout::layout` and `ui::render` must agree on every row.** #10
  shipped a version where `ui` laid out with a text-only image sizing
  while `layout` used the real one, shifting every row below a picture.
- **Every palette lays out identically.** Only colour may vary between
  `Dark`/`Light`/`Plain`; a dropped heading rule row would move every
  scroll offset, TOC target and search match below it.
- **`app::KEYBINDINGS` and `handle_key` are checked against each other in
  both directions.** The one-directional version of that test missed two
  keys the app honoured but the overlay never mentioned.
- **Anything that isn't drawable right now shows a placeholder**, images
  pending an *encode* included — that gap made every picture flash a hole
  for one frame after decoding and after every resize.
- **A key that cancels must never move the reader.** Every `Esc`
  precedence case asserts `scroll` is untouched, because the one thing an
  accidental press must not cost is the reader's place.

## Suggested next step

No decision has been made on what comes next. #14 is the only unblocked
issue left; #13 needs a git remote to exist first.
