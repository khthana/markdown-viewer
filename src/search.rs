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
    use crate::markdown::blocks::lower;
    use crate::markdown::layout;

    fn layout_doc_for(source: &str) -> layout::LayoutDoc {
        let blocks = lower(source);
        layout::layout(&blocks, 80)
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
}
