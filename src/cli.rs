use std::path::PathBuf;

use clap::Parser;

/// View a Markdown file in the terminal.
#[derive(Debug, Parser)]
#[command(name = "mdview", version, about)]
pub struct Args {
    /// Path to the Markdown file to view
    pub file: PathBuf,
}
