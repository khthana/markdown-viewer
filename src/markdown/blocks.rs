use pulldown_cmark::{CodeBlockKind, Event as CmEvent, Tag, TagEnd};

use crate::markdown::parser;

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Bold(Vec<Inline>),
    Italic(Vec<Inline>),
    Link { text: Vec<Inline>, url: String },
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

    for event in parser::parse(source) {
        match event {
            CmEvent::Start(Tag::Paragraph | Tag::Heading { .. } | Tag::Strong | Tag::Emphasis) => {
                stack.push(Vec::new())
            }
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
            }
            CmEvent::End(TagEnd::Item) => {
                let direct_text = stack.pop().unwrap_or_default();
                if !direct_text.is_empty() {
                    push_block(&mut blocks, &mut containers, Block::Paragraph(direct_text));
                }
                let item = containers.pop().unwrap_or_default();
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
}
