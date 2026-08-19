use crate::markdown::layout::LayoutDoc;

/// A single occurrence of a search query, as a half-open char range
/// within `layout_doc.rows[row]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub row: usize,
    pub start: usize,
    pub end: usize,
}

/// Finds every case-insensitive occurrence of `query` in `layout_doc`'s
/// rendered rows, in row then column order.
///
/// Compares char-by-char via `char::to_lowercase` rather than lowercasing
/// whole strings up front, since some Unicode casefolds change a string's
/// char count (e.g. German `ß`) and would desync char offsets from the
/// text they're supposed to index into.
pub fn search(query: &str, layout_doc: &LayoutDoc) -> Vec<Match> {
    if query.is_empty() {
        return Vec::new();
    }
    let query_chars: Vec<char> = query.chars().collect();

    let mut matches = Vec::new();
    for (row, text) in layout_doc.rows.iter().enumerate() {
        let row_chars: Vec<char> = text.chars().collect();
        if query_chars.len() > row_chars.len() {
            continue;
        }
        for start in 0..=(row_chars.len() - query_chars.len()) {
            let end = start + query_chars.len();
            let is_match = row_chars[start..end]
                .iter()
                .zip(&query_chars)
                .all(|(h, n)| h.to_lowercase().eq(n.to_lowercase()));
            if is_match {
                matches.push(Match { row, start, end });
            }
        }
    }
    matches
}

/// Identifies one search match by what the reader sees — the text of the
/// row it sits in, its column, and which of the identical (row, column)
/// pairs it is — rather than by its index in the match list, so it
/// survives matches appearing or disappearing elsewhere in the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchAnchor {
    row_text: String,
    start: usize,
    occurrence: usize,
}

impl MatchAnchor {
    /// Whether `candidate` sits at the same column of the same row text
    /// this anchor names. Row text rather than row number: an edit above
    /// shifts every row's number but not what the row says.
    fn matches(&self, candidate: &Match, layout_doc: &LayoutDoc) -> bool {
        candidate.start == self.start
            && layout_doc
                .rows
                .get(candidate.row)
                .is_some_and(|text| text == &self.row_text)
    }
}

/// What re-running the query after a reload did to the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reselection {
    /// The anchored match still exists, now at this index.
    Preserved(usize),
    /// Nothing was selected before the reload, so the first match is
    /// selected now — a plain re-run of the query, not a lost selection.
    SelectedFirst,
    /// The anchored match is gone but the query still matches; the first
    /// match is selected instead. Index 0 is that match: the list is in
    /// document order.
    FellBackToFirst,
    /// The query no longer matches anything.
    NoMatches,
}

/// Captures the currently selected match, so it can be found again after
/// the document is re-parsed. `None` when nothing is selected.
pub fn anchor_match(
    query: &str,
    layout_doc: &LayoutDoc,
    current: Option<usize>,
) -> Option<MatchAnchor> {
    let matches = search(query, layout_doc);
    let index = current?;
    let selected = matches.get(index)?;
    let row_text = layout_doc.rows.get(selected.row)?.clone();

    let anchor = MatchAnchor {
        row_text,
        start: selected.start,
        occurrence: 0,
    };
    let occurrence = matches[..index]
        .iter()
        .filter(|other| anchor.matches(other, layout_doc))
        .count();
    Some(MatchAnchor {
        occurrence,
        ..anchor
    })
}

/// Finds the anchored match in a freshly laid-out document. `anchor` is
/// `None` when nothing was selected before the reload, in which case the
/// query is simply re-run.
pub fn resolve_match(
    anchor: Option<&MatchAnchor>,
    query: &str,
    layout_doc: &LayoutDoc,
) -> Reselection {
    let matches = search(query, layout_doc);
    if matches.is_empty() {
        return Reselection::NoMatches;
    }
    let Some(anchor) = anchor else {
        return Reselection::SelectedFirst;
    };

    let candidates = matches
        .iter()
        .enumerate()
        .filter(|(_, candidate)| anchor.matches(candidate, layout_doc));
    match candidates.map(|(index, _)| index).nth(anchor.occurrence) {
        Some(index) => Reselection::Preserved(index),
        None => Reselection::FellBackToFirst,
    }
}

/// The match index to select after `n`, wrapping from the last match back
/// to the first. `current: None` (no selection yet) starts at the first.
pub fn next_match(current: Option<usize>, total: usize) -> Option<usize> {
    if total == 0 {
        return None;
    }
    Some(match current {
        Some(i) => (i + 1) % total,
        None => 0,
    })
}

/// The match index to select after `N`, wrapping from the first match
/// back to the last. `current: None` (no selection yet) starts at the last.
pub fn prev_match(current: Option<usize>, total: usize) -> Option<usize> {
    if total == 0 {
        return None;
    }
    Some(match current {
        Some(i) => (i + total - 1) % total,
        None => total - 1,
    })
}

#[cfg(test)]
mod tests {
    use crate::theme::Palette;

    use crate::markdown::blocks::lower;
    use crate::markdown::layout;

    fn layout_doc_for(source: &str) -> layout::LayoutDoc {
        let blocks = lower(source);
        layout::layout(
            &blocks,
            80,
            &crate::image::Sizing::text_only(),
            Palette::Dark,
        )
    }

    #[test]
    fn finds_a_single_match_in_a_single_row() {
        let doc = layout_doc_for("the quick brown fox");

        let matches = super::search("quick", &doc);

        assert_eq!(
            matches,
            vec![super::Match {
                row: 0,
                start: 4,
                end: 9
            }]
        );
    }

    #[test]
    fn matches_regardless_of_query_or_text_case() {
        let doc = layout_doc_for("The Quick Brown Fox");

        assert_eq!(
            super::search("quick", &doc),
            vec![super::Match {
                row: 0,
                start: 4,
                end: 9
            }]
        );
        assert_eq!(
            super::search("QUICK", &doc),
            vec![super::Match {
                row: 0,
                start: 4,
                end: 9
            }]
        );
    }

    #[test]
    fn finds_multiple_matches_across_rows_in_row_then_column_order() {
        // Narrow width forces two paragraphs onto separate rows; "fox"
        // appears twice in the first row and once in the second.
        let doc = layout_doc_for("fox fox\n\nlazy fox sleeps");

        let matches = super::search("fox", &doc);

        assert_eq!(
            matches,
            vec![
                super::Match {
                    row: 0,
                    start: 0,
                    end: 3
                },
                super::Match {
                    row: 0,
                    start: 4,
                    end: 7
                },
                super::Match {
                    row: 1,
                    start: 5,
                    end: 8
                },
            ]
        );
    }

    #[test]
    fn returns_no_matches_when_the_query_does_not_appear() {
        let doc = layout_doc_for("the quick brown fox");
        assert_eq!(super::search("elephant", &doc), vec![]);
    }

    #[test]
    fn returns_no_matches_for_an_empty_query() {
        let doc = layout_doc_for("the quick brown fox");
        assert_eq!(super::search("", &doc), vec![]);
    }

    #[test]
    fn next_match_advances_and_wraps_from_the_last_to_the_first() {
        assert_eq!(super::next_match(Some(0), 3), Some(1));
        assert_eq!(super::next_match(Some(2), 3), Some(0));
    }

    #[test]
    fn prev_match_retreats_and_wraps_from_the_first_to_the_last() {
        assert_eq!(super::prev_match(Some(1), 3), Some(0));
        assert_eq!(super::prev_match(Some(0), 3), Some(2));
    }

    #[test]
    fn next_and_prev_match_start_at_the_first_entry_when_nothing_is_selected() {
        assert_eq!(super::next_match(None, 3), Some(0));
        assert_eq!(super::prev_match(None, 3), Some(2));
    }

    #[test]
    fn next_and_prev_match_return_none_when_there_are_no_matches() {
        assert_eq!(super::next_match(None, 0), None);
        assert_eq!(super::prev_match(Some(0), 0), None);
    }

    #[test]
    fn keeps_the_same_match_selected_when_a_new_occurrence_appears_above_it() {
        // Two matches; the second one is selected.
        let old = layout_doc_for("alpha fox\n\nbravo fox");
        let anchor = super::anchor_match("fox", &old, Some(1)).expect("a match is selected");

        // A new "fox" above pushes the selected one from index 1 to 2.
        let new = layout_doc_for("fox first\n\nalpha fox\n\nbravo fox");

        assert_eq!(
            super::resolve_match(Some(&anchor), "fox", &new),
            super::Reselection::Preserved(2)
        );
    }

    #[test]
    fn resolves_to_the_matchs_new_row_after_an_edit_above_it() {
        let old = layout_doc_for("alpha fox");
        let anchor = super::anchor_match("fox", &old, Some(0)).expect("a match is selected");

        let new = layout_doc_for("intro\n\nalpha fox");

        let index = match super::resolve_match(Some(&anchor), "fox", &new) {
            super::Reselection::Preserved(index) => index,
            other => panic!("expected the match to be preserved, got {other:?}"),
        };
        assert_eq!(
            super::search("fox", &new)[index].row,
            1,
            "the same match now lives one row further down"
        );
    }

    #[test]
    fn falls_back_to_the_first_match_when_the_selected_one_was_deleted() {
        let old = layout_doc_for("alpha fox\n\nbravo fox");
        let anchor = super::anchor_match("fox", &old, Some(1)).expect("a match is selected");

        let new = layout_doc_for("alpha fox");

        assert_eq!(
            super::resolve_match(Some(&anchor), "fox", &new),
            super::Reselection::FellBackToFirst
        );
    }

    #[test]
    fn reports_no_matches_when_the_query_stops_matching() {
        let old = layout_doc_for("alpha fox");
        let anchor = super::anchor_match("fox", &old, Some(0)).expect("a match is selected");

        let new = layout_doc_for("nothing here now");

        assert_eq!(
            super::resolve_match(Some(&anchor), "fox", &new),
            super::Reselection::NoMatches
        );
    }

    #[test]
    fn distinguishes_matches_in_identical_rows_by_occurrence() {
        // Both rows read "fox", so only the occurrence count tells the
        // second match apart from the first.
        let old = layout_doc_for("fox\n\nfox");
        let anchor = super::anchor_match("fox", &old, Some(1)).expect("a match is selected");

        let new = layout_doc_for("cat\n\nfox\n\nfox");

        assert_eq!(
            super::resolve_match(Some(&anchor), "fox", &new),
            super::Reselection::Preserved(1)
        );
        assert_eq!(super::search("fox", &new)[1].row, 2);
    }

    #[test]
    fn has_nothing_to_anchor_when_no_match_is_selected() {
        let doc = layout_doc_for("alpha fox");

        assert_eq!(super::anchor_match("fox", &doc, None), None);
    }

    #[test]
    fn keeps_the_selection_through_an_edit_elsewhere_in_the_document() {
        let old = layout_doc_for("alpha fox\n\nbravo fox\n\ntail");
        let anchor = super::anchor_match("fox", &old, Some(0)).expect("a match is selected");

        // The edit is below both matches and touches neither.
        let new = layout_doc_for("alpha fox\n\nbravo fox\n\ntail, now longer");

        assert_eq!(
            super::resolve_match(Some(&anchor), "fox", &new),
            super::Reselection::Preserved(0)
        );
        assert_eq!(super::search("fox", &new)[0].row, 0, "and it hasn't moved");
    }

    #[test]
    fn selects_the_first_match_when_nothing_was_selected_before() {
        let new = layout_doc_for("a fox appears");

        assert_eq!(
            super::resolve_match(None, "fox", &new),
            super::Reselection::SelectedFirst
        );
    }

    #[test]
    fn reports_no_matches_for_an_unselected_query_that_still_matches_nothing() {
        let new = layout_doc_for("still nothing");

        assert_eq!(
            super::resolve_match(None, "fox", &new),
            super::Reselection::NoMatches
        );
    }
}
