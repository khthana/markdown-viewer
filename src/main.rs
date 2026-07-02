mod app;
mod cli;
mod event;
mod highlight;
mod markdown;
mod theme;
mod toc;
mod ui;

use anyhow::Context;
use clap::Parser;
use ratatui::layout::Rect;

use app::App;
use event::Event;
use markdown::blocks::{Block, HeadingRef};
use markdown::layout;

fn main() -> anyhow::Result<()> {
    let args = cli::Args::parse();

    let content = std::fs::read_to_string(&args.file)
        .with_context(|| format!("could not read file: {}", args.file.display()))?;
    let (blocks, headings) = markdown::blocks::lower_with_headings(&content);

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, blocks, headings);
    ratatui::restore();
    result
}

fn run(
    terminal: &mut ratatui::DefaultTerminal,
    blocks: Vec<Block>,
    headings: Vec<HeadingRef>,
) -> anyhow::Result<()> {
    let mut app = App::new(0);
    let rx = event::spawn_crossterm_source();

    loop {
        let size = terminal.size()?;
        let area = Rect::new(0, 0, size.width, size.height);
        let (_, main_area) = ui::split_areas(area, app.toc_open);

        app.viewport_height = main_area.height as usize;
        let layout_doc = layout::layout(&blocks, main_area.width as usize);
        app.total_rows = layout_doc.total_rows;
        let toc_entries = toc::resolve(&headings, &layout_doc);

        terminal.draw(|frame| ui::render(frame, &app, &blocks, &toc_entries))?;

        match rx.recv() {
            Ok(Event::Key(key)) => {
                if app.on_key(key, &toc_entries) {
                    return Ok(());
                }
            }
            Ok(Event::Resize) => {}
            Err(_) => return Ok(()),
        }
    }
}
