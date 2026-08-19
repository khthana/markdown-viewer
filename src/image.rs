use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use ratatui::layout::Size;
use ratatui_image::FontSize;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::thread::{ResizeRequest, ResizeResponse, ThreadProtocol};
use ratatui_image::{Resize, ResizeEncodeRender};

use crate::event::Event;

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

/// Reads and decodes an image into the terminal's own protocol, ready to
/// be sized. This is the slow part — file I/O plus a full decode — and
/// the reason it runs on a worker thread rather than in the render loop.
///
/// `None` for anything that won't open or won't decode: the caller shows
/// the alt-text placeholder and doesn't ask again.
pub fn decode_protocol(picker: &Picker, file: &Path) -> Option<StatefulProtocol> {
    let image = ::image::ImageReader::open(file)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    Some(picker.new_resize_protocol(image))
}

/// The protocol named by `MDVIEW_PROTOCOL`, for trying a tier the
/// terminal wasn't detected as supporting — or, more often, for checking
/// that the fallback tiers still look right on a terminal that supports
/// everything. An unrecognised value is ignored rather than fatal.
fn parse_protocol_override(value: &str) -> Option<ProtocolType> {
    match value.trim().to_ascii_lowercase().as_str() {
        "halfblocks" | "halfblock" => Some(ProtocolType::Halfblocks),
        "sixel" => Some(ProtocolType::Sixel),
        "kitty" => Some(ProtocolType::Kitty),
        "iterm2" => Some(ProtocolType::Iterm2),
        _ => None,
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
/// `MDVIEW_PROTOCOL` overrides whatever was detected, for testing a tier
/// on a terminal that would otherwise pick a different one.
pub fn detect_picker() -> Picker {
    let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    if let Some(forced) = std::env::var("MDVIEW_PROTOCOL")
        .ok()
        .as_deref()
        .and_then(parse_protocol_override)
    {
        picker.set_protocol_type(forced);
    }
    picker
}

/// How a picture is fitted to the rows reserved for it. Shared between
/// the widget that renders it and the check for whether it's ready to be
/// rendered, which have to agree or the check is meaningless.
pub const IMAGE_RESIZE: Resize = Resize::Fit(None);

/// Identifies one image block in one version of the document.
///
/// The generation is what makes a reply from before a reload
/// recognisably stale: block 3 of the old document is not block 3 of the
/// new one, and a picture decoded for the old one must not be painted
/// over the new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageId {
    generation: u64,
    block: usize,
}

/// Work for the image thread. Both kinds are slow enough to be worth
/// keeping out of the render loop: a decode reads and unpacks a file, a
/// resize re-encodes the picture for a new area.
pub enum Job {
    Decode {
        id: ImageId,
        file: PathBuf,
    },
    /// Boxed because a resize request carries the whole decoded image,
    /// which would otherwise make every queued job that size.
    Resize {
        id: ImageId,
        request: Box<ResizeRequest>,
    },
}

/// Runs the image thread, answering every job on `events`.
pub fn spawn_worker(picker: Picker, events: Sender<Event>) -> Sender<Job> {
    let (sender, jobs) = mpsc::channel::<Job>();
    thread::spawn(move || {
        for job in jobs {
            let answer = match job {
                Job::Decode { id, file } => Event::ImageReady {
                    block_id: id,
                    protocol: decode_protocol(&picker, &file).map(Box::new),
                },
                Job::Resize { id, request } => match request.resize_encode() {
                    Ok(response) => Event::ImageResized {
                        block_id: id,
                        response: Box::new(response),
                    },
                    // An image that won't re-encode is as good as one that
                    // wouldn't decode: back to the placeholder.
                    Err(_) => Event::ImageReady {
                        block_id: id,
                        protocol: None,
                    },
                },
            };
            if events.send(answer).is_err() {
                break;
            }
        }
    });
    sender
}

/// A gallery wired to a channel the caller reads from, standing in for
/// the worker thread. Shared with `ui`'s tests, which play that part.
#[cfg(test)]
pub(crate) fn test_gallery() -> (Gallery, Receiver<Job>) {
    let (sender, jobs) = mpsc::channel();
    (Gallery::new(Picker::halfblocks(), sender), jobs)
}

/// What the render pass knows about one image.
enum Slot {
    /// Asked for; the worker hasn't answered yet.
    Pending,
    /// Decoded and drawable. `outbox` is where the protocol posts its own
    /// resize requests during rendering, which [`Gallery::dispatch_resizes`]
    /// forwards to the worker.
    Ready {
        protocol: Box<ThreadProtocol>,
        outbox: Receiver<ResizeRequest>,
    },
    /// Won't decode. Never asked for again.
    Failed,
}

/// Every image the reader has scrolled to, in whatever state the worker
/// has got it to.
///
/// The render loop only ever *asks*; nothing here blocks on a decode.
/// Until an answer arrives the caller draws the alt-text placeholder, so
/// a document full of pictures opens as fast as one without.
pub struct Gallery {
    backend: Backend,
    generation: u64,
    slots: HashMap<ImageId, Slot>,
}

/// A gallery either has both a picker and a worker to talk to, or it
/// draws nothing at all. Keeping them in one enum means there's no state
/// where one is present and the other isn't.
enum Backend {
    /// Only tests build one today; issue #12's `--no-images` is what
    /// makes this reachable in production.
    #[allow(dead_code)]
    Disabled,
    Live {
        picker: Picker,
        jobs: Sender<Job>,
    },
}

impl Gallery {
    /// A gallery that draws nothing — the alt-text tier. Only tests need
    /// it until issue #12 adds `--no-images`.
    #[cfg(test)]
    pub(crate) fn disabled() -> Self {
        Self {
            backend: Backend::Disabled,
            generation: 0,
            slots: HashMap::new(),
        }
    }

    /// A gallery drawing through `picker`, with `jobs` going to the image
    /// thread.
    pub fn new(picker: Picker, jobs: Sender<Job>) -> Self {
        Self {
            backend: Backend::Live { picker, jobs },
            generation: 0,
            slots: HashMap::new(),
        }
    }

    /// The terminal font size the picker detected, if there is one.
    pub fn font_size(&self) -> Option<FontSize> {
        match &self.backend {
            Backend::Live { picker, .. } => Some(picker.font_size()),
            Backend::Disabled => None,
        }
    }

    /// The identity of the image in top-level block `block`, as of the
    /// document currently loaded.
    pub fn id_for(&self, block: usize) -> ImageId {
        ImageId {
            generation: self.generation,
            block,
        }
    }

    /// Asks the worker for this image, unless it has been asked for
    /// already — a request per frame would queue a decode per keystroke.
    pub fn request(&mut self, id: ImageId, sizing: &Sizing, path: &str) {
        if self.slots.contains_key(&id) {
            return;
        }
        let Backend::Live { jobs, .. } = &self.backend else {
            return;
        };
        let Some(file) = sizing.resolve(path) else {
            self.slots.insert(id, Slot::Failed);
            return;
        };
        if jobs.send(Job::Decode { id, file }).is_ok() {
            self.slots.insert(id, Slot::Pending);
        }
    }

    /// Whether this image will actually paint into `area` on the next
    /// frame, as opposed to posting an encode job and leaving the rows
    /// untouched.
    ///
    /// Rendering a protocol that still needs encoding draws nothing —
    /// it hands the picture to the worker instead — so the caller has to
    /// know the difference to keep a placeholder in those rows rather
    /// than a hole.
    pub fn paints_now(&self, id: ImageId, area: Size) -> bool {
        match self.slots.get(&id) {
            Some(Slot::Ready { protocol, .. }) => {
                protocol.protocol_type().is_some()
                    && protocol.needs_resize(&IMAGE_RESIZE, area).is_none()
            }
            _ => false,
        }
    }

    /// The drawable state of an image, or `None` while it's still being
    /// decoded, or if it never will be.
    pub fn protocol_mut(&mut self, id: ImageId) -> Option<&mut ThreadProtocol> {
        match self.slots.get_mut(&id) {
            Some(Slot::Ready { protocol, .. }) => Some(protocol),
            _ => None,
        }
    }

    /// Takes the worker's answer to a decode. `None` means the file
    /// wouldn't decode.
    pub fn image_decoded(&mut self, id: ImageId, protocol: Option<StatefulProtocol>) {
        if id.generation != self.generation {
            return;
        }
        let slot = match protocol {
            Some(protocol) => {
                let (sender, outbox) = mpsc::channel();
                Slot::Ready {
                    protocol: Box::new(ThreadProtocol::new(sender, Some(protocol))),
                    outbox,
                }
            }
            None => Slot::Failed,
        };
        self.slots.insert(id, slot);
    }

    /// Takes the worker's answer to a resize.
    pub fn image_resized(&mut self, id: ImageId, response: ResizeResponse) {
        if id.generation != self.generation {
            return;
        }
        if let Some(Slot::Ready { protocol, .. }) = self.slots.get_mut(&id) {
            // Discards the answer if the protocol has since asked for a
            // different size — the newer request is the one that counts.
            protocol.update_resized_protocol(response);
        }
    }

    /// Hands the worker every resize the last render asked for. Called
    /// after drawing, since that's when the protocols post them.
    pub fn dispatch_resizes(&mut self) {
        let Backend::Live { jobs, .. } = &self.backend else {
            return;
        };
        // A request carries the only copy of its decoded image — the
        // protocol handed it over — so one that can't be delivered has
        // lost the picture for good. Those slots fall back to the
        // placeholder instead of staying blank forever.
        let mut undeliverable = Vec::new();
        for (id, slot) in &self.slots {
            if let Slot::Ready { outbox, .. } = slot {
                while let Ok(request) = outbox.try_recv() {
                    let request = Box::new(request);
                    if jobs.send(Job::Resize { id: *id, request }).is_err() {
                        undeliverable.push(*id);
                        break;
                    }
                }
            }
        }
        for id in undeliverable {
            self.slots.insert(id, Slot::Failed);
        }
    }

    /// Drops every decoded image and moves to a new generation, so
    /// answers still in flight for the old document are ignored. Called
    /// on reload: the same path may be a different picture now.
    pub fn forget_all(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.slots.clear();
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

    fn decoded(picker_file: &Path) -> StatefulProtocol {
        decode_protocol(&Picker::halfblocks(), picker_file).expect("a decodable image")
    }

    #[test]
    fn an_image_is_asked_for_once_however_many_frames_go_by() {
        let dir = scratch_dir("gallery-request");
        write_png(&dir.join("square.png"), 200, 200);
        let sizing = Sizing::measure(&dir, FONT, ["square.png"]);
        let (mut gallery, jobs) = test_gallery();
        let id = gallery.id_for(0);

        for _ in 0..3 {
            gallery.request(id, &sizing, "square.png");
        }

        assert!(matches!(jobs.try_recv(), Ok(Job::Decode { .. })));
        assert!(jobs.try_recv().is_err(), "one decode, not one per frame");
        assert!(
            gallery.protocol_mut(id).is_none(),
            "nothing to draw until the worker answers"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_decoded_image_becomes_drawable() {
        let dir = scratch_dir("gallery-decoded");
        write_png(&dir.join("square.png"), 200, 200);
        let sizing = Sizing::measure(&dir, FONT, ["square.png"]);
        let (mut gallery, _jobs) = test_gallery();
        let id = gallery.id_for(0);
        gallery.request(id, &sizing, "square.png");

        gallery.image_decoded(id, Some(decoded(&dir.join("square.png"))));

        assert!(gallery.protocol_mut(id).is_some());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_image_that_would_not_decode_is_never_asked_for_again() {
        let dir = scratch_dir("gallery-failed");
        std::fs::write(dir.join("broken.png"), b"this is not an image").unwrap();
        let sizing = Sizing::measure(&dir, FONT, ["broken.png"]);
        let (mut gallery, jobs) = test_gallery();
        let id = gallery.id_for(0);

        gallery.request(id, &sizing, "broken.png");
        let _ = jobs.try_recv();
        gallery.image_decoded(id, None);
        gallery.request(id, &sizing, "broken.png");

        assert!(jobs.try_recv().is_err(), "a failure isn't retried");
        assert!(gallery.protocol_mut(id).is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_remote_reference_is_not_worth_a_job() {
        let sizing = Sizing::measure(Path::new("."), FONT, ["https://example.com/logo.png"]);
        let (mut gallery, jobs) = test_gallery();
        let id = gallery.id_for(0);

        gallery.request(id, &sizing, "https://example.com/logo.png");

        assert!(jobs.try_recv().is_err());
        assert!(gallery.protocol_mut(id).is_none());
    }

    #[test]
    fn an_answer_from_before_a_reload_is_ignored() {
        let dir = scratch_dir("gallery-stale");
        write_png(&dir.join("square.png"), 200, 200);
        let sizing = Sizing::measure(&dir, FONT, ["square.png"]);
        let (mut gallery, _jobs) = test_gallery();
        let old_id = gallery.id_for(0);
        gallery.request(old_id, &sizing, "square.png");

        gallery.forget_all();
        gallery.image_decoded(old_id, Some(decoded(&dir.join("square.png"))));

        let new_id = gallery.id_for(0);
        assert_ne!(old_id, new_id, "the same block, a different document");
        assert!(gallery.protocol_mut(old_id).is_none());
        assert!(gallery.protocol_mut(new_id).is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_gallery_without_a_picker_asks_for_nothing() {
        let dir = scratch_dir("gallery-disabled");
        write_png(&dir.join("square.png"), 200, 200);
        let sizing = Sizing::measure(&dir, FONT, ["square.png"]);
        let mut gallery = Gallery::disabled();
        let id = gallery.id_for(0);

        gallery.request(id, &sizing, "square.png");

        assert!(gallery.protocol_mut(id).is_none());
        assert!(gallery.font_size().is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_real_image_decodes_into_a_protocol_for_the_terminal() {
        let dir = scratch_dir("decode");
        write_png(&dir.join("square.png"), 200, 200);

        assert!(decode_protocol(&Picker::halfblocks(), &dir.join("square.png")).is_some());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_file_that_is_not_an_image_decodes_to_nothing() {
        let dir = scratch_dir("decode-broken");
        std::fs::write(dir.join("broken.png"), b"this is not an image").unwrap();

        assert!(decode_protocol(&Picker::halfblocks(), &dir.join("broken.png")).is_none());
        assert!(decode_protocol(&Picker::halfblocks(), &dir.join("missing.png")).is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_protocol_override_accepts_every_tier_and_ignores_nonsense() {
        assert_eq!(
            parse_protocol_override("kitty"),
            Some(ProtocolType::Kitty),
            "kitty"
        );
        assert_eq!(
            parse_protocol_override(" Sixel "),
            Some(ProtocolType::Sixel)
        );
        assert_eq!(
            parse_protocol_override("halfblocks"),
            Some(ProtocolType::Halfblocks)
        );
        assert_eq!(
            parse_protocol_override("iterm2"),
            Some(ProtocolType::Iterm2)
        );
        assert_eq!(parse_protocol_override("wat"), None);
        assert_eq!(parse_protocol_override(""), None);
    }
}
