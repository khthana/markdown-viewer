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

use std::io::IsTerminal;
use std::path::Path;
use std::process::ExitCode;

use clap::Parser;
use ratatui::layout::Rect;

use app::{App, KeyOutcome};
use event::{Event, PendingSources, Sources};
use image::Gallery;
use markdown::layout;
use reload::Document;
use theme::Palette;

fn main() -> ExitCode {
    let args = cli::Args::parse();
    let rendering = args.rendering(cli::no_color_env());

    // Checked before anything is drawn: a bad path should print one
    // plain line, not flash an empty pager and explain itself afterwards.
    if let Err(problem) = reload::check_readable(&args.file) {
        eprintln!("mdview: {problem}");
        return ExitCode::FAILURE;
    }

    // A pager needs a terminal to page. Redirected output gets the
    // document as plain text instead — which is what `--no-color`'s
    // "suitable for piping" has to mean for a full-screen viewer.
    if !std::io::stdout().is_terminal() {
        return match dump(&args.file) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("mdview: {error}");
                ExitCode::FAILURE
            }
        };
    }

    // Started before the alternate screen so a watcher warning is visible.
    let sources = PendingSources::watching(&args.file);

    let mut terminal = ratatui::init();
    // The capability query writes to the terminal and reads the reply off
    // stdin, so it has to run inside the alternate screen but before the
    // input thread starts taking stdin for itself.
    let gallery = if rendering.draw_images {
        let picker = image::detect_picker();
        Gallery::new(
            picker.clone(),
            image::spawn_worker(picker, sources.sender()),
        )
    } else {
        Gallery::disabled()
    };
    let sources = sources.start_input();

    let result = reload::load(&args.file, gallery.font_size()).and_then(|document| {
        run(
            &mut terminal,
            &args.file,
            document,
            &sources,
            rendering.palette,
            gallery,
        )
    });
    ratatui::restore();

    match result {
        Ok(()) => ExitCode::SUCCESS,
        // Displayed, not debug-printed: the message is already written
        // for the reader, and a `Caused by:` chain would bury it.
        Err(error) => {
            eprintln!("mdview: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Writes the document to stdout as plain text, one row per line, for
/// when there's no terminal to page in.
///
/// Reuses the pager's own layout so what a pipe receives is what the
/// screen would have shown, minus the styling: `LayoutDoc::rows` is the
/// text of each rendered row with the styles already stripped. Which
/// palette laid it out therefore can't matter — every palette lays out
/// identically, and none of the colour survives `rows` — so this asks
/// for the colourless one and takes no palette of its own. Images are
/// always alt text here: there's nothing to draw a picture into.
fn dump(path: &Path) -> anyhow::Result<()> {
    const PIPE_WIDTH: usize = 80;

    let document = reload::load(path, None)?;
    let laid_out = layout::layout(
        &document.blocks,
        PIPE_WIDTH,
        &document.image_sizing,
        Palette::Plain,
    );
    for row in &laid_out.rows {
        println!("{row}");
    }
    Ok(())
}

fn run(
    terminal: &mut ratatui::DefaultTerminal,
    path: &Path,
    mut document: Document,
    sources: &Sources,
    palette: Palette,
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
        let layout_doc = layout::layout(&document.blocks, width, &document.image_sizing, palette);
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
                    palette,
                },
                &mut gallery,
            )
        })?;
        // Drawing is what discovers an image needs re-encoding for its
        // area; the work itself goes to the image thread.
        gallery.dispatch_resizes();

        let outcome = match sources.recv() {
            Ok(Event::Key(key)) => app.on_key(key, &toc_entries, &matches),
            // A change on disk asks for exactly the work `r` does.
            Ok(Event::FileChanged) => KeyOutcome::Reload,
            Ok(Event::Resize) => KeyOutcome::Continue,
            Ok(Event::ImageReady { block_id, protocol }) => {
                gallery.image_decoded(block_id, protocol.map(|protocol| *protocol));
                KeyOutcome::Continue
            }
            Ok(Event::ImageResized { block_id, response }) => {
                gallery.image_resized(block_id, *response);
                KeyOutcome::Continue
            }
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
                    palette,
                    &mut gallery,
                );
            }
            KeyOutcome::Continue => {}
        }
    }
}
