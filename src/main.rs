mod app;
mod cli;
mod event;
mod highlight;
mod image;
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
use event::{Event, PendingSources, Sources};
use image::Gallery;
use markdown::layout;
use reload::Document;

fn main() -> anyhow::Result<()> {
    let args = cli::Args::parse();
    // Started before the alternate screen so a watcher warning is visible.
    let sources = PendingSources::watching(&args.file);

    let mut terminal = ratatui::init();
    // The capability query writes to the terminal and reads the reply off
    // stdin, so it has to run inside the alternate screen but before the
    // input thread starts taking stdin for itself.
    let gallery = Gallery::new(Some(image::detect_picker()));
    let sources = sources.start_input();

    let result = reload::load(&args.file, gallery.font_size())
        .and_then(|document| run(&mut terminal, &args.file, document, &sources, gallery));
    ratatui::restore();
    result
}

fn run(
    terminal: &mut ratatui::DefaultTerminal,
    path: &Path,
    mut document: Document,
    sources: &Sources,
    mut gallery: Gallery,
) -> anyhow::Result<()> {
    let mut app = App::new(0);

    loop {
        let size = terminal.size()?;
        let area = Rect::new(0, 0, size.width, size.height);
        let (_, main_area) = ui::split_areas(area, app.toc_open);
        let (content_area, _) = ui::split_status(main_area, ui::search_status_visible(&app));

        app.viewport_height = content_area.height as usize;
        let width = main_area.width as usize;
        let layout_doc = layout::layout(&document.blocks, width, &document.image_sizing);
        app.total_rows = layout_doc.total_rows;
        let toc_entries = toc::resolve(&document.headings, &layout_doc);
        let matches = search::search(&app.search_query, &layout_doc);

        terminal.draw(|frame| {
            ui::render(
                frame,
                ui::Screen {
                    app: &app,
                    blocks: &document.blocks,
                    layout_doc: &layout_doc,
                    toc: &toc_entries,
                    matches: &matches,
                    image_sizing: &document.image_sizing,
                },
                &mut gallery,
            )
        })?;

        let outcome = match sources.recv() {
            Ok(Event::Key(key)) => app.on_key(key, &toc_entries, &matches),
            // A change on disk asks for exactly the work `r` does.
            Ok(Event::FileChanged) => KeyOutcome::Reload,
            Ok(Event::Resize) => KeyOutcome::Continue,
            Err(_) => return Ok(()),
        };

        match outcome {
            KeyOutcome::Quit => return Ok(()),
            KeyOutcome::Reload => {
                reload::reload_preserving_position(
                    path,
                    &mut document,
                    &mut app,
                    &layout_doc,
                    width,
                    &mut gallery,
                );
            }
            KeyOutcome::Continue => {}
        }
    }
}
