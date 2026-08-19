Status: ready-for-agent

# Search selection survives a reflow of its own paragraph

## Parent

.scratch/mdview-v1/PRD.md

## What to build

A search match is currently re-found after a reload by the plain text of
the *rendered row* it sits in, plus its column and which of the identical
(row text, column) pairs it is (`search::MatchAnchor`, ADR-0003). Rows are
wrap output, so any edit inside the same paragraph — fixing a typo, adding
a word earlier in it — rewrites the row text and drops the anchor: the
reader is bounced to match 1 with a "previous match gone" note even though
the matched text is still there, unmoved in the document.

That is the writer's-live-edit case user story 18 targets ("preserve my
active search query and match position across a reload where possible"),
so it should resolve rather than fall back. Give the anchor an identity
that survives its own block being re-wrapped, without losing the ability
to tell repeated occurrences apart.

Deciding what identifies the containing block is part of this issue. Block
*index* is not stable (an inserted block above shifts every index) and
block *text* is not stable (the typo fix changes it); the candidates worth
weighing are the match's ordinal within its block combined with either the
nearest preceding heading (level + text + occurrence, as
`reload::AnchorKind::Heading` already does) or a fuzzier block match.

## Acceptance criteria

- [ ] A match stays selected across a reload when its own paragraph is
      edited such that the text rewraps but the matched text survives
      (e.g. a word inserted earlier in the same paragraph)
- [ ] Repeated identical matches inside one block are still told apart —
      selecting the second "fox" in a paragraph and rewrapping it keeps
      the second one selected, not the first
- [ ] Genuinely deleted matched text still falls back to the first match
      with the status-line note, and a query that stops matching still
      shows "no matches" (issue #8's behavior is preserved)
- [ ] Anchors still survive edits *above* the match's block — every
      existing `search`/`reload` test from issue #8 keeps passing
      unchanged, or its change is justified in the ADR
- [ ] The chosen block-identity strategy is recorded in a new ADR, and
      ADR-0003's "Known limitation" section is updated to point at it
- [ ] Unit tests: rewrap of the match's own paragraph, a word inserted
      before the match within that paragraph, two identical matches in one
      paragraph, matched text deleted (fallback), and reflow caused by a
      terminal-width change rather than an edit

## Notes

Found by the spec-axis code review of issue #8 (2026-08-19), which flagged
that the AC's phrase "same text at a resolvable position" arguably covers
this case already. ADR-0003 documents the current behavior as a deliberate
scope cut rather than a bug.

## Blocked by

- 08-reload-preserves-search-state
