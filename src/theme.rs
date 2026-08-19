use ratatui::style::{Color, Modifier, Style};

/// How a heading level should render: its text style, an optional
/// box-drawing rule underneath it, and an indent (in columns) used by the
/// lower levels that don't get a rule of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadingStyle {
    pub style: Style,
    pub rule_style: Option<Style>,
    pub indent: u16,
}

/// The set of styles one run renders with.
///
/// Every colour in the app comes from here, so `--no-color` has exactly
/// one place to be honoured rather than a rule each renderer has to
/// remember. Colours stay inside the ANSI-16 palette (no RGB, no indexed)
/// because those are the only ones a terminal's own theme can remap to
/// suit its background.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Palette {
    /// Legible on a dark background — the default since v0.1.
    #[default]
    Dark,
    /// The same design in colours that survive a light background: no
    /// yellow or cyan text, which washes out on white.
    Light,
    /// `--no-color`: attributes only. Bold, italic and reverse video are
    /// what's left to tell a heading from body text, or the selected
    /// search match from the rest.
    Plain,
}

impl Palette {
    /// Headers must be visually distinct from body text and from each
    /// other without relying on font size, since a terminal can't vary
    /// it.
    ///
    /// Which levels get a rule row, and how far each indents, is the same
    /// in every palette — only the colours differ — so changing palette
    /// can never change how many rows a document occupies.
    pub fn heading_style(self, level: u8) -> HeadingStyle {
        let fg = match (self, level) {
            (Palette::Dark, 1) => Some(Color::Magenta),
            (Palette::Dark, 2) => Some(Color::Cyan),
            (Palette::Dark, 3) => Some(Color::Yellow),
            (Palette::Light, 1) => Some(Color::Blue),
            (Palette::Light, 2) => Some(Color::Magenta),
            (Palette::Light, 3) => Some(Color::Red),
            _ => None,
        };
        // The rule's colour is chosen inside the test for whether there
        // is a rule at all, so a palette can't quietly name a colour for
        // a level that doesn't draw one.
        let rule_style = (level <= 2).then(|| {
            fg_or_plain(match (self, level) {
                (Palette::Dark, 1) => Some(Color::Magenta),
                (Palette::Dark, _) => Some(Color::LightCyan),
                (Palette::Light, 1) => Some(Color::Blue),
                (Palette::Light, _) => Some(Color::Magenta),
                (Palette::Plain, _) => None,
            })
        });

        HeadingStyle {
            style: fg_or_plain(fg).add_modifier(Modifier::BOLD),
            rule_style,
            indent: 2 * u16::from(level.saturating_sub(3)),
        }
    }

    /// Style for a search match that isn't the currently-selected one.
    pub fn search_match_style(self) -> Style {
        match self {
            Palette::Dark => Style::new().bg(Color::Yellow).fg(Color::Black),
            Palette::Light => Style::new().bg(Color::LightYellow).fg(Color::Black),
            Palette::Plain => Style::new().add_modifier(Modifier::REVERSED),
        }
    }

    /// Style for the currently-selected search match — a shade apart from
    /// [`Palette::search_match_style`] so `n`/`N` navigation is visible at
    /// a glance, following the same "one shade distinguishes" idiom as the
    /// H1/H2 heading rules. Which way the shade goes flips with the
    /// background: brighter stands out on dark, darker on light.
    pub fn search_current_match_style(self) -> Style {
        match self {
            Palette::Dark => Style::new()
                .bg(Color::LightYellow)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            Palette::Light => Style::new()
                .bg(Color::Yellow)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            Palette::Plain => Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD),
        }
    }

    /// Styling for an image's alt-text placeholder: dimmed and italic, so
    /// it reads as a stand-in for content rather than as body text.
    pub fn image_placeholder_style(self) -> Style {
        self.dimmed_italic()
    }

    /// Blockquote body — dimmed the same way as a placeholder, since both
    /// are text the reader is meant to register as set apart.
    pub fn blockquote_style(self) -> Style {
        self.dimmed_italic()
    }

    fn dimmed_italic(self) -> Style {
        let fg = (self != Palette::Plain).then_some(Color::DarkGray);
        fg_or_plain(fg).add_modifier(Modifier::ITALIC)
    }

    /// The `[^label]` marker standing in for a footnote reference.
    pub fn footnote_marker_style(self) -> Style {
        let fg = match self {
            Palette::Dark => Some(Color::Cyan),
            Palette::Light => Some(Color::Blue),
            Palette::Plain => None,
        };
        fg_or_plain(fg).add_modifier(Modifier::BOLD)
    }

    /// Code with no language, or a language syntect doesn't know.
    pub fn code_plain_style(self) -> Style {
        fg_or_plain((self != Palette::Plain).then_some(Color::DarkGray))
    }

    /// Which syntect theme highlights fenced code, or `None` to leave it
    /// unstyled. Unlike the rest of the palette these are RGB themes —
    /// syntax highlighting is the one place fine colour distinctions earn
    /// their keep.
    pub fn code_theme(self) -> Option<&'static str> {
        match self {
            Palette::Dark => Some("base16-ocean.dark"),
            Palette::Light => Some("base16-ocean.light"),
            Palette::Plain => None,
        }
    }
}

/// A style with `fg` set, or a bare one when there's no colour to set.
fn fg_or_plain(fg: Option<Color>) -> Style {
    match fg {
        Some(color) => Style::new().fg(color),
        None => Style::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every colour the palette can produce, so a check that applies to
    /// all of them doesn't have to list them at each call site.
    fn all_styles(palette: Palette) -> Vec<Style> {
        let mut styles = vec![
            palette.search_match_style(),
            palette.search_current_match_style(),
            palette.image_placeholder_style(),
            palette.blockquote_style(),
            palette.footnote_marker_style(),
            palette.code_plain_style(),
        ];
        for level in 1..=6 {
            let heading = palette.heading_style(level);
            styles.push(heading.style);
            styles.extend(heading.rule_style);
        }
        styles
    }

    #[test]
    fn h1_is_bold_magenta_with_a_rule() {
        let h1 = Palette::Dark.heading_style(1);
        assert_eq!(h1.style.fg, Some(Color::Magenta));
        assert!(h1.style.add_modifier.contains(Modifier::BOLD));
        assert!(h1.rule_style.is_some());
    }

    #[test]
    fn h2_is_bold_cyan_with_a_lighter_rule_than_h1() {
        let h1 = Palette::Dark.heading_style(1);
        let h2 = Palette::Dark.heading_style(2);
        assert_eq!(h2.style.fg, Some(Color::Cyan));
        assert!(h2.style.add_modifier.contains(Modifier::BOLD));
        assert_ne!(h2.rule_style, h1.rule_style);
    }

    #[test]
    fn h3_is_bold_yellow_with_no_rule() {
        let h3 = Palette::Dark.heading_style(3);
        assert_eq!(h3.style.fg, Some(Color::Yellow));
        assert!(h3.style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(h3.rule_style, None);
    }

    #[test]
    fn h4_through_h6_are_bold_with_no_rule_and_increasing_indent() {
        let h4 = Palette::Dark.heading_style(4);
        let h5 = Palette::Dark.heading_style(5);
        let h6 = Palette::Dark.heading_style(6);

        for h in [h4, h5, h6] {
            assert!(h.style.add_modifier.contains(Modifier::BOLD));
            assert_eq!(h.rule_style, None);
        }
        assert!(h4.indent < h5.indent);
        assert!(h5.indent < h6.indent);
    }

    #[test]
    fn search_match_and_current_match_styles_are_visually_distinct() {
        for palette in [Palette::Dark, Palette::Light, Palette::Plain] {
            assert_ne!(
                palette.search_match_style(),
                palette.search_current_match_style(),
                "{palette:?} can't tell the selected match from the others"
            );
        }
    }

    #[test]
    fn every_palette_stays_within_the_16_color_palette() {
        // The 16-color set excludes Rgb and Indexed variants entirely.
        for palette in [Palette::Dark, Palette::Light, Palette::Plain] {
            for style in all_styles(palette) {
                for color in [style.fg, style.bg] {
                    match color {
                        Some(Color::Rgb(..)) | Some(Color::Indexed(_)) => {
                            panic!("{palette:?} uses a non-16-color Color variant")
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    #[test]
    fn the_plain_palette_carries_no_colour_but_stays_distinguishable() {
        let plain = Palette::Plain;

        for style in all_styles(plain) {
            assert_eq!(style.fg, None, "plain palette set a foreground colour");
            assert_eq!(style.bg, None, "plain palette set a background colour");
        }

        // Attributes are what's left to tell things apart with.
        assert!(
            plain
                .heading_style(1)
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert!(
            plain.heading_style(1).rule_style.is_some(),
            "the rule row has to survive, or --no-color would change how many rows a heading occupies"
        );
        assert_eq!(
            plain.code_theme(),
            None,
            "plain means syntect must not colour code at all"
        );
    }

    #[test]
    fn the_light_palette_avoids_the_colours_that_wash_out_on_white() {
        // Pale foregrounds and light backgrounds both disappear against
        // white. DarkGray is ANSI bright-black, which stays dark — it's
        // what the dimmed styles rely on.
        let washed_out = [
            Color::Yellow,
            Color::LightYellow,
            Color::Cyan,
            Color::LightCyan,
            Color::White,
            Color::Gray,
        ];

        // Every style, not just headings: the dimmed ones (blockquotes,
        // placeholders, unhighlighted code) are the easiest to forget.
        for style in all_styles(Palette::Light) {
            assert!(
                !style.fg.is_some_and(|fg| washed_out.contains(&fg)),
                "a light-palette foreground is {:?}, illegible on white",
                style.fg
            );
        }
    }

    #[test]
    fn light_and_dark_actually_differ() {
        assert_ne!(
            Palette::Light.heading_style(1).style,
            Palette::Dark.heading_style(1).style
        );
        assert_ne!(Palette::Light.code_theme(), Palette::Dark.code_theme());
    }

    #[test]
    fn palettes_agree_on_heading_geometry_so_colour_cannot_move_rows() {
        for level in 1..=6 {
            let dark = Palette::Dark.heading_style(level);
            for other in [
                Palette::Light.heading_style(level),
                Palette::Plain.heading_style(level),
            ] {
                assert_eq!(
                    other.indent, dark.indent,
                    "level {level} indents differently"
                );
                assert_eq!(
                    other.rule_style.is_some(),
                    dark.rule_style.is_some(),
                    "level {level} disagrees about having a rule row"
                );
            }
        }
    }
}
