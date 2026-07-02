use pulldown_cmark::{Options, Parser};

/// Parses `source` as CommonMark plus GFM tables, task lists,
/// strikethrough, and footnotes.
pub fn parse(source: &str) -> Parser<'_> {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES;
    Parser::new_ext(source, options)
}
