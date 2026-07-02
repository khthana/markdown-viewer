use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Text;
use ratatui::widgets::{Block as RatBlock, Borders, List, ListItem, Paragraph};

use crate::app::App;
use crate::markdown::blocks::Block;
use crate::markdown::layout;
use crate::toc::TocEntry;

/// Splits a frame's area into an optional TOC sidebar and the main
/// content pane. Used by both `main.rs` (to lay out the document at the
/// right width) and `render` (to draw at that same width) — a single
/// seam so the two can't compute different widths for the same frame.
pub fn split_areas(area: Rect, toc_open: bool) -> (Option<Rect>, Rect) {
    if !toc_open {
        return (None, area);
    }
    let chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            ratatui::layout::Constraint::Length(TOC_WIDTH),
            ratatui::layout::Constraint::Min(0),
        ])
        .split(area);
    (Some(chunks[0]), chunks[1])
}

const TOC_WIDTH: u16 = 28;

/// Renders the document in a scrollable pane, plus the TOC sidebar when
/// open.
///
/// Uses `layout::render_lines` (the same function `layout::layout` uses
/// to compute row counts) so what's on screen always matches what
/// `App`'s scroll math thinks is there. `toc` must have been resolved
/// against a `LayoutDoc` built at this same main-pane width — callers
/// should use `split_areas` on the same `area` for that layout pass too.
pub fn render(frame: &mut Frame, app: &App, blocks: &[Block], toc: &[TocEntry]) {
    let area = frame.area();
    let (sidebar_area, main_area) = split_areas(area, app.toc_open);

    let lines = layout::render_lines(blocks, main_area.width as usize);
    let paragraph = Paragraph::new(Text::from(lines)).scroll((app.scroll as u16, 0));
    frame.render_widget(paragraph, main_area);

    if let Some(sidebar_area) = sidebar_area {
        render_toc(frame, sidebar_area, toc, app.toc_selected, app.toc_focused);
    }
}

fn render_toc(frame: &mut Frame, area: Rect, toc: &[TocEntry], selected: usize, focused: bool) {
    let items: Vec<ListItem> = toc
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let indent = " ".repeat(entry.level.saturating_sub(1) as usize * 2);
            let style = if i == selected {
                Style::new().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            ListItem::new(format!("{indent}{}", entry.text)).style(style)
        })
        .collect();

    let border_style = if focused {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let block = RatBlock::new()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title("Outline");
    frame.render_widget(List::new(items).block(block), area);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::{Color, Modifier};

    use super::*;
    use crate::markdown::blocks::Inline;

    #[test]
    fn split_areas_returns_full_area_when_toc_closed() {
        let area = Rect::new(0, 0, 80, 24);
        let (sidebar, main) = split_areas(area, false);
        assert_eq!(sidebar, None);
        assert_eq!(main, area);
    }

    #[test]
    fn split_areas_narrows_main_pane_when_toc_open() {
        let area = Rect::new(0, 0, 80, 24);
        let (sidebar, main) = split_areas(area, true);
        let sidebar = sidebar.expect("sidebar should be present when open");

        assert_eq!(sidebar.x, 0);
        assert_eq!(main.x, sidebar.width);
        assert_eq!(sidebar.width + main.width, area.width);
        assert_eq!(
            main.height, area.height,
            "sidebar is horizontal, full height"
        );
    }

    #[test]
    fn toc_row_resolution_matches_actual_render_width_when_sidebar_is_open() {
        // Regression test: TOC rows must be resolved against the
        // *narrowed* main-pane width (post-split), not the full frame
        // width — otherwise jumping to a heading lands on the wrong row
        // once the sidebar takes up horizontal space.
        let area = Rect::new(0, 0, 50, 10);
        let (_, main_area) = split_areas(area, true);

        let source = "one two three four five six seven eight\n\n# Target";
        let (blocks, headings) = crate::markdown::blocks::lower_with_headings(source);

        let layout_doc = layout::layout(&blocks, main_area.width as usize);
        let toc = crate::toc::resolve(&headings, &layout_doc);
        let target_row = toc[0].row;
        assert!(
            target_row > 0,
            "the paragraph should wrap to more than one row at the narrowed width, \
             pushing the heading down"
        );

        let mut app = App::new(layout_doc.total_rows);
        app.toc_open = true;
        app.scroll = target_row;
        app.viewport_height = area.height as usize;

        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &app, &blocks, &toc))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let sidebar_width = area.width - main_area.width;
        let first_char = buffer
            .cell((sidebar_width, 0))
            .unwrap()
            .symbol()
            .to_string();
        assert_eq!(
            first_char, "T",
            "scrolling to the resolved row should show the heading, not paragraph overflow"
        );
    }

    #[test]
    fn toc_open_snapshot_shows_sidebar_with_indented_entries_and_selection() {
        let source = "# Title\n\nIntro.\n\n## Section One\n\n### Sub";
        let (blocks, headings) = crate::markdown::blocks::lower_with_headings(source);
        let area = Rect::new(0, 0, 40, 6);
        let (_, main_area) = split_areas(area, true);
        let layout_doc = layout::layout(&blocks, main_area.width as usize);
        let toc = crate::toc::resolve(&headings, &layout_doc);

        let mut app = App::new(layout_doc.total_rows);
        app.toc_open = true;
        app.toc_focused = true;
        app.toc_selected = 1;

        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &app, &blocks, &toc))
            .unwrap();

        let buffer = terminal.backend().buffer();
        // Sidebar has a bordered box titled "Outline" with each entry
        // indented by heading level; the selected entry (index 1) is
        // reverse-styled.
        let title_row: String = (0..12)
            .map(|x| buffer.cell((x, 0)).unwrap().symbol().to_string())
            .collect();
        assert!(title_row.contains("Outline"), "got: {title_row:?}");

        // Row 2 is "Section One" (index 1, selected); indented 2 spaces
        // for its level-2 heading. Row 1 ("Title") is unselected.
        let selected_cell = buffer.cell((3, 2)).unwrap();
        assert!(
            selected_cell
                .style()
                .add_modifier
                .contains(Modifier::REVERSED),
            "selected TOC entry should be reverse-styled"
        );
        let unselected_cell = buffer.cell((1, 1)).unwrap();
        assert!(
            !unselected_cell
                .style()
                .add_modifier
                .contains(Modifier::REVERSED),
            "unselected TOC entries should not be reverse-styled"
        );
    }

    #[test]
    fn headings_render_with_distinct_styles_and_rules() {
        let blocks = vec![
            Block::Heading {
                level: 1,
                text: vec![Inline::Text("H1".to_string())],
            },
            Block::Heading {
                level: 2,
                text: vec![Inline::Text("H2".to_string())],
            },
            Block::Heading {
                level: 3,
                text: vec![Inline::Text("H3".to_string())],
            },
        ];

        let backend = TestBackend::new(20, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::new(0);
        terminal
            .draw(|frame| render(frame, &app, &blocks, &[]))
            .unwrap();

        let buffer = terminal.backend().buffer();

        let h1 = buffer.cell((0, 0)).unwrap();
        assert_eq!(h1.symbol(), "H");
        assert_eq!(h1.style().fg, Some(Color::Magenta));
        assert!(h1.style().add_modifier.contains(Modifier::BOLD));

        let h1_rule = buffer.cell((0, 1)).unwrap();
        assert_eq!(h1_rule.symbol(), "\u{2500}");
        assert_eq!(h1_rule.style().fg, Some(Color::Magenta));

        let h2 = buffer.cell((0, 2)).unwrap();
        assert_eq!(h2.symbol(), "H");
        assert_eq!(h2.style().fg, Some(Color::Cyan));
        assert!(h2.style().add_modifier.contains(Modifier::BOLD));

        let h2_rule = buffer.cell((0, 3)).unwrap();
        assert_eq!(h2_rule.style().fg, Some(Color::LightCyan));

        let h3 = buffer.cell((0, 4)).unwrap();
        assert_eq!(h3.symbol(), "H");
        assert_eq!(h3.style().fg, Some(Color::Yellow));
        assert!(h3.style().add_modifier.contains(Modifier::BOLD));

        // H3 has no rule: row 5 is the next block's content, not a rule.
        assert_ne!(buffer.cell((0, 5)).unwrap().symbol(), "\u{2500}");
    }

    #[test]
    fn full_document_snapshot_across_all_block_types() {
        let source = "# H1\n\nSome paragraph text.\n\n- item one\n- item two\n\n\
                       > A quote\n\n1. first\n2. second\n\n---\n\n```\ncode line\n```";
        let blocks = crate::markdown::blocks::lower(source);
        let rows = render_to_rows(&blocks, 20, 18);

        assert_eq!(
            rows,
            vec![
                "H1                  ",
                "────────────────────",
                "Some paragraph text.",
                "• item one          ",
                "• item two          ",
                "│ A quote           ",
                "1. first            ",
                "2. second           ",
                "────────────────────",
                "code line           ",
                "                    ",
                "                    ",
                "                    ",
                "                    ",
                "                    ",
                "                    ",
                "                    ",
                "                    ",
            ]
        );
    }

    #[test]
    fn gfm_snapshot_across_tables_task_lists_strikethrough_footnotes() {
        let source = "| a | b |\n| --- | :---: |\n| 1 | 22 |\n\n\
                       - [x] done\n- [ ] todo\n\n\
                       ~~struck~~ text.\n\n\
                       A note[^1].\n\n[^1]: The footnote.";
        let blocks = crate::markdown::blocks::lower(source);
        let rows = render_to_rows(&blocks, 30, 12);

        assert_eq!(
            rows,
            vec![
                "a │ b                         ",
                "──┼───                        ",
                "1 │ 22                        ",
                "☑ done                        ",
                "☐ todo                        ",
                "struck text.                  ",
                "A note [^1] .                 ",
                "[^1]: The footnote.           ",
                "                              ",
                "                              ",
                "                              ",
                "                              ",
            ]
        );
    }

    #[test]
    fn snapshot_with_a_highlighted_code_block_shows_more_than_one_token_color() {
        let source = "# Example\n\n```rust\nfn main() {\n    let s = \"hi\"; // greet\n}\n```";
        let blocks = crate::markdown::blocks::lower(source);

        let backend = TestBackend::new(30, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::new(0);
        terminal
            .draw(|frame| render(frame, &app, &blocks, &[]))
            .unwrap();

        let buffer = terminal.backend().buffer();
        // Rows 2-4 are the code block's three lines (row 0: heading text,
        // row 1: heading rule).
        let code_fg_colors: std::collections::HashSet<_> = (2..5)
            .flat_map(|y| (0..30).map(move |x| (x, y)))
            .filter_map(|(x, y)| buffer.cell((x, y)))
            .map(|cell| cell.style().fg)
            .collect();

        assert!(
            code_fg_colors.len() > 1,
            "expected multiple distinct token colors in the code block, got {code_fg_colors:?}"
        );
    }

    fn render_to_rows(blocks: &[Block], width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::new(0);
        terminal
            .draw(|frame| render(frame, &app, blocks, &[]))
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer.cell((x, y)).unwrap().symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }
}
