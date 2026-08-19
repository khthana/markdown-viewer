use pulldown_cmark::{Alignment, CodeBlockKind, Event as CmEvent, Tag, TagEnd};

use crate::markdown::parser;

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Bold(Vec<Inline>),
    Italic(Vec<Inline>),
    Strikethrough(Vec<Inline>),
    Link {
        text: Vec<Inline>,
        url: String,
    },
    FootnoteReference(String),
    /// An image that can't be given rows of its own — one inside a
    /// heading, a link, a table cell, or an emphasis span, where hoisting
    /// it out to a block would destroy the structure around it. It
    /// renders as the same placeholder label, inline with the text.
    Image {
        alt: String,
        path: String,
    },
}

/// Whether the inline frame being filled belongs to something an image
/// may interrupt — a paragraph, or a tight list item's own text — or to
/// an inline span (heading text, link label, table cell, emphasis) where
/// emitting a block would tear the surrounding structure apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Frame {
    Interruptible,
    InlineOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnAlignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Paragraph(Vec<Inline>),
    Heading {
        level: u8,
        text: Vec<Inline>,
    },
    HorizontalRule,
    Code {
        lang: Option<String>,
        text: String,
    },
    Blockquote(Vec<Block>),
    List {
        ordered: bool,
        items: Vec<Vec<Block>>,
    },
    TaskListItem {
        checked: bool,
        content: Vec<Block>,
    },
    FootnoteDefinition {
        label: String,
        content: Vec<Block>,
    },
    Table {
        alignments: Vec<ColumnAlignment>,
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    /// `![alt](path)`. Markdown treats an image as inline, but this app
    /// renders it as a block: later tiers paint real graphics into the
    /// block's own row range, which can't be done to part of a line of
    /// text. `alt` is exactly what the document said (possibly empty) —
    /// the placeholder label is derived at render time.
    Image {
        alt: String,
        path: String,
    },
}

/// A heading collected during the lowering pass, for TOC construction.
/// `block_index` refers to the top-level `Vec<Block>` `lower_with_headings`
/// returns, so it lines up with `layout::LaidOutBlock::block_index`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingRef {
    pub level: u8,
    pub text: String,
    pub block_index: usize,
}

/// Lowers CommonMark `source` into an owned `Vec<Block>`, discarding the
/// heading list. Production code always needs both (for the TOC), so
/// this convenience wrapper only exists for tests that don't.
#[cfg(test)]
pub(crate) fn lower(source: &str) -> Vec<Block> {
    lower_with_headings(source).0
}

/// Same as [`lower`], but also collects every top-level heading in
/// document order — in the same pass, so building a TOC doesn't require
/// a second walk of the document.
///
/// Headings nested inside a blockquote or list item are skipped: their
/// `block_index` wouldn't refer to a top-level block, and headings that
/// deep are rare enough that a v1 TOC can reasonably omit them.
///
/// Inline content (text, bold, italic, ...) can nest, so open spans are
/// tracked as a stack of buffers: starting a span pushes a fresh buffer,
/// ending one pops it and appends the finished span to its parent. Block
/// containers (blockquotes, list items) nest the same way one level up,
/// via a stack of block buffers.
pub fn lower_with_headings(source: &str) -> (Vec<Block>, Vec<HeadingRef>) {
    let mut headings = Vec::new();
    let mut blocks = Vec::new();
    let mut containers: Vec<Vec<Block>> = Vec::new();
    let mut stack: Vec<(Frame, Vec<Inline>)> = Vec::new();
    let mut link_urls: Vec<String> = Vec::new();
    let mut code_block: Option<(Option<String>, String)> = None;
    let mut list_stack: Vec<(bool, Vec<Vec<Block>>)> = Vec::new();
    // `TaskListMarker` can land directly under `Item` (tight lists) or
    // inside the item's `Paragraph` (loose lists) — either way, it always
    // fires while that item's frame is on top, so a stack scoped per-item
    // catches both shapes without caring which one it is.
    let mut task_marker_stack: Vec<Option<bool>> = Vec::new();
    let mut footnote_labels: Vec<String> = Vec::new();
    let mut table_alignments: Vec<ColumnAlignment> = Vec::new();
    let mut table_header: Vec<Vec<Inline>> = Vec::new();
    let mut table_rows: Vec<Vec<Vec<Inline>>> = Vec::new();
    let mut table_current_row: Vec<Vec<Inline>> = Vec::new();
    let mut image_urls: Vec<String> = Vec::new();

    for event in parser::parse(source) {
        match event {
            CmEvent::Start(Tag::Paragraph) => push_frame(&mut stack, Frame::Interruptible),
            CmEvent::Start(
                Tag::Heading { .. } | Tag::Strong | Tag::Emphasis | Tag::Strikethrough,
            ) => push_frame(&mut stack, Frame::InlineOnly),
            CmEvent::Start(Tag::Image { dest_url, .. }) => {
                image_urls.push(dest_url.into_string());
                push_frame(&mut stack, Frame::InlineOnly);
            }
            CmEvent::End(TagEnd::Image) => {
                let alt = flatten_plain_text(&pop_inlines(&mut stack));
                let path = image_urls.pop().unwrap_or_default();
                if let Some((Frame::Interruptible, _)) = stack.last() {
                    // Give the image rows of its own: text before it is
                    // closed off as a paragraph, and the rest continues in
                    // a fresh one after it.
                    flush_pending_item_text(&mut stack, &mut blocks, &mut containers);
                    push_block(&mut blocks, &mut containers, Block::Image { alt, path });
                } else {
                    push_inline(&mut stack, Inline::Image { alt, path });
                }
            }
            CmEvent::Start(Tag::Link { dest_url, .. }) => {
                link_urls.push(dest_url.into_string());
                push_frame(&mut stack, Frame::InlineOnly);
            }
            CmEvent::Start(Tag::CodeBlock(kind)) => {
                flush_pending_item_text(&mut stack, &mut blocks, &mut containers);
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => Some(lang.into_string()),
                    _ => None,
                };
                code_block = Some((lang, String::new()));
            }
            CmEvent::Start(Tag::BlockQuote(_)) => {
                flush_pending_item_text(&mut stack, &mut blocks, &mut containers);
                containers.push(Vec::new());
            }
            CmEvent::Start(Tag::FootnoteDefinition(label)) => {
                flush_pending_item_text(&mut stack, &mut blocks, &mut containers);
                footnote_labels.push(label.into_string());
                containers.push(Vec::new());
            }
            CmEvent::FootnoteReference(label) => {
                push_inline(&mut stack, Inline::FootnoteReference(label.into_string()));
            }
            CmEvent::Start(Tag::Table(alignments)) => {
                flush_pending_item_text(&mut stack, &mut blocks, &mut containers);
                table_alignments = alignments.iter().map(|a| column_alignment(*a)).collect();
            }
            CmEvent::Start(Tag::TableCell) => push_frame(&mut stack, Frame::InlineOnly),
            CmEvent::End(TagEnd::TableCell) => {
                let cell = pop_inlines(&mut stack);
                table_current_row.push(cell);
            }
            CmEvent::End(TagEnd::TableHead) => {
                table_header = std::mem::take(&mut table_current_row);
            }
            CmEvent::End(TagEnd::TableRow) => {
                table_rows.push(std::mem::take(&mut table_current_row));
            }
            CmEvent::End(TagEnd::Table) => {
                push_block(
                    &mut blocks,
                    &mut containers,
                    Block::Table {
                        alignments: std::mem::take(&mut table_alignments),
                        header: std::mem::take(&mut table_header),
                        rows: std::mem::take(&mut table_rows),
                    },
                );
            }
            CmEvent::Start(Tag::List(first_item_number)) => {
                flush_pending_item_text(&mut stack, &mut blocks, &mut containers);
                list_stack.push((first_item_number.is_some(), Vec::new()))
            }
            // An item's content may arrive either as direct text (tight
            // lists skip the Paragraph wrapper) or as nested blocks
            // (loose lists, nested lists): track both a block container
            // and an inline frame so either shape lands in the right place.
            CmEvent::Start(Tag::Item) => {
                containers.push(Vec::new());
                push_frame(&mut stack, Frame::Interruptible);
                task_marker_stack.push(None);
            }
            CmEvent::TaskListMarker(checked) => {
                if let Some(top) = task_marker_stack.last_mut() {
                    *top = Some(checked);
                }
            }
            CmEvent::End(TagEnd::Item) => {
                let direct_text = pop_inlines(&mut stack);
                if !direct_text.is_empty() {
                    push_block(&mut blocks, &mut containers, Block::Paragraph(direct_text));
                }
                let mut item = containers.pop().unwrap_or_default();
                if let Some(Some(checked)) = task_marker_stack.pop() {
                    item = vec![Block::TaskListItem {
                        checked,
                        content: item,
                    }];
                }
                if let Some((_, items)) = list_stack.last_mut() {
                    items.push(item);
                }
            }
            CmEvent::End(TagEnd::List(_)) => {
                if let Some((ordered, items)) = list_stack.pop() {
                    push_block(&mut blocks, &mut containers, Block::List { ordered, items });
                }
            }
            CmEvent::End(TagEnd::CodeBlock) => {
                if let Some((lang, text)) = code_block.take() {
                    push_block(&mut blocks, &mut containers, Block::Code { lang, text });
                }
            }
            CmEvent::End(TagEnd::Paragraph) => {
                let text = pop_inlines(&mut stack);
                // A paragraph holding nothing but an image has already had
                // its content emitted as an Image block; don't follow it
                // with an empty paragraph.
                if !text.is_empty() {
                    push_block(&mut blocks, &mut containers, Block::Paragraph(text));
                }
            }
            CmEvent::End(TagEnd::Heading(level)) => {
                let text = pop_inlines(&mut stack);
                if containers.is_empty() {
                    headings.push(HeadingRef {
                        level: level as u8,
                        text: flatten_plain_text(&text),
                        block_index: blocks.len(),
                    });
                }
                push_block(
                    &mut blocks,
                    &mut containers,
                    Block::Heading {
                        level: level as u8,
                        text,
                    },
                );
            }
            CmEvent::End(TagEnd::Strong) => {
                let inner = pop_inlines(&mut stack);
                push_inline(&mut stack, Inline::Bold(inner));
            }
            CmEvent::End(TagEnd::Emphasis) => {
                let inner = pop_inlines(&mut stack);
                push_inline(&mut stack, Inline::Italic(inner));
            }
            CmEvent::End(TagEnd::Strikethrough) => {
                let inner = pop_inlines(&mut stack);
                push_inline(&mut stack, Inline::Strikethrough(inner));
            }
            CmEvent::End(TagEnd::Link) => {
                let text = pop_inlines(&mut stack);
                if let Some(url) = link_urls.pop() {
                    push_inline(&mut stack, Inline::Link { text, url });
                }
            }
            CmEvent::End(TagEnd::BlockQuote(_)) => {
                let inner = containers.pop().unwrap_or_default();
                push_block(&mut blocks, &mut containers, Block::Blockquote(inner));
            }
            CmEvent::End(TagEnd::FootnoteDefinition) => {
                let content = containers.pop().unwrap_or_default();
                if let Some(label) = footnote_labels.pop() {
                    push_block(
                        &mut blocks,
                        &mut containers,
                        Block::FootnoteDefinition { label, content },
                    );
                }
            }
            CmEvent::Text(text) => {
                if let Some((_, code)) = code_block.as_mut() {
                    code.push_str(&text);
                } else {
                    push_inline(&mut stack, Inline::Text(text.into_string()));
                }
            }
            CmEvent::Rule => push_block(&mut blocks, &mut containers, Block::HorizontalRule),
            _ => {}
        }
    }

    (blocks, headings)
}

/// What to call an image in place of the image itself: its alt text, or
/// — when the document gave none — the file's name, which is usually
/// descriptive enough to be worth showing. Falls back to the raw path,
/// and finally to a generic word, so the reader never faces an empty
/// label.
pub fn image_label(alt: &str, path: &str) -> String {
    if !alt.trim().is_empty() {
        return alt.to_string();
    }
    let file_name = std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    if file_name.trim().is_empty() {
        "image".to_string()
    } else {
        file_name.to_string()
    }
}

/// Flattens inline spans down to their plain text content, e.g. for TOC
/// entry labels where inline styling doesn't apply.
pub fn flatten_plain_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(text) => out.push_str(text),
            Inline::Bold(inner)
            | Inline::Italic(inner)
            | Inline::Strikethrough(inner)
            | Inline::Link { text: inner, .. } => out.push_str(&flatten_plain_text(inner)),
            Inline::FootnoteReference(label) => out.push_str(&format!("[^{label}]")),
            Inline::Image { alt, path } => out.push_str(&format!("[{}]", image_label(alt, path))),
        }
    }
    out
}

fn column_alignment(alignment: Alignment) -> ColumnAlignment {
    match alignment {
        Alignment::None => ColumnAlignment::None,
        Alignment::Left => ColumnAlignment::Left,
        Alignment::Center => ColumnAlignment::Center,
        Alignment::Right => ColumnAlignment::Right,
    }
}

fn push_inline(stack: &mut [(Frame, Vec<Inline>)], inline: Inline) {
    if let Some((_, top)) = stack.last_mut() {
        top.push(inline);
    }
}

/// Opens a fresh inline frame of the given kind.
fn push_frame(stack: &mut Vec<(Frame, Vec<Inline>)>, frame: Frame) {
    stack.push((frame, Vec::new()));
}

/// Closes the innermost inline frame, returning what it collected.
fn pop_inlines(stack: &mut Vec<(Frame, Vec<Inline>)>) -> Vec<Inline> {
    stack.pop().map(|(_, inlines)| inlines).unwrap_or_default()
}

/// Pushes a finished block into the innermost open container (blockquote,
/// list item), or onto the top-level document if none is open.
fn push_block(blocks: &mut Vec<Block>, containers: &mut [Vec<Block>], block: Block) {
    match containers.last_mut() {
        Some(top) => top.push(block),
        None => blocks.push(block),
    }
}

/// A tight list item's leading text has no Paragraph start/end around it,
/// so it accumulates directly in the innermost inline frame. If a sibling
/// block (nested list, blockquote, code block) starts while that frame
/// still holds content, flush it as a Paragraph first so ordering is
/// preserved. The frame itself is cleared, not popped, so `Item`'s own
/// push/pop stays balanced.
fn flush_pending_item_text(
    stack: &mut [(Frame, Vec<Inline>)],
    blocks: &mut Vec<Block>,
    containers: &mut [Vec<Block>],
) {
    if let Some((_, top)) = stack.last_mut()
        && !top.is_empty()
    {
        let text = std::mem::take(top);
        push_block(blocks, containers, Block::Paragraph(text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_paragraph_lowers_to_a_single_text_run() {
        let blocks = lower("Hello world");
        assert_eq!(
            blocks,
            vec![Block::Paragraph(vec![Inline::Text(
                "Hello world".to_string()
            )])]
        );
    }

    #[test]
    fn headings_lower_with_their_level_and_text() {
        let source = "# One\n\n## Two\n\n###### Six";
        let blocks = lower(source);
        assert_eq!(
            blocks,
            vec![
                Block::Heading {
                    level: 1,
                    text: vec![Inline::Text("One".to_string())]
                },
                Block::Heading {
                    level: 2,
                    text: vec![Inline::Text("Two".to_string())]
                },
                Block::Heading {
                    level: 6,
                    text: vec![Inline::Text("Six".to_string())]
                },
            ]
        );
    }

    #[test]
    fn bold_and_italic_runs_lower_as_nested_inlines() {
        let blocks = lower("This is **bold** and *italic* text.");
        assert_eq!(
            blocks,
            vec![Block::Paragraph(vec![
                Inline::Text("This is ".to_string()),
                Inline::Bold(vec![Inline::Text("bold".to_string())]),
                Inline::Text(" and ".to_string()),
                Inline::Italic(vec![Inline::Text("italic".to_string())]),
                Inline::Text(" text.".to_string()),
            ])]
        );
    }

    #[test]
    fn strikethrough_run_lowers_as_a_nested_inline() {
        let blocks = lower("This is ~~struck~~ text.");
        assert_eq!(
            blocks,
            vec![Block::Paragraph(vec![
                Inline::Text("This is ".to_string()),
                Inline::Strikethrough(vec![Inline::Text("struck".to_string())]),
                Inline::Text(" text.".to_string()),
            ])]
        );
    }

    #[test]
    fn unordered_and_ordered_lists_lower_with_item_paragraphs() {
        let blocks = lower("- one\n- two");
        assert_eq!(
            blocks,
            vec![Block::List {
                ordered: false,
                items: vec![
                    vec![Block::Paragraph(vec![Inline::Text("one".to_string())])],
                    vec![Block::Paragraph(vec![Inline::Text("two".to_string())])],
                ],
            }]
        );

        let blocks = lower("1. one\n2. two");
        assert_eq!(
            blocks,
            vec![Block::List {
                ordered: true,
                items: vec![
                    vec![Block::Paragraph(vec![Inline::Text("one".to_string())])],
                    vec![Block::Paragraph(vec![Inline::Text("two".to_string())])],
                ],
            }]
        );
    }

    #[test]
    fn task_list_items_lower_with_their_checked_state() {
        let tight = lower("- [x] done\n- [ ] not done");
        assert_eq!(
            tight,
            vec![Block::List {
                ordered: false,
                items: vec![
                    vec![Block::TaskListItem {
                        checked: true,
                        content: vec![Block::Paragraph(vec![Inline::Text("done".to_string())])],
                    }],
                    vec![Block::TaskListItem {
                        checked: false,
                        content: vec![Block::Paragraph(vec![Inline::Text("not done".to_string())])],
                    }],
                ],
            }]
        );

        // Loose lists (blank line between items) wrap content in an
        // explicit Paragraph before the marker; the shape must match.
        let loose = lower("- [x] done\n\n- [ ] not done");
        assert_eq!(loose, tight);
    }

    #[test]
    fn nested_list_lowers_as_a_list_block_inside_its_parent_item() {
        let blocks = lower("- outer\n  - inner");
        assert_eq!(
            blocks,
            vec![Block::List {
                ordered: false,
                items: vec![vec![
                    Block::Paragraph(vec![Inline::Text("outer".to_string())]),
                    Block::List {
                        ordered: false,
                        items: vec![vec![Block::Paragraph(vec![Inline::Text(
                            "inner".to_string()
                        )])]],
                    },
                ]],
            }]
        );
    }

    #[test]
    fn blockquote_lowers_to_a_blockquote_of_nested_blocks() {
        let blocks = lower("> Quoted text\n>\n> ## Nested heading");
        assert_eq!(
            blocks,
            vec![Block::Blockquote(vec![
                Block::Paragraph(vec![Inline::Text("Quoted text".to_string())]),
                Block::Heading {
                    level: 2,
                    text: vec![Inline::Text("Nested heading".to_string())]
                },
            ])]
        );
    }

    #[test]
    fn fenced_code_block_lowers_with_language_and_raw_text() {
        let blocks = lower("```rust\nfn main() {}\n```");
        assert_eq!(
            blocks,
            vec![Block::Code {
                lang: Some("rust".to_string()),
                text: "fn main() {}\n".to_string(),
            }]
        );
    }

    #[test]
    fn thematic_break_lowers_to_a_horizontal_rule() {
        let blocks = lower("Before\n\n---\n\nAfter");
        assert_eq!(
            blocks,
            vec![
                Block::Paragraph(vec![Inline::Text("Before".to_string())]),
                Block::HorizontalRule,
                Block::Paragraph(vec![Inline::Text("After".to_string())]),
            ]
        );
    }

    #[test]
    fn links_lower_with_their_text_and_url() {
        let blocks = lower("See [the docs](https://example.com) for more.");
        assert_eq!(
            blocks,
            vec![Block::Paragraph(vec![
                Inline::Text("See ".to_string()),
                Inline::Link {
                    text: vec![Inline::Text("the docs".to_string())],
                    url: "https://example.com".to_string(),
                },
                Inline::Text(" for more.".to_string()),
            ])]
        );
    }

    #[test]
    fn footnote_reference_lowers_as_an_inline_marker() {
        let blocks = lower("A note[^1].\n\n[^1]: The footnote text.");
        assert_eq!(
            blocks,
            vec![
                Block::Paragraph(vec![
                    Inline::Text("A note".to_string()),
                    Inline::FootnoteReference("1".to_string()),
                    Inline::Text(".".to_string()),
                ]),
                Block::FootnoteDefinition {
                    label: "1".to_string(),
                    content: vec![Block::Paragraph(vec![Inline::Text(
                        "The footnote text.".to_string()
                    )])],
                },
            ]
        );
    }

    #[test]
    fn table_lowers_with_alignments_header_and_rows() {
        let blocks = lower("| a | b |\n| --- | :---: |\n| 1 | 2 |\n| 3 | 4 |");
        assert_eq!(
            blocks,
            vec![Block::Table {
                alignments: vec![ColumnAlignment::None, ColumnAlignment::Center],
                header: vec![
                    vec![Inline::Text("a".to_string())],
                    vec![Inline::Text("b".to_string())],
                ],
                rows: vec![
                    vec![
                        vec![Inline::Text("1".to_string())],
                        vec![Inline::Text("2".to_string())],
                    ],
                    vec![
                        vec![Inline::Text("3".to_string())],
                        vec![Inline::Text("4".to_string())],
                    ],
                ],
            }]
        );
    }

    #[test]
    fn lower_with_headings_collects_headings_in_document_order_with_block_index() {
        let source = "# Title\n\nIntro text.\n\n## Section\n\nMore text.\n\n### Sub";
        let (blocks, headings) = lower_with_headings(source);

        assert_eq!(
            headings,
            vec![
                HeadingRef {
                    level: 1,
                    text: "Title".to_string(),
                    block_index: 0,
                },
                HeadingRef {
                    level: 2,
                    text: "Section".to_string(),
                    block_index: 2,
                },
                HeadingRef {
                    level: 3,
                    text: "Sub".to_string(),
                    block_index: 4,
                },
            ]
        );
        // The wrapper `lower()` returns the same blocks lower_with_headings does.
        assert_eq!(blocks, lower(source));
    }

    #[test]
    fn an_image_lowers_to_an_image_block_with_its_alt_text_and_path() {
        let blocks = lower("![A diagram](diagram.png)");

        assert_eq!(
            blocks,
            vec![Block::Image {
                alt: "A diagram".to_string(),
                path: "diagram.png".to_string(),
            }]
        );
    }

    #[test]
    fn an_image_without_alt_text_keeps_an_empty_alt_and_its_path() {
        let blocks = lower("![](photos/holiday.jpg)");

        assert_eq!(
            blocks,
            vec![Block::Image {
                alt: String::new(),
                path: "photos/holiday.jpg".to_string(),
            }]
        );
    }

    #[test]
    fn an_image_path_that_points_nowhere_lowers_like_any_other() {
        let blocks = lower("![missing](does/not/exist.png)");

        assert_eq!(
            blocks,
            vec![Block::Image {
                alt: "missing".to_string(),
                path: "does/not/exist.png".to_string(),
            }]
        );
    }

    #[test]
    fn an_image_mid_paragraph_splits_the_text_around_it() {
        let blocks = lower("before ![chart](chart.png) after");

        assert_eq!(
            blocks,
            vec![
                Block::Paragraph(vec![Inline::Text("before ".to_string())]),
                Block::Image {
                    alt: "chart".to_string(),
                    path: "chart.png".to_string(),
                },
                Block::Paragraph(vec![Inline::Text(" after".to_string())]),
            ]
        );
    }

    #[test]
    fn an_image_in_a_heading_stays_inline_and_leaves_the_heading_intact() {
        let (blocks, headings) = lower_with_headings("# Title ![logo](l.png)");

        assert_eq!(
            blocks,
            vec![Block::Heading {
                level: 1,
                text: vec![
                    Inline::Text("Title ".to_string()),
                    Inline::Image {
                        alt: "logo".to_string(),
                        path: "l.png".to_string(),
                    },
                ],
            }]
        );
        assert_eq!(headings[0].text, "Title [logo]");
    }

    #[test]
    fn a_badge_image_stays_inside_its_link() {
        let blocks = lower("[![Build](badge.svg)](https://ci.example.com)");

        assert_eq!(
            blocks,
            vec![Block::Paragraph(vec![Inline::Link {
                text: vec![Inline::Image {
                    alt: "Build".to_string(),
                    path: "badge.svg".to_string(),
                }],
                url: "https://ci.example.com".to_string(),
            }])]
        );
    }

    #[test]
    fn an_image_in_a_table_cell_stays_in_the_cell() {
        let blocks = lower("| a |\n|---|\n| ![i](p.png) |");

        assert_eq!(
            blocks,
            vec![Block::Table {
                alignments: vec![ColumnAlignment::None],
                header: vec![vec![Inline::Text("a".to_string())]],
                rows: vec![vec![vec![Inline::Image {
                    alt: "i".to_string(),
                    path: "p.png".to_string(),
                }]]],
            }]
        );
    }

    #[test]
    fn an_image_inside_emphasis_stays_inline() {
        let blocks = lower("**![x](y.png)**");

        assert_eq!(
            blocks,
            vec![Block::Paragraph(vec![Inline::Bold(vec![Inline::Image {
                alt: "x".to_string(),
                path: "y.png".to_string(),
            }])])]
        );
    }
}
