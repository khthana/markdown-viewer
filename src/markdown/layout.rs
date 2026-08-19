use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::image::Sizing;
use crate::markdown::blocks::{self, Block, ColumnAlignment, Inline, flatten_plain_text};
use crate::theme::Palette;

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
    /// Each row's plain text (styling stripped), index-aligned with the
    /// row-number space `blocks`/`total_rows` use. Exists for `search` to
    /// match against, without a second render pass.
    pub rows: Vec<String>,
}

/// Computes virtual row ranges for each top-level block at the given
/// viewport width, including text wrapping.
///
/// Row counts come from actually rendering each block via
/// [`render_lines`] and measuring the result, rather than a parallel
/// counting algorithm — otherwise the two are guaranteed to drift (e.g. a
/// heading's rule line getting counted for rendering but not for
/// scrolling).
pub fn layout(blocks: &[Block], width: usize, images: &Sizing, palette: Palette) -> LayoutDoc {
    let ctx = Ctx {
        width,
        images,
        palette,
    };
    let mut laid_out = Vec::with_capacity(blocks.len());
    let mut rows = Vec::new();
    let mut row_start = 0;

    for (block_index, block) in blocks.iter().enumerate() {
        let mut scratch = Vec::new();
        render_block(block, ctx, &mut scratch);
        let row_count = scratch.len();
        rows.extend(scratch.iter().map(line_plain_text));
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
        rows,
    }
}

/// Concatenates a line's spans down to their plain text, discarding style.
fn line_plain_text(line: &Line<'static>) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

/// Renders the full document into styled, wrapped terminal lines at the
/// given viewport width. This is the single source of truth for both
/// on-screen rendering (`ui::render`) and row-count bookkeeping
/// (`layout`), so the two can never disagree about how many rows a
/// document occupies.
pub fn render_lines(
    blocks: &[Block],
    width: usize,
    images: &Sizing,
    palette: Palette,
) -> Vec<Line<'static>> {
    let ctx = Ctx {
        width,
        images,
        palette,
    };
    let mut lines = Vec::new();
    for block in blocks {
        render_block(block, ctx, &mut lines);
    }
    lines
}

/// What rendering one block needs to know beyond the block itself. These
/// three always travel together — the width a block wraps at, how its
/// images are sized, and what it's painted in — so they're carried as one
/// value rather than threaded through every recursive call separately.
#[derive(Clone, Copy)]
struct Ctx<'a> {
    width: usize,
    images: &'a Sizing,
    palette: Palette,
}

fn render_block(block: &Block, ctx: Ctx<'_>, out: &mut Vec<Line<'static>>) {
    let Ctx {
        width,
        images,
        palette,
    } = ctx;
    match block {
        Block::Heading { level, text } => {
            let hs = palette.heading_style(*level);
            let indent = hs.indent as usize;
            let content_width = width.saturating_sub(indent).max(1);
            let words = styled_words(text, hs.style, palette);
            for spans in wrap_words(&words, content_width) {
                let mut spans = spans;
                if indent > 0 {
                    spans.insert(0, Span::raw(" ".repeat(indent)));
                }
                out.push(Line::from(spans));
            }
            if let Some(rule_style) = hs.rule_style {
                out.push(Line::from(Span::styled(
                    "─".repeat(width.max(1)),
                    rule_style,
                )));
            }
        }
        Block::Paragraph(inlines) => {
            let words = styled_words(inlines, Style::default(), palette);
            for spans in wrap_words(&words, width.max(1)) {
                out.push(Line::from(spans));
            }
        }
        Block::Image { alt, path } => {
            if images.draws(path) {
                // Blank rows held open for `ui` to paint the picture into.
                // The text has to leave them genuinely empty: whatever is
                // written here would show through the image's own cells.
                for _ in 0..images.rows_for_path(path, width) {
                    out.push(Line::default());
                }
            } else {
                // One row, at any width: a placeholder that wrapped would
                // make the document's height depend on the terminal's.
                out.push(Line::from(Span::styled(
                    blocks::image_placeholder(alt, path),
                    palette.image_placeholder_style(),
                )));
            }
        }
        Block::HorizontalRule => {
            out.push(Line::from(Span::raw("─".repeat(width.max(1)))));
        }
        Block::Code { lang, text } => {
            out.extend(crate::highlight::highlight_code(
                text,
                lang.as_deref(),
                palette,
            ));
        }
        Block::Blockquote(inner) => {
            // Uses the full width (not narrowed for the "│ " prefix) so
            // the prefix doesn't shift wrap points relative to row counts.
            let quote_style = palette.blockquote_style();
            let mut inner_lines = Vec::new();
            for b in inner {
                render_block(b, ctx, &mut inner_lines);
            }
            for line in inner_lines {
                let mut spans = vec![Span::styled("\u{2502} ", quote_style)];
                spans.extend(line.spans);
                out.push(Line::from(spans));
            }
        }
        Block::List { ordered, items } => {
            for (i, item) in items.iter().enumerate() {
                // A task list item wraps its content and carries checked
                // state; unwrap it here so its checkbox glyph replaces
                // the normal bullet instead of showing both.
                let (bullet, content): (String, &[Block]) = match item.first() {
                    Some(Block::TaskListItem { checked, content }) => {
                        (task_glyph(*checked), content.as_slice())
                    }
                    _ => {
                        let glyph = if *ordered {
                            format!("{}. ", i + 1)
                        } else {
                            "\u{2022} ".to_string()
                        };
                        (glyph, item.as_slice())
                    }
                };
                render_list_item(&bullet, content, ctx, out);
            }
        }
        Block::TaskListItem { checked, content } => {
            render_list_item(&task_glyph(*checked), content, ctx, out);
        }
        Block::FootnoteDefinition { label, content } => {
            render_list_item(&format!("[^{label}]: "), content, ctx, out);
        }
        Block::Table {
            alignments,
            header,
            rows,
        } => render_table(alignments, header, rows, width, out),
    }
}

/// Renders a table as aligned columns. v1 simplification: cells are
/// single-line plain text (no inline styling, no per-cell wrapping) — a
/// row that exceeds the viewport is truncated rather than reflowed,
/// mirroring the width tradeoff already made for blockquotes/lists.
fn render_table(
    alignments: &[ColumnAlignment],
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    width: usize,
    out: &mut Vec<Line<'static>>,
) {
    let column_count = header.len();
    let header_text: Vec<String> = header.iter().map(|c| flatten_plain_text(c)).collect();
    let row_texts: Vec<Vec<String>> = rows
        .iter()
        .map(|r| r.iter().map(|c| flatten_plain_text(c)).collect())
        .collect();

    let mut col_widths = vec![0usize; column_count];
    for (i, cell) in header_text.iter().enumerate() {
        col_widths[i] = col_widths[i].max(cell.chars().count());
    }
    for row in &row_texts {
        for (i, cell) in row.iter().enumerate().take(column_count) {
            col_widths[i] = col_widths[i].max(cell.chars().count());
        }
    }

    let header_style = Style::new().add_modifier(Modifier::BOLD);
    out.push(render_table_row(
        &header_text,
        &col_widths,
        alignments,
        header_style,
        width,
    ));
    out.push(Line::from(Span::raw(table_separator(&col_widths, width))));
    for row in &row_texts {
        out.push(render_table_row(
            row,
            &col_widths,
            alignments,
            Style::default(),
            width,
        ));
    }
}

fn pad_cell(text: &str, col_width: usize, alignment: ColumnAlignment) -> String {
    let pad = col_width.saturating_sub(text.chars().count());
    match alignment {
        ColumnAlignment::Right => format!("{}{text}", " ".repeat(pad)),
        ColumnAlignment::Center => {
            let left = pad / 2;
            let right = pad - left;
            format!("{}{text}{}", " ".repeat(left), " ".repeat(right))
        }
        ColumnAlignment::None | ColumnAlignment::Left => format!("{text}{}", " ".repeat(pad)),
    }
}

fn render_table_row(
    cells: &[String],
    col_widths: &[usize],
    alignments: &[ColumnAlignment],
    style: Style,
    width: usize,
) -> Line<'static> {
    let mut rendered = String::new();
    for (i, cell) in cells.iter().enumerate() {
        if i > 0 {
            rendered.push_str(" \u{2502} ");
        }
        let align = alignments.get(i).copied().unwrap_or(ColumnAlignment::None);
        let col_width = col_widths.get(i).copied().unwrap_or(cell.chars().count());
        rendered.push_str(&pad_cell(cell, col_width, align));
    }
    let truncated: String = rendered.chars().take(width.max(1)).collect();
    Line::from(Span::styled(truncated, style))
}

fn table_separator(col_widths: &[usize], width: usize) -> String {
    let mut sep = String::new();
    for (i, col_width) in col_widths.iter().enumerate() {
        if i > 0 {
            sep.push_str("\u{2500}\u{253C}\u{2500}");
        }
        sep.push_str(&"\u{2500}".repeat(*col_width));
    }
    sep.chars().take(width.max(1)).collect()
}

fn task_glyph(checked: bool) -> String {
    if checked {
        "\u{2611} ".to_string()
    } else {
        "\u{2610} ".to_string()
    }
}

/// Renders a list item's blocks with `bullet` prefixed on the first line
/// and blank padding aligning continuation lines under it.
fn render_list_item(bullet: &str, content: &[Block], ctx: Ctx<'_>, out: &mut Vec<Line<'static>>) {
    let bullet_width = bullet.chars().count();
    // Uses the full width (not narrowed for the bullet) so the bullet
    // doesn't shift wrap points relative to row counts.
    let mut item_lines = Vec::new();
    for b in content {
        render_block(b, ctx, &mut item_lines);
    }
    for (j, line) in item_lines.into_iter().enumerate() {
        let prefix = if j == 0 {
            bullet.to_string()
        } else {
            " ".repeat(bullet_width)
        };
        let mut spans = vec![Span::raw(prefix)];
        spans.extend(line.spans);
        out.push(Line::from(spans));
    }
}

/// Flattens inline spans (bold, italic, link text) into whitespace-split
/// words tagged with their resolved style.
fn styled_words(inlines: &[Inline], style: Style, palette: Palette) -> Vec<(String, Style)> {
    let mut words = Vec::new();
    collect_words(inlines, style, palette, &mut words);
    words
}

fn collect_words(
    inlines: &[Inline],
    style: Style,
    palette: Palette,
    out: &mut Vec<(String, Style)>,
) {
    for inline in inlines {
        match inline {
            Inline::Text(text) => {
                for word in text.split_whitespace() {
                    out.push((word.to_string(), style));
                }
            }
            Inline::Bold(inner) => {
                collect_words(inner, style.add_modifier(Modifier::BOLD), palette, out)
            }
            Inline::Italic(inner) => {
                collect_words(inner, style.add_modifier(Modifier::ITALIC), palette, out)
            }
            Inline::Strikethrough(inner) => collect_words(
                inner,
                style.add_modifier(Modifier::CROSSED_OUT),
                palette,
                out,
            ),
            Inline::Link { text, .. } => {
                collect_words(text, style.add_modifier(Modifier::UNDERLINED), palette, out)
            }
            Inline::FootnoteReference(label) => {
                let marker_style = style.patch(palette.footnote_marker_style());
                out.push((format!("[^{label}]"), marker_style));
            }
            // Pushed as one unbreakable word so the icon never wraps away
            // from the label it belongs to.
            Inline::Image { alt, path } => {
                out.push((
                    blocks::image_placeholder(alt, path),
                    palette.image_placeholder_style(),
                ));
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
    /// Layout for a terminal that can't draw images, which is what every
    /// test that isn't about images wants.
    fn text_layout(blocks: &[Block], width: usize) -> LayoutDoc {
        layout(blocks, width, &Sizing::text_only(), Palette::Dark)
    }

    fn text_paragraph(s: &str) -> Block {
        Block::Paragraph(vec![Inline::Text(s.to_string())])
    }

    #[test]
    fn single_paragraph_wraps_to_more_rows_at_a_narrower_width() {
        let blocks = vec![text_paragraph("the quick brown fox")];

        let wide = text_layout(&blocks, 80);
        assert_eq!(wide.blocks[0].row_count, 1);
        assert_eq!(wide.total_rows, 1);

        let narrow = text_layout(&blocks, 10);
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

        assert_eq!(text_layout(&blocks, 80).blocks[0].row_count, 1);
        assert_eq!(text_layout(&blocks, 10).blocks[0].row_count, 4);
    }

    #[test]
    fn heading_with_a_rule_counts_the_rule_as_an_extra_row() {
        // H1/H2 render a box-drawing rule underneath (theme::heading_style),
        // so their row count is text rows + 1, not just text rows.
        let blocks = vec![Block::Heading {
            level: 1,
            text: vec![Inline::Text("Title".to_string())],
        }];

        assert_eq!(text_layout(&blocks, 80).blocks[0].row_count, 2);
    }

    #[test]
    fn horizontal_rule_takes_exactly_one_row() {
        let blocks = vec![Block::HorizontalRule];
        assert_eq!(text_layout(&blocks, 80).blocks[0].row_count, 1);
        assert_eq!(text_layout(&blocks, 10).blocks[0].row_count, 1);
    }

    #[test]
    fn code_block_takes_one_row_per_line_unwrapped() {
        let blocks = vec![Block::Code {
            lang: None,
            text: "line one\nline two\nline three\n".to_string(),
        }];
        // Trailing newline doesn't create a phantom 4th row.
        assert_eq!(text_layout(&blocks, 80).blocks[0].row_count, 3);
        // Code doesn't reflow with viewport width.
        assert_eq!(text_layout(&blocks, 4).blocks[0].row_count, 3);
    }

    #[test]
    fn blockquote_sums_its_inner_blocks_rows() {
        let blocks = vec![Block::Blockquote(vec![
            text_paragraph("first"),
            text_paragraph("second"),
        ])];
        assert_eq!(text_layout(&blocks, 80).blocks[0].row_count, 2);
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
        assert_eq!(text_layout(&blocks, 80).blocks[0].row_count, 3);
    }

    #[test]
    fn layout_rows_expose_each_rendered_rows_plain_text_for_search() {
        let blocks = vec![text_paragraph("hello world")];
        let doc = text_layout(&blocks, 80);
        assert_eq!(doc.rows, vec!["hello world".to_string()]);
    }

    #[test]
    fn multiple_paragraphs_get_sequential_row_ranges() {
        let blocks = vec![text_paragraph("first"), text_paragraph("second")];
        let doc = text_layout(&blocks, 80);

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
            image("A picture", "square.png"),
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

        // Both tiers: a drawable image reserves several rows where the
        // placeholder takes one, and the two passes must still agree.
        let drawable = Sizing::with_pixels(
            ratatui_image::FontSize::new(10, 20),
            [("square.png", (200, 200))],
        );
        for images in [Sizing::text_only(), drawable] {
            for width in [10, 20, 80] {
                assert_eq!(
                    layout(&blocks, width, &images, Palette::Dark).total_rows,
                    render_lines(&blocks, width, &images, Palette::Dark).len(),
                    "mismatch at width {width}"
                );
            }
        }
    }

    #[test]
    fn every_palette_lays_out_the_same_rows() {
        // --no-color and --theme change how the document looks, never
        // where anything is: a palette that dropped a heading's rule row
        // would move every scroll offset, TOC target and search match
        // below it.
        let blocks = vec![
            Block::Heading {
                level: 1,
                text: vec![Inline::Text("Title".to_string())],
            },
            Block::Heading {
                level: 4,
                text: vec![Inline::Text("Deep".to_string())],
            },
            text_paragraph("Body text that is long enough to wrap somewhere."),
            Block::Code {
                lang: Some("rust".to_string()),
                text: "fn main() {}\n".to_string(),
            },
            Block::Blockquote(vec![text_paragraph("quoted")]),
            image("A picture", "square.png"),
        ];

        for width in [10, 20, 80] {
            let dark = layout(&blocks, width, &Sizing::text_only(), Palette::Dark);
            for palette in [Palette::Light, Palette::Plain] {
                let other = layout(&blocks, width, &Sizing::text_only(), palette);
                assert_eq!(
                    other.blocks, dark.blocks,
                    "{palette:?} moved a block at width {width}"
                );
                assert_eq!(
                    other.rows, dark.rows,
                    "{palette:?} changed the text at width {width}"
                );
            }
        }
    }

    fn image(alt: &str, path: &str) -> Block {
        Block::Image {
            alt: alt.to_string(),
            path: path.to_string(),
        }
    }

    #[test]
    fn an_image_reserves_one_row_whatever_the_width() {
        let blocks = vec![
            text_paragraph("before"),
            image("A diagram of the pipeline", "diagram.png"),
            text_paragraph("after"),
        ];

        for width in [80, 20] {
            let doc = text_layout(&blocks, width);
            assert_eq!(doc.blocks[1].row_count, 1, "at width {width}");
            assert_eq!(doc.blocks[2].row_start, 2, "at width {width}");
        }
    }

    #[test]
    fn an_images_reserved_row_holds_its_placeholder_text() {
        let doc = text_layout(&[image("A diagram", "diagram.png")], 80);

        assert_eq!(doc.rows, vec!["\u{1f5bc} [A diagram]".to_string()]);
    }

    #[test]
    fn an_image_without_alt_text_falls_back_to_its_file_name() {
        let doc = text_layout(&[image("", "photos/holiday.jpg")], 80);

        assert_eq!(doc.rows, vec!["\u{1f5bc} [holiday.jpg]".to_string()]);
    }

    #[test]
    fn an_image_with_no_alt_text_and_no_file_name_still_gets_a_label() {
        let doc = text_layout(&[image("", "")], 80);

        assert_eq!(doc.rows, vec!["\u{1f5bc} [image]".to_string()]);
    }

    #[test]
    fn a_drawable_image_reserves_blank_rows_for_the_picture_itself() {
        // 200x200 px at a 10x20 cell is 20 cols by 10 rows.
        let sizing = Sizing::with_pixels(
            ratatui_image::FontSize::new(10, 20),
            [("square.png", (200, 200))],
        );

        let doc = layout(&[image("Alt", "square.png")], 80, &sizing, Palette::Dark);

        assert_eq!(doc.blocks[0].row_count, 10);
        assert_eq!(
            doc.rows,
            vec![String::new(); 10],
            "the rows are left empty for the picture to be painted into"
        );
    }

    #[test]
    fn an_image_the_terminal_cannot_draw_keeps_its_placeholder_row() {
        let sizing = Sizing::with_pixels(
            ratatui_image::FontSize::new(10, 20),
            [("elsewhere.png", (200, 200))],
        );

        let doc = layout(&[image("Alt", "square.png")], 80, &sizing, Palette::Dark);

        assert_eq!(doc.blocks[0].row_count, 1);
        assert_eq!(doc.rows, vec!["\u{1f5bc} [Alt]".to_string()]);
    }
}
