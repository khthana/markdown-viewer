use std::sync::LazyLock;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme::Palette;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

// Loading these costs tens of ms, and the render loop draws every
// keypress — load once, not per call.
static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

/// Syntax-highlights `text` as `lang` into styled terminal lines, using
/// whichever syntect theme `palette` names.
///
/// Falls back to plain (dimmed, unhighlighted) lines when `lang` is
/// absent or unrecognized, and when the palette names no theme at all —
/// `--no-color`, where highlighting would be nothing but colour. Never
/// errors.
pub fn highlight_code(text: &str, lang: Option<&str>, palette: Palette) -> Vec<Line<'static>> {
    let syntax = lang.and_then(|l| SYNTAX_SET.find_syntax_by_token(l));
    match (syntax, palette.code_theme()) {
        (Some(syntax), Some(theme)) => highlight_with_syntax(text, syntax, theme, palette),
        _ => plain_lines(text, palette),
    }
}

fn highlight_with_syntax(
    text: &str,
    syntax: &syntect::parsing::SyntaxReference,
    theme_name: &str,
    palette: Palette,
) -> Vec<Line<'static>> {
    let theme = &THEME_SET.themes[theme_name];
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut lines = Vec::new();

    for line in LinesWithEndings::from(text) {
        let Ok(segments) = highlighter.highlight_line(line, &SYNTAX_SET) else {
            lines.push(Line::from(Span::styled(
                line.trim_end_matches('\n').to_string(),
                palette.code_plain_style(),
            )));
            continue;
        };
        let spans: Vec<Span<'static>> = segments
            .into_iter()
            .map(|(style, content)| {
                Span::styled(
                    content.trim_end_matches('\n').to_string(),
                    to_ratatui_style(style),
                )
            })
            .collect();
        lines.push(Line::from(spans));
    }

    lines
}

fn plain_lines(text: &str, palette: Palette) -> Vec<Line<'static>> {
    text.lines()
        .map(|line| Line::from(Span::styled(line.to_string(), palette.code_plain_style())))
        .collect()
}

/// Foreground-only: a theme's background fights the terminal's own
/// background and looks wrong in a pager, so it's intentionally dropped.
fn to_ratatui_style(style: syntect::highlighting::Style) -> Style {
    let mut ratatui_style = Style::new().fg(Color::Rgb(
        style.foreground.r,
        style.foreground.g,
        style.foreground.b,
    ));
    if style.font_style.contains(FontStyle::BOLD) {
        ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
    }
    ratatui_style
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn rust_snippet_with_keywords_strings_and_comments_gets_more_than_one_style() {
        let source = "// a comment\nfn main() {\n    let s = \"hello\";\n}\n";
        let lines = highlight_code(source, Some("rust"), Palette::Dark);

        let distinct_styles: HashSet<_> = lines
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.style))
            .collect();

        assert!(
            distinct_styles.len() > 1,
            "expected highlighting to produce more than one distinct style, got {distinct_styles:?}"
        );
    }

    #[test]
    fn no_language_falls_back_to_plain_monospace_lines() {
        let lines = highlight_code("plain text\nmore text", None, Palette::Dark);

        assert_eq!(lines.len(), 2);
        for line in &lines {
            for span in &line.spans {
                assert_eq!(span.style, Palette::Dark.code_plain_style());
            }
        }
        assert_eq!(lines[0].spans[0].content, "plain text");
        assert_eq!(lines[1].spans[0].content, "more text");
    }

    #[test]
    fn unrecognized_language_falls_back_to_plain_monospace_lines_without_erroring() {
        let lines = highlight_code("some text", Some("not-a-real-language"), Palette::Dark);

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans[0].style, Palette::Dark.code_plain_style());
        assert_eq!(lines[0].spans[0].content, "some text");
    }

    #[test]
    fn every_colour_palette_names_a_theme_syntect_actually_ships() {
        for palette in [Palette::Dark, Palette::Light] {
            let name = palette
                .code_theme()
                .expect("a colour palette highlights code");
            assert!(
                THEME_SET.themes.contains_key(name),
                "{palette:?} names {name:?}, which syntect's defaults don't include"
            );
        }
    }

    #[test]
    fn a_colourless_palette_leaves_even_known_languages_unhighlighted() {
        let lines = highlight_code("fn main() {}", Some("rust"), Palette::Plain);

        for line in &lines {
            for span in &line.spans {
                assert_eq!(span.style.fg, None, "code was coloured under --no-color");
            }
        }
        assert_eq!(lines.len(), 1, "dropping colour must not drop rows");
    }
}
