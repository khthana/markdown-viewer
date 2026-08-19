use ratatui::style::{Color, Modifier, Style};

/// How a heading level should render: its text style, an optional
/// box-drawing rule color underneath it, and an indent (in columns) used
/// by the lower levels that don't get a rule of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadingStyle {
    pub style: Style,
    pub rule_color: Option<Color>,
    pub indent: u16,
}

/// Hardcoded ANSI-16 palette: headers must be visually distinct from body
/// text and from each other without relying on font size, since a
/// terminal can't vary font size. Restricted to the 16-color palette (not
/// RGB/256-color) so it stays legible on both light and dark terminal
/// backgrounds.
pub fn heading_style(level: u8) -> HeadingStyle {
    let bold = Modifier::BOLD;
    match level {
        1 => HeadingStyle {
            style: Style::new().fg(Color::Magenta).add_modifier(bold),
            rule_color: Some(Color::Magenta),
            indent: 0,
        },
        2 => HeadingStyle {
            style: Style::new().fg(Color::Cyan).add_modifier(bold),
            rule_color: Some(Color::LightCyan),
            indent: 0,
        },
        3 => HeadingStyle {
            style: Style::new().fg(Color::Yellow).add_modifier(bold),
            rule_color: None,
            indent: 0,
        },
        _ => HeadingStyle {
            style: Style::new().add_modifier(bold),
            rule_color: None,
            indent: 2 * (level.saturating_sub(3)) as u16,
        },
    }
}

/// Style for a search match that isn't the currently-selected one.
pub fn search_match_style() -> Style {
    Style::new().bg(Color::Yellow).fg(Color::Black)
}

/// Style for the currently-selected search match — a brighter background
/// than [`search_match_style`] so `n`/`N` navigation is visible at a
/// glance, following the same "lighter shade distinguishes" idiom as the
/// H1/H2 heading rules above.
pub fn search_current_match_style() -> Style {
    Style::new()
        .bg(Color::LightYellow)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD)
}

/// Styling for an image's alt-text placeholder: dimmed and italic, so it
/// reads as a stand-in for content rather than as body text.
pub fn image_placeholder_style() -> Style {
    Style::new()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::ITALIC)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h1_is_bold_magenta_with_a_rule() {
        let h1 = heading_style(1);
        assert_eq!(h1.style.fg, Some(Color::Magenta));
        assert!(h1.style.add_modifier.contains(Modifier::BOLD));
        assert!(h1.rule_color.is_some());
    }

    #[test]
    fn h2_is_bold_cyan_with_a_lighter_rule_than_h1() {
        let h1 = heading_style(1);
        let h2 = heading_style(2);
        assert_eq!(h2.style.fg, Some(Color::Cyan));
        assert!(h2.style.add_modifier.contains(Modifier::BOLD));
        assert_ne!(h2.rule_color, h1.rule_color);
    }

    #[test]
    fn h3_is_bold_yellow_with_no_rule() {
        let h3 = heading_style(3);
        assert_eq!(h3.style.fg, Some(Color::Yellow));
        assert!(h3.style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(h3.rule_color, None);
    }

    #[test]
    fn h4_through_h6_are_bold_with_no_rule_and_increasing_indent() {
        let h4 = heading_style(4);
        let h5 = heading_style(5);
        let h6 = heading_style(6);

        for h in [h4, h5, h6] {
            assert!(h.style.add_modifier.contains(Modifier::BOLD));
            assert_eq!(h.rule_color, None);
        }
        assert!(h4.indent < h5.indent);
        assert!(h5.indent < h6.indent);
    }

    #[test]
    fn search_match_and_current_match_styles_are_visually_distinct() {
        assert_ne!(search_match_style(), search_current_match_style());
    }

    #[test]
    fn search_highlight_styles_stay_within_the_16_color_palette() {
        for style in [search_match_style(), search_current_match_style()] {
            for color in [style.fg, style.bg] {
                match color {
                    Some(Color::Rgb(..)) | Some(Color::Indexed(_)) => {
                        panic!("search highlight uses a non-16-color Color variant")
                    }
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn every_level_stays_within_the_16_color_palette() {
        // The 16-color set excludes Rgb and Indexed variants entirely.
        for level in 1..=6 {
            match heading_style(level).style.fg {
                Some(Color::Rgb(..)) | Some(Color::Indexed(_)) => {
                    panic!("level {level} uses a non-16-color Color variant")
                }
                _ => {}
            }
        }
    }
}
