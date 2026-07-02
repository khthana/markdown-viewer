use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::markdown::blocks::{Block, Inline};
use crate::theme;

/// One block's assigned position in the document's virtual row space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaidOutBlock {
    pub block_index: usize,
    pub row_start: usize,
    pub row_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutDoc {
    pub blocks: Vec<LaidOutBlock>,
    pub total_rows: usize,
}

/// Computes virtual row ranges for each top-level block at the given
/// viewport width, including text wrapping.
///
/// Row counts come from actually rendering each block via
/// [`render_lines`] and measuring the result, rather than a parallel
/// counting algorithm — otherwise the two are guaranteed to drift (e.g. a
/// heading's rule line getting counted for rendering but not for
/// scrolling).
pub fn layout(blocks: &[Block], width: usize) -> LayoutDoc {
    let mut laid_out = Vec::with_capacity(blocks.len());
    let mut row_start = 0;

    for (block_index, block) in blocks.iter().enumerate() {
        let mut scratch = Vec::new();
        render_block(block, width, &mut scratch);
        let row_count = scratch.len();
        laid_out.push(LaidOutBlock {
            block_index,
            row_start,
            row_count,
        });
        row_start += row_count;
    }

    LayoutDoc {
        blocks: laid_out,
        total_rows: row_start,
    }
}

/// Renders the full document into styled, wrapped terminal lines at the
/// given viewport width. This is the single source of truth for both
/// on-screen rendering (`ui::render`) and row-count bookkeeping
/// (`layout`), so the two can never disagree about how many rows a
/// document occupies.
pub fn render_lines(blocks: &[Block], width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for block in blocks {
        render_block(block, width, &mut lines);
    }
    lines
}

fn render_block(block: &Block, width: usize, out: &mut Vec<Line<'static>>) {
    match block {
        Block::Heading { level, text } => {
            let hs = theme::heading_style(*level);
            let indent = hs.indent as usize;
            let content_width = width.saturating_sub(indent).max(1);
            let words = styled_words(text, hs.style);
            for spans in wrap_words(&words, content_width) {
                let mut spans = spans;
                if indent > 0 {
                    spans.insert(0, Span::raw(" ".repeat(indent)));
                }
                out.push(Line::from(spans));
            }
            if let Some(color) = hs.rule_color {
                out.push(Line::from(Span::styled(
                    "─".repeat(width.max(1)),
                    Style::new().fg(color),
                )));
            }
        }
        Block::Paragraph(inlines) => {
            let words = styled_words(inlines, Style::default());
            for spans in wrap_words(&words, width.max(1)) {
                out.push(Line::from(spans));
            }
        }
        Block::HorizontalRule => {
            out.push(Line::from(Span::raw("─".repeat(width.max(1)))));
        }
        Block::Code { text, .. } => {
            let style = Style::new().fg(Color::DarkGray);
            for line in text.lines() {
                out.push(Line::from(Span::styled(line.to_string(), style)));
            }
        }
        Block::Blockquote(inner) => {
            // Uses the full width (not narrowed for the "│ " prefix) so
            // the prefix doesn't shift wrap points relative to row counts.
            let quote_style = Style::new()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC);
            let mut inner_lines = Vec::new();
            for b in inner {
                render_block(b, width, &mut inner_lines);
            }
            for line in inner_lines {
                let mut spans = vec![Span::styled("\u{2502} ", quote_style)];
                spans.extend(line.spans);
                out.push(Line::from(spans));
            }
        }
        Block::List { ordered, items } => {
            for (i, item) in items.iter().enumerate() {
                let bullet = if *ordered {
                    format!("{}. ", i + 1)
                } else {
                    "\u{2022} ".to_string()
                };
                let bullet_width = bullet.chars().count();
                // Uses the full width (not narrowed for the bullet) so
                // the bullet doesn't shift wrap points relative to row
                // counts.
                let mut item_lines = Vec::new();
                for b in item {
                    render_block(b, width, &mut item_lines);
                }
                for (j, line) in item_lines.into_iter().enumerate() {
                    let prefix = if j == 0 {
                        bullet.clone()
                    } else {
                        " ".repeat(bullet_width)
                    };
                    let mut spans = vec![Span::raw(prefix)];
                    spans.extend(line.spans);
                    out.push(Line::from(spans));
                }
            }
        }
    }
}

/// Flattens inline spans (bold, italic, link text) into whitespace-split
/// words tagged with their resolved style.
fn styled_words(inlines: &[Inline], style: Style) -> Vec<(String, Style)> {
    let mut words = Vec::new();
    collect_words(inlines, style, &mut words);
    words
}

fn collect_words(inlines: &[Inline], style: Style, out: &mut Vec<(String, Style)>) {
    for inline in inlines {
        match inline {
            Inline::Text(text) => {
                for word in text.split_whitespace() {
                    out.push((word.to_string(), style));
                }
            }
            Inline::Bold(inner) => collect_words(inner, style.add_modifier(Modifier::BOLD), out),
            Inline::Italic(inner) => {
                collect_words(inner, style.add_modifier(Modifier::ITALIC), out)
            }
            Inline::Link { text, .. } => {
                collect_words(text, style.add_modifier(Modifier::UNDERLINED), out)
            }
        }
    }
}

/// Greedy word-wrap, grouping styled words into lines at `width` columns.
fn wrap_words(words: &[(String, Style)], width: usize) -> Vec<Vec<Span<'static>>> {
    let width = width.max(1);
    let mut lines: Vec<Vec<Span<'static>>> = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    let mut col = 0usize;

    for (word, style) in words {
        let word_len = word.chars().count();
        if col == 0 {
            current.push(Span::styled(word.clone(), *style));
            col = word_len;
        } else if col + 1 + word_len <= width {
            current.push(Span::raw(" "));
            current.push(Span::styled(word.clone(), *style));
            col += 1 + word_len;
        } else {
            lines.push(std::mem::take(&mut current));
            current.push(Span::styled(word.clone(), *style));
            col = word_len;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_paragraph(s: &str) -> Block {
        Block::Paragraph(vec![Inline::Text(s.to_string())])
    }

    #[test]
    fn single_paragraph_wraps_to_more_rows_at_a_narrower_width() {
        let blocks = vec![text_paragraph("the quick brown fox")];

        let wide = layout(&blocks, 80);
        assert_eq!(wide.blocks[0].row_count, 1);
        assert_eq!(wide.total_rows, 1);

        let narrow = layout(&blocks, 10);
        assert_eq!(narrow.blocks[0].row_count, 2);
        assert_eq!(narrow.total_rows, 2);
    }

    #[test]
    fn heading_without_a_rule_wraps_like_a_paragraph() {
        // H3 has no rule line (see theme::heading_style), so its row
        // count is pure text-wrap, same as a paragraph's.
        let blocks = vec![Block::Heading {
            level: 3,
            text: vec![Inline::Text("A rather long heading title here".to_string())],
        }];

        assert_eq!(layout(&blocks, 80).blocks[0].row_count, 1);
        assert_eq!(layout(&blocks, 10).blocks[0].row_count, 4);
    }

    #[test]
    fn heading_with_a_rule_counts_the_rule_as_an_extra_row() {
        // H1/H2 render a box-drawing rule underneath (theme::heading_style),
        // so their row count is text rows + 1, not just text rows.
        let blocks = vec![Block::Heading {
            level: 1,
            text: vec![Inline::Text("Title".to_string())],
        }];

        assert_eq!(layout(&blocks, 80).blocks[0].row_count, 2);
    }

    #[test]
    fn horizontal_rule_takes_exactly_one_row() {
        let blocks = vec![Block::HorizontalRule];
        assert_eq!(layout(&blocks, 80).blocks[0].row_count, 1);
        assert_eq!(layout(&blocks, 10).blocks[0].row_count, 1);
    }

    #[test]
    fn code_block_takes_one_row_per_line_unwrapped() {
        let blocks = vec![Block::Code {
            lang: None,
            text: "line one\nline two\nline three\n".to_string(),
        }];
        // Trailing newline doesn't create a phantom 4th row.
        assert_eq!(layout(&blocks, 80).blocks[0].row_count, 3);
        // Code doesn't reflow with viewport width.
        assert_eq!(layout(&blocks, 4).blocks[0].row_count, 3);
    }

    #[test]
    fn blockquote_sums_its_inner_blocks_rows() {
        let blocks = vec![Block::Blockquote(vec![
            text_paragraph("first"),
            text_paragraph("second"),
        ])];
        assert_eq!(layout(&blocks, 80).blocks[0].row_count, 2);
    }

    #[test]
    fn list_sums_each_items_inner_blocks_rows() {
        let blocks = vec![Block::List {
            ordered: false,
            items: vec![
                vec![text_paragraph("one")],
                vec![
                    text_paragraph("two"),
                    Block::List {
                        ordered: false,
                        items: vec![vec![text_paragraph("nested")]],
                    },
                ],
            ],
        }];
        // item "one": 1 row; item "two": 1 row + 1 nested-list row = 2 rows.
        assert_eq!(layout(&blocks, 80).blocks[0].row_count, 3);
    }

    #[test]
    fn multiple_paragraphs_get_sequential_row_ranges() {
        let blocks = vec![text_paragraph("first"), text_paragraph("second")];
        let doc = layout(&blocks, 80);

        assert_eq!(
            doc.blocks,
            vec![
                LaidOutBlock {
                    block_index: 0,
                    row_start: 0,
                    row_count: 1
                },
                LaidOutBlock {
                    block_index: 1,
                    row_start: 1,
                    row_count: 1
                },
            ]
        );
        assert_eq!(doc.total_rows, 2);
    }

    #[test]
    fn total_rows_always_matches_the_actual_rendered_line_count() {
        // Regression test for the layout/render divergence: total_rows
        // must equal render_lines(...).len() for every block type,
        // including headings with rules, or scrolling breaks.
        let blocks = vec![
            Block::Heading {
                level: 1,
                text: vec![Inline::Text("Title".to_string())],
            },
            Block::Heading {
                level: 2,
                text: vec![Inline::Text("Subtitle".to_string())],
            },
            text_paragraph("Body text."),
            Block::List {
                ordered: false,
                items: vec![vec![text_paragraph("item")]],
            },
        ];

        for width in [10, 20, 80] {
            assert_eq!(
                layout(&blocks, width).total_rows,
                render_lines(&blocks, width).len(),
                "mismatch at width {width}"
            );
        }
    }
}
