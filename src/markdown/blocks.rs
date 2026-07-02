use pulldown_cmark::{Alignment, CodeBlockKind, Event as CmEvent, Tag, TagEnd};

use crate::markdown::parser;

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Bold(Vec<Inline>),
    Italic(Vec<Inline>),
    Strikethrough(Vec<Inline>),
    Link { text: Vec<Inline>, url: String },
    FootnoteReference(String),
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
}

/// Lowers CommonMark `source` into an owned `Vec<Block>`.
///
/// Inline content (text, bold, italic, ...) can nest, so open spans are
/// tracked as a stack of buffers: starting a span pushes a fresh buffer,
/// ending one pops it and appends the finished span to its parent. Block
/// containers (blockquotes, list items) nest the same way one level up,
/// via a stack of block buffers.
pub fn lower(source: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut containers: Vec<Vec<Block>> = Vec::new();
    let mut stack: Vec<Vec<Inline>> = Vec::new();
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

    for event in parser::parse(source) {
        match event {
            CmEvent::Start(
                Tag::Paragraph
                | Tag::Heading { .. }
                | Tag::Strong
                | Tag::Emphasis
                | Tag::Strikethrough,
            ) => stack.push(Vec::new()),
            CmEvent::Start(Tag::Link { dest_url, .. }) => {
                link_urls.push(dest_url.into_string());
                stack.push(Vec::new());
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
            CmEvent::Start(Tag::TableCell) => stack.push(Vec::new()),
            CmEvent::End(TagEnd::TableCell) => {
                let cell = stack.pop().unwrap_or_default();
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
                stack.push(Vec::new());
                task_marker_stack.push(None);
            }
            CmEvent::TaskListMarker(checked) => {
                if let Some(top) = task_marker_stack.last_mut() {
                    *top = Some(checked);
                }
            }
            CmEvent::End(TagEnd::Item) => {
                let direct_text = stack.pop().unwrap_or_default();
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
                let text = stack.pop().unwrap_or_default();
                push_block(&mut blocks, &mut containers, Block::Paragraph(text));
            }
            CmEvent::End(TagEnd::Heading(level)) => {
                let text = stack.pop().unwrap_or_default();
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
                let inner = stack.pop().unwrap_or_default();
                push_inline(&mut stack, Inline::Bold(inner));
            }
            CmEvent::End(TagEnd::Emphasis) => {
                let inner = stack.pop().unwrap_or_default();
                push_inline(&mut stack, Inline::Italic(inner));
            }
            CmEvent::End(TagEnd::Strikethrough) => {
                let inner = stack.pop().unwrap_or_default();
                push_inline(&mut stack, Inline::Strikethrough(inner));
            }
            CmEvent::End(TagEnd::Link) => {
                let text = stack.pop().unwrap_or_default();
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

    blocks
}

fn column_alignment(alignment: Alignment) -> ColumnAlignment {
    match alignment {
        Alignment::None => ColumnAlignment::None,
        Alignment::Left => ColumnAlignment::Left,
        Alignment::Center => ColumnAlignment::Center,
        Alignment::Right => ColumnAlignment::Right,
    }
}

fn push_inline(stack: &mut [Vec<Inline>], inline: Inline) {
    if let Some(top) = stack.last_mut() {
        top.push(inline);
    }
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
    stack: &mut [Vec<Inline>],
    blocks: &mut Vec<Block>,
    containers: &mut [Vec<Block>],
) {
    if let Some(top) = stack.last_mut()
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
}
