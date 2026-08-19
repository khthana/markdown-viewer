use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::layout::Size;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block as RatBlock, Borders, Clear, List, ListItem, Paragraph};
use ratatui_image::StatefulImage;

use crate::app::{self, App, Focus, Mode};
use crate::image::{Gallery, Sizing};
use crate::markdown::blocks::{self, Block};
use crate::markdown::layout::{self, LayoutDoc};
use crate::search;
use crate::theme::Palette;
use crate::toc::TocEntry;

/// Everything one frame draws: the pager state plus the document it's
/// looking at, already parsed, laid out, and resolved. Bundled because
/// these always travel together and are always built from the same width
/// and the same palette.
#[derive(Clone, Copy)]
pub struct Screen<'a> {
    pub app: &'a App,
    pub blocks: &'a [Block],
    pub layout_doc: &'a LayoutDoc,
    pub toc: &'a [TocEntry],
    pub matches: &'a [search::Match],
    pub image_sizing: &'a Sizing,
    /// Must be the palette the `layout_doc` was built with: they agree on
    /// row counts only because every palette lays out identically.
    pub palette: Palette,
}

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
pub fn render(frame: &mut Frame, screen: Screen<'_>, gallery: &mut Gallery) {
    // `layout_doc` isn't unpacked here: only `render_images` needs it,
    // and it takes the whole `Screen`.
    let Screen {
        app,
        blocks,
        toc,
        matches,
        image_sizing,
        palette,
        ..
    } = screen;
    let area = frame.area();
    let (sidebar_area, main_area) = split_areas(area, app.toc_open);
    let (content_area, status_area) = split_status(main_area, search_status_visible(app));

    let mut lines = layout::render_lines(blocks, main_area.width as usize, image_sizing, palette);
    if app.search_active {
        for (i, m) in matches.iter().enumerate() {
            if let Some(line) = lines.get_mut(m.row) {
                let style = if Some(i) == app.current_match {
                    palette.search_current_match_style()
                } else {
                    palette.search_match_style()
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

    render_images(frame, content_area, screen, gallery);

    // Last, so it covers the document, the sidebar and the status row
    // alike.
    if app.help_open {
        render_help(frame, area, palette);
    }
}

/// Fills the rows `layout` reserved for each image: with the picture
/// itself when the whole of it is on screen, and with its alt-text label
/// otherwise.
///
/// An image that's only partly scrolled into view is not cropped —
/// re-fitting a picture on every scroll tick would cost a decode per
/// keystroke — but it can't be left blank either, so the reader sees the
/// same placeholder the untouchable tiers show.
fn render_images(frame: &mut Frame, content_area: Rect, screen: Screen<'_>, gallery: &mut Gallery) {
    let Screen {
        app,
        blocks,
        layout_doc,
        image_sizing: images,
        palette,
        ..
    } = screen;
    let viewport = app.scroll..app.scroll + content_area.height as usize;

    for laid_out in &layout_doc.blocks {
        let Some(Block::Image { alt, path }) = blocks.get(laid_out.block_index) else {
            continue;
        };
        if !images.draws(path) {
            continue;
        }
        let last_row = laid_out.row_start + laid_out.row_count;
        if last_row <= viewport.start || laid_out.row_start >= viewport.end {
            continue;
        }

        // The overlay is drawn after this, and a picture painted by the
        // terminal's own graphics protocol would sit on top of it rather
        // than under it — so while help is up, every image falls back to
        // the placeholder the untouchable tiers already show.
        let fully_visible =
            laid_out.row_start >= viewport.start && last_row <= viewport.end && !app.help_open;
        let top = laid_out.row_start.max(viewport.start);
        let area = Rect {
            x: content_area.x,
            y: content_area.y + (top - viewport.start) as u16,
            width: images.cols_for_path(path, content_area.width as usize) as u16,
            height: (last_row.min(viewport.end) - top) as u16,
        };

        // Asking is what starts the decode: an image is only fetched once
        // the reader has actually scrolled to it.
        let id = gallery.id_for(laid_out.block_index);
        if fully_visible {
            gallery.request(id, images, path);
            // Rendering is also what posts the encode job, so the widget
            // goes out either way — but on the frames where that's all it
            // does, it paints nothing and the placeholder has to stand in.
            let paints = gallery.paints_now(id, Size::new(area.width, area.height));
            if let Some(protocol) = gallery.protocol_mut(id) {
                frame.render_stateful_widget(
                    StatefulImage::default().resize(crate::image::IMAGE_RESIZE),
                    area,
                    protocol,
                );
                if paints {
                    continue;
                }
            }
        }

        // Still decoding, still encoding, undecodable, or half off screen.
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                blocks::image_placeholder(alt, path),
                palette.image_placeholder_style(),
            ))),
            Rect {
                width: content_area.width,
                ..area
            },
        );
    }
}

/// Draws the keybinding overlay over everything else, sized to its own
/// content and centred.
///
/// Built from `app::KEYBINDINGS`, which is also what `handle_key` is
/// checked against, so the overlay can't promise a key the app doesn't
/// honour.
fn render_help(frame: &mut Frame, area: Rect, palette: Palette) {
    let key_width = app::KEYBINDINGS
        .iter()
        .map(|binding| binding.keys.chars().count())
        .max()
        .unwrap_or(0);
    let title_style = palette.heading_style(2).style;
    let key_style = Style::new().add_modifier(Modifier::BOLD);

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (focus, title) in [
        (Focus::Pager, "Document"),
        (Focus::Outline, "Outline"),
        (Focus::Search, "While searching"),
    ] {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        lines.push(Line::from(Span::styled(title, title_style)));
        for binding in app::KEYBINDINGS.iter().filter(|b| b.focus == focus) {
            lines.push(Line::from(vec![
                Span::styled(format!("  {:key_width$}  ", binding.keys), key_style),
                Span::raw(binding.description),
            ]));
        }
    }

    let content_width = lines
        .iter()
        .map(|line| line.spans.iter().map(|s| s.content.chars().count()).sum())
        .max()
        .unwrap_or(0) as u16;
    // Plus borders on both sides, and a column of breathing room inside.
    let popup = centered(area, content_width + 4, lines.len() as u16 + 2);

    let block = RatBlock::new()
        .borders(Borders::ALL)
        .title("Keys")
        .padding(ratatui::widgets::Padding::horizontal(1));
    // Erases the document underneath, so the overlay isn't read as part
    // of the text it's covering.
    frame.render_widget(Clear, popup);
    frame.render_widget(Paragraph::new(Text::from(lines)).block(block), popup);
}

/// The largest `width` x `height` rectangle that fits inside `area`,
/// centred on it.
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
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

    /// Draws with the text-only image tier and a fresh layout at the
    /// frame's own width, which is what every test that isn't about
    /// images wants.
    fn text_render(
        frame: &mut Frame,
        app: &App,
        blocks: &[Block],
        toc: &[TocEntry],
        matches: &[search::Match],
    ) {
        text_render_with(frame, app, blocks, toc, matches, Palette::Dark);
    }

    /// As `text_render`, for the tests that are about the palette itself.
    fn text_render_with(
        frame: &mut Frame,
        app: &App,
        blocks: &[Block],
        toc: &[TocEntry],
        matches: &[search::Match],
        palette: Palette,
    ) {
        let images = Sizing::text_only();
        let (_, main_area) = split_areas(frame.area(), app.toc_open);
        let layout_doc = layout::layout(blocks, main_area.width as usize, &images, palette);
        render(
            frame,
            Screen {
                app,
                blocks,
                layout_doc: &layout_doc,
                toc,
                matches,
                image_sizing: &images,
                palette,
            },
            &mut Gallery::disabled(),
        );
    }
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
            .draw(|frame| text_render(frame, &app, &blocks, &[], &[]))
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
            .draw(|frame| text_render(frame, &app, &blocks, &[], &[]))
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

        let highlighted = highlight_line(line, 4, 9, Palette::Dark.search_match_style());

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
        assert_eq!(matched.style, Palette::Dark.search_match_style());
    }

    #[test]
    fn highlight_line_splits_a_match_that_crosses_a_span_boundary() {
        // "bold" is its own styled span; " text" is a separate plain
        // span. The match "ld te" (chars 2..7) straddles both.
        let bold_style = Style::new().add_modifier(Modifier::BOLD);
        let line = Line::from(vec![Span::styled("bold", bold_style), Span::raw(" text")]);

        let highlighted = highlight_line(line, 2, 7, Palette::Dark.search_match_style());

        let plain: String = highlighted
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(plain, "bold text");

        let highlight_style = Palette::Dark.search_match_style();
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
        let layout_doc = layout::layout(
            &blocks,
            80,
            &crate::image::Sizing::text_only(),
            Palette::Dark,
        );
        let matches = crate::search::search("fox", &layout_doc);
        assert_eq!(matches.len(), 3);

        let mut app = App::new(layout_doc.total_rows);
        app.search_active = true;
        app.current_match = Some(1); // the middle "fox"

        let backend = TestBackend::new(20, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| text_render(frame, &app, &blocks, &[], &matches))
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

        let layout_doc = layout::layout(
            &blocks,
            main_area.width as usize,
            &crate::image::Sizing::text_only(),
            Palette::Dark,
        );
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
            .draw(|frame| text_render(frame, &app, &blocks, &toc, &[]))
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
        let layout_doc = layout::layout(
            &blocks,
            main_area.width as usize,
            &crate::image::Sizing::text_only(),
            Palette::Dark,
        );
        let toc = crate::toc::resolve(&headings, &layout_doc);

        let mut app = App::new(layout_doc.total_rows);
        app.toc_open = true;
        app.toc_focused = true;
        app.toc_selected = 1;

        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| text_render(frame, &app, &blocks, &toc, &[]))
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
            .draw(|frame| text_render(frame, &app, &blocks, &[], &[]))
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
            .draw(|frame| text_render(frame, &app, &blocks, &[], &[]))
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
            .draw(|frame| text_render(frame, &app, blocks, &[], &[]))
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
        let matches = search::search(
            "fox",
            &layout::layout(
                &blocks,
                60,
                &crate::image::Sizing::text_only(),
                Palette::Dark,
            ),
        );

        let backend = TestBackend::new(60, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| text_render(frame, &app, &blocks, &[], &matches))
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

    #[test]
    fn an_image_renders_as_a_styled_alt_text_placeholder() {
        // The path points nowhere: this tier never opens the file, so a
        // broken reference renders like any other image.
        let blocks = crate::markdown::blocks::lower("![A diagram](does/not/exist.png)");
        let app = App::new(1);

        let backend = TestBackend::new(30, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| text_render(frame, &app, &blocks, &[], &[]))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let first_row: String = (0..30)
            .map(|x| buffer.cell((x, 0)).unwrap().symbol().to_string())
            .collect();
        assert!(
            first_row.contains("\u{1f5bc} [A diagram]"),
            "got: {first_row:?}"
        );
        assert!(
            buffer
                .cell((0, 0))
                .unwrap()
                .style()
                .add_modifier
                .contains(Modifier::ITALIC)
        );
    }

    /// A document with a picture in it, written to a scratch directory.
    fn image_fixture(name: &str, source: &str) -> (std::path::PathBuf, Vec<Block>, Sizing) {
        use ratatui_image::FontSize;

        let dir = std::env::temp_dir().join(format!("mdview-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Ten-pixel stripes: each half-block cell then has a different
        // colour above and below, which is what makes the tier draw a
        // glyph rather than a blank cell in one flat colour.
        ::image::RgbImage::from_fn(40, 40, |_, y| {
            if (y / 10) % 2 == 0 {
                ::image::Rgb([255, 0, 0])
            } else {
                ::image::Rgb([0, 0, 255])
            }
        })
        .save(dir.join("red.png"))
        .unwrap();

        let blocks = crate::markdown::blocks::lower(source);
        let images = Sizing::measure(&dir, FontSize::new(10, 20), ["red.png"]);
        (dir, blocks, images)
    }

    fn row_text(buffer: &ratatui::buffer::Buffer, y: u16, width: u16) -> String {
        (0..width)
            .map(|x| buffer.cell((x, y)).unwrap().symbol().to_string())
            .collect()
    }

    /// Does what the image thread would: decodes what was asked for and
    /// re-encodes what the last render asked to resize.
    fn work(gallery: &mut Gallery, jobs: &std::sync::mpsc::Receiver<crate::image::Job>) -> bool {
        use crate::image::Job;
        use ratatui_image::picker::Picker;

        let mut worked = false;
        while let Ok(job) = jobs.try_recv() {
            worked = true;
            match job {
                Job::Decode { id, file } => {
                    let protocol = crate::image::decode_protocol(&Picker::halfblocks(), &file);
                    gallery.image_decoded(id, protocol);
                }
                Job::Resize { id, request } => {
                    let response = request.resize_encode().expect("the image re-encodes");
                    gallery.image_resized(id, response);
                }
            }
        }
        worked
    }

    fn draw(
        terminal: &mut Terminal<TestBackend>,
        app: &App,
        blocks: &[Block],
        layout_doc: &LayoutDoc,
        images: &Sizing,
        gallery: &mut Gallery,
    ) {
        terminal
            .draw(|frame| {
                render(
                    frame,
                    Screen {
                        app,
                        blocks,
                        layout_doc,
                        toc: &[],
                        matches: &[],
                        image_sizing: images,
                        palette: Palette::Dark,
                    },
                    gallery,
                )
            })
            .unwrap();
        gallery.dispatch_resizes();
    }

    /// Draws until the picture has actually made it onto the screen:
    /// render, let the worker answer, render again.
    fn draw_until_settled(
        terminal: &mut Terminal<TestBackend>,
        app: &App,
        blocks: &[Block],
        layout_doc: &LayoutDoc,
        images: &Sizing,
        gallery: &mut Gallery,
        jobs: &std::sync::mpsc::Receiver<crate::image::Job>,
    ) {
        for _ in 0..4 {
            draw(terminal, app, blocks, layout_doc, images, gallery);
            if !work(gallery, jobs) {
                break;
            }
        }
        draw(terminal, app, blocks, layout_doc, images, gallery);
    }

    /// Whether the half-block tier has actually painted here: it draws
    /// upper-half-block glyphs, which nothing else in the viewer uses.
    fn is_painted(buffer: &ratatui::buffer::Buffer, cols: u16, rows: u16) -> bool {
        (0..cols).any(|x| {
            (0..rows).any(|y| {
                matches!(
                    buffer.cell((x, y)).unwrap().symbol(),
                    "\u{2580}" | "\u{2584}"
                )
            })
        })
    }

    #[test]
    fn a_drawable_image_is_painted_into_the_rows_reserved_for_it() {
        let (dir, blocks, images) = image_fixture("ui-image", "![Alt](red.png)");
        let layout_doc = layout::layout(&blocks, 20, &images, Palette::Dark);
        let (mut gallery, jobs) = crate::image::test_gallery();
        let mut app = App::new(layout_doc.total_rows);
        app.viewport_height = 5;

        let mut terminal = Terminal::new(TestBackend::new(20, 5)).unwrap();
        draw_until_settled(
            &mut terminal,
            &app,
            &blocks,
            &layout_doc,
            &images,
            &mut gallery,
            &jobs,
        );

        // 40x40 px at a 10x20 cell is 4 cols by 2 rows of half-blocks.
        assert_eq!(layout_doc.total_rows, 2);
        assert!(
            is_painted(terminal.backend().buffer(), 4, 2),
            "the image's rows should carry the picture's colours"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_image_shows_its_placeholder_until_the_worker_answers() {
        let (dir, blocks, images) = image_fixture("ui-image-pending", "![Alt](red.png)");
        let layout_doc = layout::layout(&blocks, 20, &images, Palette::Dark);
        let (mut gallery, jobs) = crate::image::test_gallery();
        let mut app = App::new(layout_doc.total_rows);
        app.viewport_height = 5;

        let mut terminal = Terminal::new(TestBackend::new(20, 5)).unwrap();
        // One frame, with the worker never getting a turn.
        draw(
            &mut terminal,
            &app,
            &blocks,
            &layout_doc,
            &images,
            &mut gallery,
        );

        assert!(
            row_text(terminal.backend().buffer(), 0, 20).contains("[Alt]"),
            "got: {:?}",
            row_text(terminal.backend().buffer(), 0, 20)
        );
        assert!(
            !is_painted(terminal.backend().buffer(), 4, 2),
            "nothing of the picture yet"
        );

        // And once it does answer, the picture replaces the placeholder.
        draw_until_settled(
            &mut terminal,
            &app,
            &blocks,
            &layout_doc,
            &images,
            &mut gallery,
            &jobs,
        );
        assert!(!row_text(terminal.backend().buffer(), 0, 20).contains("[Alt]"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn text_after_an_image_starts_below_the_rows_the_picture_reserved() {
        let (dir, blocks, images) = image_fixture("ui-image-after", "![Alt](red.png)\n\nAfter it.");
        let layout_doc = layout::layout(&blocks, 20, &images, Palette::Dark);
        let (mut gallery, jobs) = crate::image::test_gallery();
        let mut app = App::new(layout_doc.total_rows);
        app.viewport_height = 5;

        let mut terminal = Terminal::new(TestBackend::new(20, 5)).unwrap();
        draw_until_settled(
            &mut terminal,
            &app,
            &blocks,
            &layout_doc,
            &images,
            &mut gallery,
            &jobs,
        );

        // The picture takes rows 0-1, so the paragraph belongs on row 2.
        let buffer = terminal.backend().buffer();
        assert!(
            row_text(buffer, 2, 20).starts_with("After it."),
            "got: {:?}",
            row_text(buffer, 2, 20)
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_image_scrolled_half_off_screen_goes_back_to_its_placeholder() {
        let (dir, blocks, images) =
            image_fixture("ui-image-partial", "Above.\n\n![Alt](red.png)\n\nAfter it.");
        let layout_doc = layout::layout(&blocks, 20, &images, Palette::Dark);
        let (mut gallery, jobs) = crate::image::test_gallery();
        let mut app = App::new(layout_doc.total_rows);
        // Rows are "Above." 0, picture 1-2, "After it." 3.
        app.viewport_height = 4;

        let mut terminal = Terminal::new(TestBackend::new(20, 4)).unwrap();
        draw_until_settled(
            &mut terminal,
            &app,
            &blocks,
            &layout_doc,
            &images,
            &mut gallery,
            &jobs,
        );
        assert!(
            !row_text(terminal.backend().buffer(), 1, 20).contains("[Alt]"),
            "the picture is on screen to begin with"
        );

        // Scroll until only the picture's second row is left on screen.
        app.viewport_height = 2;
        app.scroll = 2;
        let mut terminal = Terminal::new(TestBackend::new(20, 2)).unwrap();
        draw_until_settled(
            &mut terminal,
            &app,
            &blocks,
            &layout_doc,
            &images,
            &mut gallery,
            &jobs,
        );

        assert!(
            row_text(terminal.backend().buffer(), 0, 20).contains("[Alt]"),
            "got: {:?}",
            row_text(terminal.backend().buffer(), 0, 20)
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_image_still_being_encoded_keeps_its_placeholder_rather_than_a_gap() {
        let (dir, blocks, images) = image_fixture("ui-image-encoding", "![Alt](red.png)");
        let layout_doc = layout::layout(&blocks, 20, &images, Palette::Dark);
        let (mut gallery, jobs) = crate::image::test_gallery();
        let mut app = App::new(layout_doc.total_rows);
        app.viewport_height = 5;

        let mut terminal = Terminal::new(TestBackend::new(20, 5)).unwrap();
        // Frame one asks for the decode; the worker answers it, and only
        // it — the re-encode that the next frame asks for is left pending.
        draw(
            &mut terminal,
            &app,
            &blocks,
            &layout_doc,
            &images,
            &mut gallery,
        );
        work(&mut gallery, &jobs);
        draw(
            &mut terminal,
            &app,
            &blocks,
            &layout_doc,
            &images,
            &mut gallery,
        );

        assert!(
            row_text(terminal.backend().buffer(), 0, 20).contains("[Alt]"),
            "got: {:?}",
            row_text(terminal.backend().buffer(), 0, 20)
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The style the frame actually gave one cell, rebuilt from the cell
    /// itself — what the terminal would emit, not what the code intended.
    fn cell_style(cell: &ratatui::buffer::Cell) -> Style {
        Style::new()
            .fg(cell.fg)
            .bg(cell.bg)
            .add_modifier(cell.modifier)
    }

    fn draw_with(app: &App, blocks: &[Block], palette: Palette) -> ratatui::buffer::Buffer {
        let mut terminal = Terminal::new(TestBackend::new(30, 6)).unwrap();
        terminal
            .draw(|frame| text_render_with(frame, app, blocks, &[], &[], palette))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    #[test]
    fn a_heading_is_painted_in_whichever_palette_the_screen_carries() {
        let blocks = crate::markdown::blocks::lower("# Title");
        let app = App::new(0);

        let colour_of =
            |palette| cell_style(draw_with(&app, &blocks, palette).cell((0, 0)).unwrap()).fg;

        assert_eq!(colour_of(Palette::Dark), Some(Color::Magenta));
        assert_eq!(colour_of(Palette::Light), Some(Color::Blue));
        assert_eq!(
            colour_of(Palette::Plain),
            Some(Color::Reset),
            "--no-color left a foreground colour on a heading"
        );
    }

    #[test]
    fn without_colour_a_heading_is_still_bold_and_still_keeps_its_rule_row() {
        let blocks = crate::markdown::blocks::lower("# Title");
        let app = App::new(0);
        let buffer = draw_with(&app, &blocks, Palette::Plain);

        assert!(
            cell_style(buffer.cell((0, 0)).unwrap())
                .add_modifier
                .contains(Modifier::BOLD),
            "bold is all a colourless heading has left"
        );
        assert_eq!(
            buffer.cell((0, 1)).unwrap().symbol(),
            "\u{2500}",
            "the rule row under the heading has to survive --no-color"
        );
    }

    #[test]
    fn without_colour_the_selected_search_match_still_stands_out() {
        let blocks = crate::markdown::blocks::lower("alpha beta alpha");
        let mut app = App::new(0);
        app.search_active = true;
        app.search_query = "alpha".to_string();
        app.current_match = Some(0);

        let images = Sizing::text_only();
        let layout_doc = layout::layout(&blocks, 30, &images, Palette::Plain);
        let matches = search::search("alpha", &layout_doc);
        assert_eq!(matches.len(), 2);

        let mut terminal = Terminal::new(TestBackend::new(30, 3)).unwrap();
        terminal
            .draw(|frame| text_render_with(frame, &app, &blocks, &[], &matches, Palette::Plain))
            .unwrap();
        let buffer = terminal.backend().buffer();

        let selected = buffer.cell((0, 0)).unwrap();
        let other = buffer.cell((matches[1].start as u16, 0)).unwrap();
        assert_eq!(selected.fg, Color::Reset, "--no-color coloured a match");
        assert_eq!(other.fg, Color::Reset, "--no-color coloured a match");
        assert!(
            selected.modifier.contains(Modifier::REVERSED)
                && other.modifier.contains(Modifier::REVERSED),
            "matches need reverse video once colour is gone"
        );
        assert_ne!(
            selected.modifier, other.modifier,
            "the selected match must still be distinguishable from the rest"
        );
    }

    #[test]
    fn the_help_overlay_lists_every_binding_over_the_document() {
        let blocks = crate::markdown::blocks::lower("body text");
        let mut app = App::new(0);
        app.help_open = true;

        // 80 columns: the overlay has to fit the classic terminal width
        // without clipping a description.
        let mut terminal = Terminal::new(TestBackend::new(80, 30)).unwrap();
        terminal
            .draw(|frame| text_render(frame, &app, &blocks, &[], &[]))
            .unwrap();
        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        for binding in crate::app::KEYBINDINGS {
            assert!(
                screen.contains(binding.keys),
                "the overlay left out {:?}",
                binding.keys
            );
            assert!(
                screen.contains(binding.description),
                "the overlay left out {:?}",
                binding.description
            );
        }
        assert!(screen.contains("Keys"), "the overlay has no title");
    }

    #[test]
    fn the_document_is_back_once_the_overlay_closes() {
        // Long enough that the middle of the screen — where the overlay
        // is centred — has document text on it to be covered.
        let source: String = (0..25)
            .map(|i| {
                format!(
                    "paragraph{i}

"
                )
            })
            .collect::<Vec<_>>()
            .join("");
        let blocks = crate::markdown::blocks::lower(&source);
        let mut app = App::new(0);
        app.help_open = true;

        let mut terminal = Terminal::new(TestBackend::new(70, 30)).unwrap();
        terminal
            .draw(|frame| text_render(frame, &app, &blocks, &[], &[]))
            .unwrap();
        assert!(
            !row_text(terminal.backend().buffer(), 15, 70).contains("paragraph"),
            "the overlay should be covering the document"
        );

        app.help_open = false;
        terminal
            .draw(|frame| text_render(frame, &app, &blocks, &[], &[]))
            .unwrap();
        assert!(
            row_text(terminal.backend().buffer(), 15, 70).contains("paragraph"),
            "the document should be uncovered again"
        );
    }

    #[test]
    fn an_overlay_larger_than_the_terminal_is_clipped_not_panicked_on() {
        let blocks = crate::markdown::blocks::lower("body text");
        let mut app = App::new(0);
        app.help_open = true;

        // Smaller than the overlay wants to be in both directions.
        let mut terminal = Terminal::new(TestBackend::new(12, 4)).unwrap();
        terminal
            .draw(|frame| text_render(frame, &app, &blocks, &[], &[]))
            .unwrap();
    }
}
