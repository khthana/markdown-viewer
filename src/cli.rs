use std::ffi::OsStr;
use std::path::PathBuf;

use clap::{Parser, ValueEnum};

use crate::theme::Palette;

/// Which palette to render with. The default matches the palette every
/// version so far has used, so an existing invocation keeps its colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum ThemeChoice {
    #[default]
    Dark,
    Light,
}

/// View a Markdown file in the terminal.
#[derive(Debug, Parser)]
#[command(name = "mdview", version, about)]
pub struct Args {
    /// Path to the Markdown file to view
    pub file: PathBuf,

    /// Render without colour: plain text plus bold/italic/reverse only
    #[arg(long)]
    pub no_color: bool,

    /// Show every image as its alt-text placeholder instead of drawing it
    #[arg(long)]
    pub no_images: bool,

    /// Palette to render with
    #[arg(long, value_enum, default_value_t = ThemeChoice::Dark)]
    pub theme: ThemeChoice,
}

/// What the flags add up to: the two rendering decisions the whole app
/// reads, resolved in one place so the `--no-color` policy has a single
/// home rather than one rule per renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rendering {
    pub palette: Palette,
    /// Whether pictures are drawn at all, as opposed to standing in as
    /// alt text.
    pub draw_images: bool,
}

impl Args {
    /// Resolves the flags, with `no_color_env` folded in — see
    /// [`no_color_env`].
    ///
    /// `--no-color` beats `--theme`, because a palette that may not use
    /// colour has nothing left for light-vs-dark to vary. It also implies
    /// `--no-images`: a half-block picture is nothing but coloured cells
    /// and a protocol one isn't text at all, so honouring one flag and
    /// not the other would be a lie.
    pub fn rendering(&self, no_color_env: bool) -> Rendering {
        let no_color = self.no_color || no_color_env;
        Rendering {
            palette: match (no_color, self.theme) {
                (true, _) => Palette::Plain,
                (false, ThemeChoice::Dark) => Palette::Dark,
                (false, ThemeChoice::Light) => Palette::Light,
            },
            draw_images: !self.no_images && !no_color,
        }
    }
}

/// Whether the environment asks for no colour, by the `NO_COLOR`
/// convention: the variable set to anything non-empty.
pub fn no_color_env() -> bool {
    no_color_from(std::env::var_os("NO_COLOR").as_deref())
}

fn no_color_from(value: Option<&OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;

    use super::*;

    fn parse(argv: &[&str]) -> Args {
        Args::try_parse_from(argv).expect("expected these arguments to parse")
    }

    #[test]
    fn a_bare_file_argument_leaves_every_flag_at_its_default() {
        let args = parse(&["mdview", "doc.md"]);

        assert_eq!(args.file, PathBuf::from("doc.md"));
        assert!(!args.no_color);
        assert!(!args.no_images);
        assert_eq!(args.theme, ThemeChoice::Dark);
    }

    #[test]
    fn every_flag_combination_parses_independently() {
        let both = parse(&["mdview", "doc.md", "--no-color", "--no-images"]);
        assert!(both.no_color && both.no_images);

        let colour_only = parse(&["mdview", "doc.md", "--no-images"]);
        assert!(!colour_only.no_color && colour_only.no_images);

        let images_only = parse(&["mdview", "doc.md", "--no-color"]);
        assert!(images_only.no_color && !images_only.no_images);
    }

    #[test]
    fn theme_takes_dark_or_light_and_rejects_anything_else() {
        assert_eq!(
            parse(&["mdview", "doc.md", "--theme", "light"]).theme,
            ThemeChoice::Light
        );
        assert_eq!(
            parse(&["mdview", "doc.md", "--theme", "dark"]).theme,
            ThemeChoice::Dark
        );

        let rejected = Args::try_parse_from(["mdview", "doc.md", "--theme", "solarized"])
            .expect_err("an unknown theme should be rejected, not silently defaulted");
        assert_eq!(rejected.kind(), ErrorKind::InvalidValue);
    }

    #[test]
    fn the_flags_compose_with_a_theme_and_the_file_stays_positional() {
        let args = parse(&[
            "mdview",
            "--no-color",
            "--theme",
            "light",
            "--no-images",
            "notes/doc.md",
        ]);

        assert_eq!(args.file, PathBuf::from("notes/doc.md"));
        assert!(args.no_color && args.no_images);
        assert_eq!(args.theme, ThemeChoice::Light);
    }

    #[test]
    fn help_lists_the_file_argument_and_every_flag() {
        let help = Args::try_parse_from(["mdview", "--help"])
            .expect_err("--help exits by reporting the help text as an error")
            .to_string();

        for expected in ["FILE", "--no-color", "--no-images", "--theme", "--version"] {
            assert!(
                help.contains(expected),
                "--help omitted {expected}:
{help}"
            );
        }
    }

    #[test]
    fn version_reports_the_crate_version() {
        let version = Args::try_parse_from(["mdview", "--version"])
            .expect_err("--version exits by reporting the version as an error")
            .to_string();

        assert!(
            version.contains(env!("CARGO_PKG_VERSION")),
            "--version printed {version:?}"
        );
    }

    #[test]
    fn a_missing_file_argument_is_an_error_rather_than_a_default() {
        let rejected = Args::try_parse_from(["mdview"]).expect_err("the file argument is required");
        assert_eq!(rejected.kind(), ErrorKind::MissingRequiredArgument);
    }

    fn rendering(argv: &[&str]) -> Rendering {
        parse(argv).rendering(false)
    }

    #[test]
    fn no_images_and_no_color_both_drop_to_the_alt_text_tier() {
        assert!(rendering(&["mdview", "doc.md"]).draw_images);
        assert!(!rendering(&["mdview", "doc.md", "--no-images"]).draw_images);
        assert!(
            !rendering(&["mdview", "doc.md", "--no-color"]).draw_images,
            "a half-block picture is nothing but colour, so --no-color has to drop it too"
        );
    }

    #[test]
    fn no_color_overrides_the_theme_choice() {
        assert_eq!(rendering(&["mdview", "doc.md"]).palette, Palette::Dark);
        assert_eq!(
            rendering(&["mdview", "doc.md", "--theme", "light"]).palette,
            Palette::Light
        );
        assert_eq!(
            rendering(&["mdview", "doc.md", "--no-color", "--theme", "light"]).palette,
            Palette::Plain
        );
    }

    #[test]
    fn the_no_color_environment_variable_is_honoured_like_the_flag() {
        let from_env = parse(&["mdview", "doc.md", "--theme", "light"]).rendering(true);

        assert_eq!(from_env.palette, Palette::Plain);
        assert!(!from_env.draw_images);
    }

    #[test]
    fn an_empty_no_color_does_not_count_as_set() {
        // The NO_COLOR convention: presence isn't enough, it has to have
        // a value, so `NO_COLOR=` can turn it back off.
        assert!(!no_color_from(None));
        assert!(!no_color_from(Some(OsStr::new(""))));
        assert!(no_color_from(Some(OsStr::new("1"))));
        assert!(no_color_from(Some(OsStr::new("anything"))));
    }
}
