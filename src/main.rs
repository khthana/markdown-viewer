mod app;
mod cli;
mod event;
mod markdown;
mod theme;
mod ui;

use anyhow::Context;
use clap::Parser;

use app::App;
use event::Event;
use markdown::blocks::Block;
use markdown::layout;

fn main() -> anyhow::Result<()> {
    let args = cli::Args::parse();

    let content = std::fs::read_to_string(&args.file)
        .with_context(|| format!("could not read file: {}", args.file.display()))?;
    let blocks = markdown::blocks::lower(&content);

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, blocks);
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal, blocks: Vec<Block>) -> anyhow::Result<()> {
    let mut app = App::new(0);
    let rx = event::spawn_crossterm_source();

    loop {
        let size = terminal.size()?;
        app.viewport_height = size.height as usize;
        app.total_rows = layout::layout(&blocks, size.width as usize).total_rows;
        terminal.draw(|frame| ui::render(frame, &app, &blocks))?;

        match rx.recv() {
            Ok(Event::Key(key)) => {
                if app.on_key(key) {
                    return Ok(());
                }
            }
            Ok(Event::Resize) => {}
            Err(_) => return Ok(()),
        }
    }
}
