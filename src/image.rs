use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ratatui::layout::Size;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::Protocol;
use ratatui_image::{FontSize, Resize};

/// The tallest an image is allowed to be. Without a cap, one large
/// picture would fill several screens and bury the text around it.
pub const MAX_ROWS: usize = 15;

/// What the layout pass needs to know about the document's images: how
/// big each one is, and how big the terminal's cells are. Built once per
/// document load, since neither changes as the reader scrolls or resizes.
///
/// `text_only` is the tier from issue #9 — no terminal support, or no
/// picker — where every image is one row of alt-text placeholder.
#[derive(Debug, Clone)]
pub struct Sizing {
    font: Option<FontSize>,
    base: PathBuf,
    pixels: HashMap<String, (u32, u32)>,
}

impl Sizing {
    /// Sizing for a terminal that can't draw images: every image is a
    /// one-row placeholder.
    pub fn text_only() -> Self {
        Self {
            font: None,
            base: PathBuf::new(),
            pixels: HashMap::new(),
        }
    }

    /// Reads the pixel dimensions of each image the document references,
    /// resolved relative to `base` (the document's own directory).
    ///
    /// Only the file header is read, not the whole image — the decode
    /// happens later, and only for images that actually come into view.
    /// An image that can't be read stays out of the map and falls back to
    /// the placeholder.
    pub fn measure<I, S>(base: &Path, font: FontSize, paths: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut pixels = HashMap::new();
        for path in paths {
            let path = path.as_ref();
            if pixels.contains_key(path) {
                continue;
            }
            if let Some(resolved) = resolve(base, path)
                && let Ok(size) = ::image::ImageReader::open(&resolved)
                    .and_then(|reader| reader.with_guessed_format())
                    .map_err(|_| ())
                    .and_then(|reader| reader.into_dimensions().map_err(|_| ()))
            {
                pixels.insert(path.to_string(), size);
            }
        }
        Self {
            font: Some(font),
            base: base.to_path_buf(),
            pixels,
        }
    }

    /// Sizing with pixel dimensions supplied directly, for tests that
    /// shouldn't need image files on disk.
    #[cfg(test)]
    pub(crate) fn with_pixels<I, S>(font: FontSize, sizes: I) -> Self
    where
        I: IntoIterator<Item = (S, (u32, u32))>,
        S: Into<String>,
    {
        Self {
            font: Some(font),
            base: PathBuf::new(),
            pixels: sizes
                .into_iter()
                .map(|(path, size)| (path.into(), size))
                .collect(),
        }
    }

    /// The cell size and pixel size of a drawable image, or `None` when
    /// this one falls back to its alt-text placeholder.
    fn measured(&self, path: &str) -> Option<(FontSize, (u32, u32))> {
        Some((self.font?, *self.pixels.get(path)?))
    }

    /// Whether this image will be drawn, as opposed to standing in as an
    /// alt-text placeholder.
    pub fn draws(&self, path: &str) -> bool {
        self.measured(path).is_some()
    }

    /// How many columns wide the image is drawn, never more than the
    /// pane it sits in.
    pub fn cols_for_path(&self, path: &str, available_cols: usize) -> usize {
        let available_cols = available_cols.max(1);
        match self.measured(path) {
            Some((font, (width_px, _))) if font.width > 0 => {
                let cols = width_px.div_ceil(font.width as u32) as usize;
                cols.clamp(1, available_cols)
            }
            _ => available_cols,
        }
    }

    /// How many rows the layout should reserve for this image.
    pub fn rows_for_path(&self, path: &str, available_cols: usize) -> usize {
        match self.measured(path) {
            Some((font, pixels)) => rows_for(pixels, font, available_cols, MAX_ROWS),
            // The placeholder is a single line of text.
            None => 1,
        }
    }

    /// The file to decode for `path`, or `None` when it isn't a local
    /// file this viewer can open (a URL, say).
    pub fn resolve(&self, path: &str) -> Option<PathBuf> {
        resolve(&self.base, path)
    }
}

/// Asks the terminal what it can draw and how big its cells are.
///
/// Must run after entering the alternate screen but before any input is
/// read — the query writes an escape sequence and reads the reply off
/// stdin. A terminal that doesn't answer still gets half-blocks with a
/// sensible default cell size, since half-blocks are just coloured
/// characters that any 16-colour terminal can show.
///
/// The result is clamped to half-blocks even where a real graphics
/// protocol was detected: Sixel/Kitty/iTerm2 output is issue #11.
pub fn detect_picker() -> Picker {
    let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    picker.set_protocol_type(ProtocolType::Halfblocks);
    picker
}

/// The decoded, terminal-ready form of each image, kept between frames.
///
/// Decoding and protocol-encoding an image is far too slow to redo on
/// every keystroke, and the result depends on the area it was fitted to,
/// so results are cached per (path, area). A failure is cached too — a
/// corrupt file shouldn't be reopened sixty times a second just to fail
/// again.
pub struct Gallery {
    picker: Option<Picker>,
    protocols: HashMap<String, (Size, Option<Protocol>)>,
}

impl Gallery {
    /// A gallery drawing through `picker`, or — with `None` — one that
    /// draws nothing, leaving every image as an alt-text placeholder.
    pub fn new(picker: Option<Picker>) -> Self {
        Self {
            picker,
            protocols: HashMap::new(),
        }
    }

    /// The terminal font size the picker detected, if there is one.
    pub fn font_size(&self) -> Option<FontSize> {
        self.picker.as_ref().map(|picker| picker.font_size())
    }

    /// The drawable form of `path` fitted to `area`, decoding it the
    /// first time it's asked for.
    ///
    /// `None` means the caller should fall back to the alt-text
    /// placeholder: no picker, a path that isn't a local file, or an
    /// image that wouldn't decode.
    pub fn protocol(&mut self, sizing: &Sizing, path: &str, area: Size) -> Option<&Protocol> {
        // One entry per image, replaced when the pane changes shape, so a
        // session of resizing can't pile up stale copies.
        let stale = self
            .protocols
            .get(path)
            .is_none_or(|(fitted, _)| *fitted != area);
        if stale {
            let protocol = self.decode(sizing, path, area);
            self.protocols.insert(path.to_string(), (area, protocol));
        }
        self.protocols.get(path)?.1.as_ref()
    }

    /// Drops every decoded image. Called on reload: the file on disk may
    /// be a different picture now, under the same path.
    pub fn forget_all(&mut self) {
        self.protocols.clear();
    }

    fn decode(&self, sizing: &Sizing, path: &str, area: Size) -> Option<Protocol> {
        let picker = self.picker.as_ref()?;
        let file = sizing.resolve(path)?;
        let image = ::image::ImageReader::open(file)
            .ok()?
            .with_guessed_format()
            .ok()?
            .decode()
            .ok()?;
        picker.new_protocol(image, area, Resize::Fit(None)).ok()
    }
}

/// Resolves a document-relative image path against the document's own
/// directory. Remote references are left to a later tier: nothing here
/// fetches over the network, so they render as placeholders.
fn resolve(base: &Path, path: &str) -> Option<PathBuf> {
    if path.contains("://") {
        return None;
    }
    Some(base.join(path))
}

/// How many terminal rows an image occupies once it's scaled to fit
/// `available_cols`, clamped to at least one row and at most `max_rows`.
///
/// Terminal cells are roughly twice as tall as they are wide, so the row
/// count has to come from the terminal's own font metrics rather than the
/// image's aspect ratio alone.
pub fn rows_for(
    pixels: (u32, u32),
    font: FontSize,
    available_cols: usize,
    max_rows: usize,
) -> usize {
    let (width_px, height_px) = pixels;
    if width_px == 0 || height_px == 0 || font.width == 0 || font.height == 0 {
        return 1;
    }

    let cols = width_px.div_ceil(font.width as u32) as usize;
    let rows = height_px.div_ceil(font.height as u32) as usize;
    let available_cols = available_cols.max(1);
    // Too wide for the pane: shrink the height by the same factor the
    // width has to shrink by, so the picture keeps its proportions.
    let rows = if cols > available_cols {
        (rows * available_cols).div_ceil(cols)
    } else {
        rows
    };
    rows.clamp(1, max_rows.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FONT: FontSize = FontSize::new(10, 20);

    #[test]
    fn an_image_that_fits_keeps_its_natural_cell_height() {
        // 100x200 px at a 10x20 cell is exactly 10 cols by 10 rows.
        assert_eq!(rows_for((100, 200), FONT, 80, 15), 10);
    }

    #[test]
    fn a_wide_image_shrinks_its_height_to_keep_its_proportions() {
        // 800x400 px is 80 cols by 20 rows; half the width means half the
        // height too.
        assert_eq!(rows_for((800, 400), FONT, 40, 15), 10);
    }

    #[test]
    fn a_very_tall_image_is_capped() {
        assert_eq!(rows_for((100, 2000), FONT, 80, 15), 15);
    }

    #[test]
    fn an_image_smaller_than_one_cell_still_gets_a_row() {
        assert_eq!(rows_for((4, 4), FONT, 80, 15), 1);
    }

    #[test]
    fn a_zero_sized_image_gets_a_row_rather_than_dividing_by_zero() {
        assert_eq!(rows_for((0, 0), FONT, 80, 15), 1);
        assert_eq!(rows_for((100, 200), FontSize::new(0, 0), 80, 15), 1);
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mdview-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_png(path: &Path, width: u32, height: u32) {
        ::image::RgbImage::new(width, height).save(path).unwrap();
    }

    #[test]
    fn a_measured_image_reserves_rows_for_its_pixel_size() {
        let dir = scratch_dir("sizing");
        write_png(&dir.join("square.png"), 200, 200);

        let sizing = Sizing::measure(&dir, FONT, ["square.png"]);

        // 200x200 px at a 10x20 cell is 20 cols by 10 rows.
        assert_eq!(sizing.rows_for_path("square.png", 80), 10);
        assert!(sizing.draws("square.png"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_image_that_cannot_be_read_falls_back_to_the_placeholder_row() {
        let dir = scratch_dir("sizing-broken");
        std::fs::write(dir.join("broken.png"), b"this is not an image").unwrap();

        let sizing = Sizing::measure(&dir, FONT, ["broken.png", "missing.png"]);

        for path in ["broken.png", "missing.png"] {
            assert_eq!(sizing.rows_for_path(path, 80), 1, "{path}");
            assert!(!sizing.draws(path), "{path}");
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_remote_image_reference_is_not_something_this_tier_opens() {
        let sizing = Sizing::measure(Path::new("."), FONT, ["https://example.com/logo.png"]);

        assert!(!sizing.draws("https://example.com/logo.png"));
        assert_eq!(sizing.resolve("https://example.com/logo.png"), None);
    }

    #[test]
    fn the_text_only_tier_reserves_one_row_for_every_image() {
        let sizing = Sizing::text_only();

        assert_eq!(sizing.rows_for_path("anything.png", 80), 1);
        assert!(!sizing.draws("anything.png"));
    }

    #[test]
    fn an_image_path_resolves_against_the_documents_own_directory() {
        let sizing = Sizing::measure(Path::new("/docs"), FONT, Vec::<String>::new());

        assert_eq!(
            sizing.resolve("assets/diagram.png"),
            Some(PathBuf::from("/docs").join("assets/diagram.png"))
        );
    }

    #[test]
    fn a_real_image_decodes_into_something_drawable() {
        let dir = scratch_dir("gallery");
        write_png(&dir.join("square.png"), 200, 200);
        let sizing = Sizing::measure(&dir, FONT, ["square.png"]);
        let mut gallery = Gallery::new(Some(Picker::halfblocks()));

        assert!(
            gallery
                .protocol(&sizing, "square.png", Size::new(20, 10))
                .is_some()
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_image_that_will_not_decode_is_not_drawable_and_does_not_panic() {
        let dir = scratch_dir("gallery-broken");
        std::fs::write(dir.join("broken.png"), b"this is not an image").unwrap();
        let sizing = Sizing::measure(&dir, FONT, ["broken.png"]);
        let mut gallery = Gallery::new(Some(Picker::halfblocks()));

        for _ in 0..2 {
            assert!(
                gallery
                    .protocol(&sizing, "broken.png", Size::new(20, 10))
                    .is_none()
            );
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_gallery_without_a_picker_draws_nothing() {
        let dir = scratch_dir("gallery-no-picker");
        write_png(&dir.join("square.png"), 200, 200);
        let sizing = Sizing::measure(&dir, FONT, ["square.png"]);
        let mut gallery = Gallery::new(None);

        assert!(
            gallery
                .protocol(&sizing, "square.png", Size::new(20, 10))
                .is_none()
        );
        assert!(gallery.font_size().is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_reload_forgets_what_was_decoded() {
        let dir = scratch_dir("gallery-forget");
        write_png(&dir.join("square.png"), 200, 200);
        let sizing = Sizing::measure(&dir, FONT, ["square.png"]);
        let mut gallery = Gallery::new(Some(Picker::halfblocks()));
        gallery.protocol(&sizing, "square.png", Size::new(20, 10));

        gallery.forget_all();

        // The file is gone now, so a re-decode has to fail — proving the
        // earlier decode wasn't served from the cache.
        std::fs::remove_file(dir.join("square.png")).unwrap();
        assert!(
            gallery
                .protocol(&sizing, "square.png", Size::new(20, 10))
                .is_none()
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
