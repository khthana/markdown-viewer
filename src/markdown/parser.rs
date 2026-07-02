use pulldown_cmark::{Options, Parser};

/// Parses `source` as CommonMark (no GFM extensions yet).
pub fn parse(source: &str) -> Parser<'_> {
    Parser::new_ext(source, Options::empty())
}
