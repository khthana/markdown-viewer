use ratatui::Frame;
use ratatui::text::Text;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::markdown::blocks::Block;
use crate::markdown::layout;

/// Renders the document in a scrollable pane.
///
/// Uses `layout::render_lines` (the same function `layout::layout` uses
/// to compute row counts) so what's on screen always matches what
/// `App`'s scroll math thinks is there.
pub fn render(frame: &mut Frame, app: &App, blocks: &[Block]) {
    let area = frame.area();
    let lines = layout::render_lines(blocks, area.width as usize);
    let paragraph = Paragraph::new(Text::from(lines)).scroll((app.scroll as u16, 0));
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::{Color, Modifier};

    use super::*;
    use crate::markdown::blocks::Inline;

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
        terminal.draw(|frame| render(frame, &app, &blocks)).unwrap();

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

        let backend = TestBackend::new(20, 18);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::new(0);
        terminal.draw(|frame| render(frame, &app, &blocks)).unwrap();

        let buffer = terminal.backend().buffer();
        let rows: Vec<String> = (0..18)
            .map(|y| {
                (0..20)
                    .map(|x| buffer.cell((x, y)).unwrap().symbol().to_string())
                    .collect::<String>()
            })
            .collect();

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
}
