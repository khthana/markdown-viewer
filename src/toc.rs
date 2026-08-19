use crate::markdown::blocks::HeadingRef;
use crate::markdown::layout::LayoutDoc;

/// A heading's entry in the TOC sidebar, resolved to a concrete row in
/// the currently laid-out document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    pub level: u8,
    pub text: String,
    pub row: usize,
}

/// Resolves collected headings to their rows in `layout_doc`. Layout is
/// width-dependent, so this must be re-run whenever the viewport width
/// (and therefore `layout_doc`) changes.
///
/// Headings whose `block_index` isn't in `layout_doc` (shouldn't happen
/// in practice, since both come from the same document) are skipped
/// rather than panicking.
pub fn resolve(headings: &[HeadingRef], layout_doc: &LayoutDoc) -> Vec<TocEntry> {
    headings
        .iter()
        .filter_map(|heading| {
            layout_doc
                .blocks
                .get(heading.block_index)
                .map(|laid_out| TocEntry {
                    level: heading.level,
                    text: heading.text.clone(),
                    row: laid_out.row_start,
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::blocks::lower_with_headings;
    use crate::markdown::layout;

    #[test]
    fn resolves_headings_to_their_laid_out_rows() {
        let source = "# Title\n\nIntro paragraph text.\n\n## Section";
        let (blocks, headings) = lower_with_headings(source);
        let layout_doc = layout::layout(&blocks, 80, &crate::image::Sizing::text_only());

        let entries = resolve(&headings, &layout_doc);

        assert_eq!(
            entries,
            vec![
                TocEntry {
                    level: 1,
                    text: "Title".to_string(),
                    row: 0,
                },
                TocEntry {
                    level: 2,
                    text: "Section".to_string(),
                    // H1 (1 text row + 1 rule) + paragraph (1 row) = 3.
                    row: 3,
                },
            ]
        );
    }
}
