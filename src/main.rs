mod app;
mod cli;
mod event;
mod highlight;
mod markdown;
mod reload;
mod search;
mod theme;
mod toc;
mod ui;
mod watch;

use std::path::Path;

use clap::Parser;
use ratatui::layout::Rect;

use app::{App, KeyOutcome};
use event::{Event, Sources};
use markdown::layout;
use reload::Document;

fn main() -> anyhow::Result<()> {
    let args = cli::Args::parse();
    let document = reload::load(&args.file)?;
    // Started before the alternate screen so a watcher warning is visible.
    let sources = event::spawn_sources(&args.file);

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &args.file, document, &sources);
    ratatui::restore();
    result
}

fn run(
    terminal: &mut ratatui::DefaultTerminal,
    path: &Path,
    mut document: Document,
    sources: &Sources,
) -> anyhow::Result<()> {
    let mut app = App::new(0);

    loop {
        let size = terminal.size()?;
        let area = Rect::new(0, 0, size.width, size.height);
        let (_, main_area) = ui::split_areas(area, app.toc_open);
        let (content_area, _) = ui::split_status(main_area, ui::search_status_visible(&app));

        app.viewport_height = content_area.height as usize;
        let width = main_area.width as usize;
        let layout_doc = layout::layout(&document.blocks, width);
        app.total_rows = layout_doc.total_rows;
        let toc_entries = toc::resolve(&document.headings, &layout_doc);
        let matches = search::search(&app.search_query, &layout_doc);

        terminal.draw(|frame| ui::render(frame, &app, &document.blocks, &toc_entries, &matches))?;

        match sources.recv() {
            Ok(Event::Key(key)) => match app.on_key(key, &toc_entries, &matches) {
                KeyOutcome::Quit => return Ok(()),
                KeyOutcome::Reload => reload::reload_preserving_position(
                    path,
                    &mut document,
                    &mut app,
                    &layout_doc,
                    width,
                ),
                KeyOutcome::Continue => {}
            },
            Ok(Event::FileChanged) => reload::reload_preserving_position(
                path,
                &mut document,
                &mut app,
                &layout_doc,
                width,
            ),
            Ok(Event::Resize) => {}
            Err(_) => return Ok(()),
        }
    }
}
