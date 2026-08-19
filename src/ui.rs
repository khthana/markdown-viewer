use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block as RatBlock, Borders, List, ListItem, Paragraph};

use crate::app::{App, Mode};
use crate::markdown::blocks::Block;
use crate::markdown::layout;
use crate::search;
use crate::theme;
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

/// Splits a pane's area into the content area and, when `show`, a
/// reserved bottom row for the search status line (live query while
/// typing, or a "no matches" indicator after confirming). Called by both
/// `main.rs` (for `App::viewport_height`) and `render`, on the same area,
/// for the same reason `split_areas` is shared: the two must never
/// compute different heights for the same frame.
pub fn split_status(area: Rect, show: bool) -> (Rect, Option<Rect>) {
    if !show {
        return (area, None);
    }
    let chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Min(0),
            ratatui::layout::Constraint::Length(1),
        ])
        .split(area);
    (chunks[0], Some(chunks[1]))
}

/// Patches `style` onto the half-open char range `[start, end)` of
/// `line`, splitting spans at the range's boundaries so the rest of the
/// line keeps its existing style. `start`/`end` are char offsets into the
/// line's full plain text, matching `search::Match`'s row-relative
/// offsets.
fn highlight_line(line: Line<'static>, start: usize, end: usize, style: Style) -> Line<'static> {
    if start >= end {
        return line;
    }

    let mut spans = Vec::with_capacity(line.spans.len());
    let mut col = 0usize;

    for span in line.spans {
        let chars: Vec<char> = span.content.chars().collect();
        let span_start = col;
        let span_end = col + chars.len();
        col = span_end;

        if end <= span_start || start >= span_end {
            spans.push(span);
            continue;
        }

        let local_start = start.saturating_sub(span_start);
        let local_end = end.min(span_end) - span_start;

        let before: String = chars[..local_start].iter().collect();
        let matched: String = chars[local_start..local_end].iter().collect();
        let after: String = chars[local_end..].iter().collect();

        if !before.is_empty() {
            spans.push(Span::styled(before, span.style));
        }
        if !matched.is_empty() {
            spans.push(Span::styled(matched, span.style.patch(style)));
        }
        if !after.is_empty() {
            spans.push(Span::styled(after, span.style));
        }
    }

    Line::from(spans)
}

/// Renders the document in a scrollable pane, plus the TOC sidebar when
/// open.
///
/// Uses `layout::render_lines` (the same function `layout::layout` uses
/// to compute row counts) so what's on screen always matches what
/// `App`'s scroll math thinks is there. `toc` must have been resolved
/// against a `LayoutDoc` built at this same main-pane width — callers
/// should use `split_areas` on the same `area` for that layout pass too.
/// `matches` is only painted onto the text when `app.search_active`, and
/// must have been resolved against that same `LayoutDoc`.
pub fn render(
    frame: &mut Frame,
    app: &App,
    blocks: &[Block],
    toc: &[TocEntry],
    matches: &[search::Match],
) {
    let area = frame.area();
    let (sidebar_area, main_area) = split_areas(area, app.toc_open);
    let (content_area, status_area) = split_status(main_area, search_status_visible(app));

    let mut lines = layout::render_lines(blocks, main_area.width as usize);
    if app.search_active {
        for (i, m) in matches.iter().enumerate() {
            if let Some(line) = lines.get_mut(m.row) {
                let style = if Some(i) == app.current_match {
                    theme::search_current_match_style()
                } else {
                    theme::search_match_style()
                };
                *line = highlight_line(std::mem::take(line), m.start, m.end, style);
            }
        }
    }
    let paragraph = Paragraph::new(Text::from(lines)).scroll((app.scroll as u16, 0));
    frame.render_widget(paragraph, content_area);

    if let Some(status_area) = status_area {
        render_status(frame, status_area, app, matches);
    }

    if let Some(sidebar_area) = sidebar_area {
        render_toc(frame, sidebar_area, toc, app.toc_selected, app.toc_focused);
    }
}

/// Whether the bottom status row should be reserved: while typing a
/// query, or while a confirmed search is active (to show either its
/// highlighted-match state or a "no matches" indicator). Shared between
/// `render` and `main.rs`'s `App::viewport_height` bookkeeping so both
/// agree on how tall the content pane actually is.
pub fn search_status_visible(app: &App) -> bool {
    app.mode == Mode::Search || app.search_active
}

fn render_status(frame: &mut Frame, area: Rect, app: &App, matches: &[search::Match]) {
    let text = if app.mode == Mode::Search {
        format!("/{}", app.search_query)
    } else if app.search_active && matches.is_empty() {
        format!("No matches for \"{}\"", app.search_query)
    } else if app.search_fell_back {
        // Only shown after a reload dropped the selected match: the count
        // is worth stating precisely because the document just changed
        // under the reader.
        format!(
            "Match 1/{} for \"{}\" (previous match gone)",
            matches.len(),
            app.search_query
        )
    } else {
        String::new()
    };
    frame.render_widget(Paragraph::new(text), area);
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
    fn split_status_returns_full_area_when_not_shown() {
        let area = Rect::new(0, 0, 80, 24);
        let (content, status) = split_status(area, false);
        assert_eq!(status, None);
        assert_eq!(content, area);
    }

    #[test]
    fn split_status_reserves_the_bottom_row_when_shown() {
        let area = Rect::new(0, 0, 80, 24);
        let (content, status) = split_status(area, true);
        let status = status.expect("status row should be present when shown");

        assert_eq!(
            content.height,
            area.height - 1,
            "content pane loses one row"
        );
        assert_eq!(status.height, 1);
        assert_eq!(
            status.y,
            content.y + content.height,
            "status sits below content"
        );
        assert_eq!(status.width, area.width);
    }

    #[test]
    fn typing_a_search_query_shows_it_on_the_status_line() {
        let blocks = crate::markdown::blocks::lower("hello world");
        let mut app = App::new(0);
        app.mode = Mode::Search;
        app.search_query = "wor".to_string();

        let backend = TestBackend::new(20, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &app, &blocks, &[], &[]))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let status_row: String = (0..20)
            .map(|x| buffer.cell((x, 2)).unwrap().symbol().to_string())
            .collect();
        assert!(status_row.starts_with("/wor"), "got: {status_row:?}");
    }

    #[test]
    fn confirmed_search_with_no_matches_shows_a_no_matches_indicator() {
        let blocks = crate::markdown::blocks::lower("hello world");
        let mut app = App::new(0);
        app.search_active = true;
        app.search_query = "xyz".to_string();

        let backend = TestBackend::new(20, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &app, &blocks, &[], &[]))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let status_row: String = (0..20)
            .map(|x| buffer.cell((x, 2)).unwrap().symbol().to_string())
            .collect();
        assert!(status_row.contains("No matches"), "got: {status_row:?}");
    }

    #[test]
    fn highlight_line_patches_style_onto_the_matched_range_and_preserves_the_rest() {
        let line = Line::from(Span::raw("the quick brown fox"));

        let highlighted = highlight_line(line, 4, 9, crate::theme::search_match_style());

        let plain: String = highlighted
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(
            plain, "the quick brown fox",
            "content is unchanged, only style differs"
        );
        let matched = highlighted
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "quick")
            .expect("the matched substring should be its own span");
        assert_eq!(matched.style, crate::theme::search_match_style());
    }

    #[test]
    fn highlight_line_splits_a_match_that_crosses_a_span_boundary() {
        // "bold" is its own styled span; " text" is a separate plain
        // span. The match "ld te" (chars 2..7) straddles both.
        let bold_style = Style::new().add_modifier(Modifier::BOLD);
        let line = Line::from(vec![Span::styled("bold", bold_style), Span::raw(" text")]);

        let highlighted = highlight_line(line, 2, 7, crate::theme::search_match_style());

        let plain: String = highlighted
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(plain, "bold text");

        let highlight_style = crate::theme::search_match_style();
        let highlighted_text: String = highlighted
            .spans
            .iter()
            .filter(|s| s.style == bold_style.patch(highlight_style) || s.style == highlight_style)
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(
            highlighted_text, "ld te",
            "the matched substring spanning both original spans is fully highlighted"
        );
    }

    #[test]
    fn confirmed_search_highlights_the_current_match_distinctly_from_other_matches() {
        let source = "fox fox fox";
        let blocks = crate::markdown::blocks::lower(source);
        let layout_doc = layout::layout(&blocks, 80);
        let matches = crate::search::search("fox", &layout_doc);
        assert_eq!(matches.len(), 3);

        let mut app = App::new(layout_doc.total_rows);
        app.search_active = true;
        app.current_match = Some(1); // the middle "fox"

        let backend = TestBackend::new(20, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &app, &blocks, &[], &matches))
            .unwrap();

        let buffer = terminal.backend().buffer();
        // "fox fox fox": matches at cols 0-2, 4-6, 8-10.
        assert_eq!(
            buffer.cell((0, 0)).unwrap().style().bg,
            Some(Color::Yellow),
            "first match uses the ordinary match style"
        );
        assert_eq!(
            buffer.cell((4, 0)).unwrap().style().bg,
            Some(Color::LightYellow),
            "the selected match (index 1) is visually distinct"
        );
        assert_eq!(
            buffer.cell((8, 0)).unwrap().style().bg,
            Some(Color::Yellow),
            "third match uses the ordinary match style"
        );
    }

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
            .draw(|frame| render(frame, &app, &blocks, &toc, &[]))
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
            .draw(|frame| render(frame, &app, &blocks, &toc, &[]))
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
            .draw(|frame| render(frame, &app, &blocks, &[], &[]))
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
            .draw(|frame| render(frame, &app, &blocks, &[], &[]))
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
            .draw(|frame| render(frame, &app, blocks, &[], &[]))
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

    #[test]
    fn a_reload_that_moved_the_selection_says_so_on_the_status_line() {
        let blocks = crate::markdown::blocks::lower("fox one\n\nfox two");
        let mut app = App::new(0);
        app.search_active = true;
        app.search_query = "fox".to_string();
        app.apply_reselection(search::Reselection::FellBackToFirst);
        let matches = search::search("fox", &layout::layout(&blocks, 60));

        let backend = TestBackend::new(60, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &app, &blocks, &[], &matches))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let status_row: String = (0..60)
            .map(|x| buffer.cell((x, 3)).unwrap().symbol().to_string())
            .collect();
        assert!(
            status_row.contains("Match 1/2 for \"fox\" (previous match gone)"),
            "got: {status_row:?}"
        );
    }
}
