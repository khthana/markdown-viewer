# ADR-0010: A match anchor that survives its own paragraph being re-wrapped

Status: Accepted
Implements: `.scratch/mdview-v1/issues/14-search-anchor-survives-paragraph-reflow.md`

## Context

ADR-0003 identifies a search match by what the reader sees: the text of
the row it sits in, its column, and which of the identical (row text,
column) pairs it is. Row *numbers* shift with any edit above; row *text*
does not — which is why the anchor survives blocks being inserted or
removed elsewhere in the document.

Rows are wrap output, though. An edit *inside the match's own paragraph*
rewrites the row text without touching the matched word, and the anchor
finds nothing: the reader is bounced to match 1 with "previous match
gone" while the word they were looking at sits exactly where it was.
ADR-0003 recorded that as a known limitation. It is also precisely the
case user story 18 is about — the writer fixing a typo in the paragraph
they are reading.

The issue proposed identifying the containing block, by nearest preceding
heading or by a fuzzier match. Both were weighed and neither is stable in
the way the problem needs: a heading identity moves the failure to
"someone edited the heading", and a text identity for the block fails for
the same reason the row text fails — the block's text is what changed.

## Decisions

- **Resolution is two tiers, tried in order.** Tier one is ADR-0003's row
  text, unchanged, and still the first thing tried — it is an identity,
  and it is what carries the anchor through edits elsewhere in the
  document. Tier two applies only when tier one finds nothing, which is
  exactly the case where the row the anchor named no longer exists.
- **Tier two is the match's position in the document-wide match list.**
  Not the block, not the section: no scope at all. `MatchAnchor` records
  the index it had and how many matches there were.
- **The position is trusted only while the total match count is
  unchanged.** An equal population is the cheap signal that separates
  "the same matches, laid out differently" from "the matches themselves
  changed". A deleted occurrence lowers the count; one added ahead of the
  selection would silently shift what the position means. Neither is
  trusted, and the reader gets the "previous match gone" note, which is
  ADR-0003's behaviour for an unresolvable anchor.
- **This is the rule the app already lives by on a resize.** Matches are
  recomputed from the current layout every frame and `current_match` is a
  position into that list, so a terminal resize already re-wraps every
  row in the document and keeps the selection by position alone. Tier two
  makes a reload degrade to what a resize does, instead of to a lost
  selection. That symmetry is the argument for the design: it adds no new
  concept, it stops one path from being worse than the other.
- **No new dependencies.** `search.rs` still takes only `(query,
  layout_doc)` and stays terminal-agnostic; a block-scoped ordinal would
  have needed `Block` or `TocEntry` inside it.

## Consequences

- ADR-0003's "Known limitation" is closed, and its consequence that "a
  rewritten line loses its anchor" is now true only when the match
  population also changed. Both are updated to point here.
- **It is a population check, not an identity check.** A save that
  removes one occurrence and adds another keeps the count, and the
  position then resolves to an occurrence the reader never selected —
  silently, with no note. That is the price of having no block identity,
  and `swapping_one_match_for_another_keeps_the_position_not_the_text`
  pins it as a decision rather than leaving it to be rediscovered as a
  bug.
- The conservatism cuts the other way too: adding a new match anywhere
  while editing the match's own paragraph gives up the selection, even
  though the match itself survived. Falling back announces itself, so the
  reader is never quietly moved.
- Every `search` and `reload` test from issue #8 passes unchanged, which
  is the evidence that tier one's behaviour was not disturbed — tier two
  is only reachable where the old code had already given up.
