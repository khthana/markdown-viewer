# Handoff: mdview — the v1 issue set is complete

Date: 2026-08-27
Repo: `C:\Users\Terry\Desktop\Code\markdown-viewer`, branch `master`,
pushed to `origin` at https://github.com/khthana/markdown-viewer.

## Where the project stands

Every issue in `.scratch/mdview-v1/` has shipped: 01–15. The viewer now covers the pager,
GFM, syntax highlighting, the TOC sidebar, in-document search,
auto-reload with anchored scroll restoration, search state across
reloads, all three image tiers, the full CLI surface, and an `Esc` that
backs out of one context per press. Tagging a version builds and
attaches binaries for four platforms; crates.io publishing is wired but
waiting on a token.

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
| #13 distribution | `87cba2d` | `f881b52` (ADR-0009) |
| #14 anchor survives a rewrap | `8e9f6bd` | ADR-0010 |

Test suite: 218 passing, 1 ignored (a filesystem watcher smoke test that
is run manually), `cargo fmt --check` clean, `cargo clippy --all-targets
-- -D warnings` clean. CI (`ci.yml`, push and PR) is green on
windows-latest, macos-latest and ubuntu-latest as of `7666992`.

The very first CI run went red, on tests that had passed locally for the
whole project: two watcher tests spelled their paths as Windows literals,
and off Windows a backslash is an ordinary filename character. Worth
knowing that this repo's tests were Windows-only by accident until then —
`cargo test` on one platform is no longer the whole story.

Acceptance criteria, the module map, and testing decisions live in the
issue/PRD files and are not duplicated here:

- Issues: `.scratch/mdview-v1/issues/`
- PRD: `.scratch/mdview-v1/PRD.md`
- Design rationale: `docs/adr/`

## What is left

Nothing in the v1 issue set. What remains is release work (below) and
the manual checklist #11 still owes.

## Before the first real release

Two things are deliberately left for a human:

- **`CARGO_REGISTRY_TOKEN` is not set.** Create a token on crates.io and
  add it as a repository secret. Until then a real (non-prerelease) tag
  will attach its binaries and then fail the crates.io job on purpose —
  see ADR-0009 for why failing beats skipping quietly.
- **Nothing has been published under `mdview-term` yet.** The name is
  free but unclaimed; the first `cargo publish` claims it.

A release is `git tag -a vX.Y.Z` where `X.Y.Z` matches `Cargo.toml`
exactly — `dist` resolves the tag against the version and refuses
anything else.

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
- **A reload must not be worse than a resize.** Both re-wrap every row;
  a resize keeps the selected search match by its position in the match
  list, so ADR-0010 makes a reload fall back to the same rule rather than
  to a lost selection.
- **A key that cancels must never move the reader.** Every `Esc`
  precedence case asserts `scroll` is untouched, because the one thing an
  accidental press must not cost is the reader's place.

## Suggested next step

The v1 set is done. The next real step is a release: set
`CARGO_REGISTRY_TOKEN`, then tag `v0.1.0`. Issue #11's manual checklist
is the one piece of verification still owed.
