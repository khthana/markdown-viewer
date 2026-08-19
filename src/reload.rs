use std::io::ErrorKind;
use std::path::Path;

use ratatui_image::FontSize;

use crate::app::{self, App};
use crate::image::{Gallery, Sizing};
use crate::markdown::blocks::{self, Block, HeadingRef};
use crate::markdown::layout::{self, LayoutDoc};
use crate::search;
use crate::theme::Palette;
use crate::toc::{self, TocEntry};

/// What the user's scroll position was pinned to when a reload started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnchorKind {
    /// The nearest heading at or above the scroll position, identified by
    /// its level and text rather than its index, so it survives blocks
    /// being inserted or removed above it. `occurrence` disambiguates
    /// repeated identical headings; `offset` is how far the viewport had
    /// scrolled past the heading's own row.
    Heading {
        level: u8,
        text: String,
        occurrence: usize,
        offset: usize,
    },
    /// No heading exists at or above the scroll position (the viewport is
    /// in the document's preamble), so the nearest non-blank row's text is
    /// the anchor instead, with the same offset rule as a heading.
    Content { text: String, offset: usize },
    /// Nothing renderable sits at or above the scroll position (an empty
    /// document), so there's nothing to pin to at all.
    Unanchored,
}

/// A parsed Markdown file: its blocks, the headings collected from them,
/// and the measurements of the images it references. A reload replaces
/// one of these wholesale.
pub struct Document {
    pub blocks: Vec<Block>,
    pub headings: Vec<HeadingRef>,
    pub image_sizing: Sizing,
}

/// Reads and parses the file at `path`, measuring its images against
/// `font` — the terminal's cell size, or `None` on a terminal that can't
/// draw them, where every image stays an alt-text placeholder.
pub fn load(path: &Path, font: Option<FontSize>) -> anyhow::Result<Document> {
    // The same wording the startup check uses, so a file that goes
    // missing later reads the same as one that never existed.
    let content = std::fs::read_to_string(path)
        .map_err(|error| anyhow::anyhow!(describe_failure(path, &error)))?;
    let (blocks, headings) = blocks::lower_with_headings(&content);
    let images = match font {
        // Image paths are written relative to the document, not to
        // wherever the viewer happens to be run from.
        Some(font) => Sizing::measure(
            path.parent().unwrap_or(Path::new(".")),
            font,
            blocks::image_paths(&blocks),
        ),
        None => Sizing::text_only(),
    };
    Ok(Document {
        blocks,
        headings,
        image_sizing: images,
    })
}

/// Checks the file can actually be read, before the caller enters the
/// alternate screen.
///
/// Doing it up front is what makes a bad path print one plain line
/// instead of flashing an empty pager and erroring out behind it.
pub fn check_readable(path: &Path) -> anyhow::Result<()> {
    // Checked before opening because Windows reports opening a directory
    // as a permission error, which would be a confusing thing to tell the
    // reader.
    let failure = if path.is_dir() {
        std::io::Error::from(ErrorKind::IsADirectory)
    } else {
        match std::fs::File::open(path) {
            Ok(_) => return Ok(()),
            Err(error) => error,
        }
    };
    Err(anyhow::anyhow!(describe_failure(path, &failure)))
}

/// Says what went wrong in the reader's terms rather than Rust's: they
/// get "no such file: notes.md", not an `os error 2` dump.
fn describe_failure(path: &Path, error: &std::io::Error) -> String {
    let path = path.display();
    match error.kind() {
        ErrorKind::NotFound => format!("no such file: {path}"),
        ErrorKind::PermissionDenied => format!("permission denied: {path}"),
        ErrorKind::IsADirectory => format!("{path} is a directory, not a file"),
        // Anything rarer still names the file first, so the reader knows
        // which one failed even when the rest is the OS talking.
        _ => format!("could not read {path}: {error}"),
    }
}

/// Re-reads the file and swaps in the new document, moving `app`'s scroll
/// position to wherever the old document's anchor now lives and, when a
/// search is active, re-running the query and re-selecting the equivalent
/// match in the new text.
///
/// A read failure leaves the current document on screen rather than
/// erroring out: a save in progress can leave the file momentarily
/// missing or empty, and the next change event (or `r`) picks it up.
pub fn reload_preserving_position(
    path: &Path,
    document: &mut Document,
    app: &mut App,
    layout_doc: &LayoutDoc,
    width: usize,
    palette: Palette,
    gallery: &mut Gallery,
) {
    let anchor = compute_anchor(&document.headings, layout_doc, app.scroll);
    let match_anchor = search::anchor_match(&app.search_query, layout_doc, app.current_match);
    let Ok(fresh) = load(path, gallery.font_size()) else {
        return;
    };
    // Whatever was decoded belongs to the old file; the same path may be
    // a different picture now.
    gallery.forget_all();

    let new_layout = layout::layout(&fresh.blocks, width, &fresh.image_sizing, palette);
    app.total_rows = new_layout.total_rows;
    app.scroll = resolve_anchor(&anchor, &fresh.headings, &new_layout, app.viewport_height);
    // The scroll position stays where the document anchor put it: the
    // reader's place in the text outranks the selected match, which they
    // can step back to with `n`.
    if app.search_active {
        let reselection =
            search::resolve_match(match_anchor.as_ref(), &app.search_query, &new_layout);
        app.apply_reselection(reselection);
    }
    *document = fresh;
}

/// What a reload should put the viewport back on, captured against the
/// document being replaced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    pub kind: AnchorKind,
    /// The scroll offset the anchor was computed at, used as the fallback
    /// when the anchored heading no longer exists after the edit.
    pub scroll: usize,
}

/// Captures what the current scroll position is anchored to, before the
/// old layout is discarded.
pub fn compute_anchor(headings: &[HeadingRef], layout_doc: &LayoutDoc, scroll: usize) -> Anchor {
    let entries = toc::resolve(headings, layout_doc);
    let anchored = entries
        .iter()
        .enumerate()
        .rev()
        .find(|(_, entry)| entry.row <= scroll);
    let kind = match anchored {
        Some((index, entry)) => AnchorKind::Heading {
            level: entry.level,
            text: entry.text.clone(),
            occurrence: entries[..index]
                .iter()
                .filter(|other| is_same_heading(other, entry.level, &entry.text))
                .count(),
            offset: scroll - entry.row,
        },
        None => nearest_content(layout_doc, scroll),
    };
    Anchor { kind, scroll }
}

/// Whether a TOC entry is the same heading an anchor names. Headings are
/// identified by what the reader sees — level and text — since indices
/// and rows both move as soon as anything above them is edited.
fn is_same_heading(entry: &TocEntry, level: u8, text: &str) -> bool {
    entry.level == level && entry.text == text
}

/// The nearest non-blank rendered row at or above `scroll`, used when no
/// heading precedes the viewport. Blank rows are skipped because they
/// match everywhere and would anchor to an arbitrary position.
fn nearest_content(layout_doc: &LayoutDoc, scroll: usize) -> AnchorKind {
    layout_doc
        .rows
        .iter()
        .enumerate()
        .take(scroll + 1)
        .rev()
        .find(|(_, text)| !text.trim().is_empty())
        .map(|(row, text)| AnchorKind::Content {
            text: text.clone(),
            offset: scroll - row,
        })
        .unwrap_or(AnchorKind::Unanchored)
}

/// Resolves an anchor against a freshly parsed document, returning the
/// scroll offset that puts the user back where they were.
pub fn resolve_anchor(
    anchor: &Anchor,
    headings: &[HeadingRef],
    layout_doc: &LayoutDoc,
    viewport_height: usize,
) -> usize {
    let max_scroll = app::max_scroll(layout_doc.total_rows, viewport_height);
    match &anchor.kind {
        AnchorKind::Heading {
            level,
            text,
            occurrence,
            offset,
        } => {
            let entries = toc::resolve(headings, layout_doc);
            let candidates: Vec<_> = entries
                .iter()
                .filter(|entry| is_same_heading(entry, *level, text))
                .collect();
            // Only the same occurrence counts: falling back to another
            // copy of a repeated heading would scroll the reader to a
            // different section, which is worse than clamping in place.
            match candidates.get(*occurrence) {
                Some(entry) => (entry.row + offset).min(max_scroll),
                None => anchor.scroll.min(max_scroll),
            }
        }
        AnchorKind::Content { text, offset } => {
            match layout_doc.rows.iter().position(|row| row == text) {
                Some(row) => (row + offset).min(max_scroll),
                None => anchor.scroll.min(max_scroll),
            }
        }
        AnchorKind::Unanchored => anchor.scroll.min(max_scroll),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::blocks::lower_with_headings;
    use crate::markdown::layout;

    const WIDTH: usize = 80;
    const VIEWPORT: usize = 2;

    struct Doc {
        headings: Vec<HeadingRef>,
        layout: LayoutDoc,
    }

    fn doc(source: &str) -> Doc {
        let (blocks, headings) = lower_with_headings(source);
        let layout = layout::layout(
            &blocks,
            WIDTH,
            &crate::image::Sizing::text_only(),
            Palette::Dark,
        );
        Doc { headings, layout }
    }

    #[test]
    fn follows_the_anchored_heading_when_content_is_inserted_above_it() {
        // H1 "Title" occupies rows 0-1 (text + rule), "Intro." row 2, so
        // "## Section" starts at row 3.
        let old = doc("# Title\n\nIntro.\n\n## Section\n\nBody text.");
        let anchor = compute_anchor(&old.headings, &old.layout, 3);

        // One extra paragraph above pushes "## Section" down to row 4.
        let new = doc("# Title\n\nIntro.\n\nExtra paragraph.\n\n## Section\n\nBody text.");

        assert_eq!(
            resolve_anchor(&anchor, &new.headings, &new.layout, VIEWPORT),
            4
        );
    }

    #[test]
    fn distinguishes_repeated_identical_headings_by_occurrence() {
        // Two identical "## Notes" headings: rows 2-3 and 5-6, with the
        // viewport parked on the second one.
        let old = doc("# T

## Notes

A.

## Notes

B.");
        let anchor = compute_anchor(&old.headings, &old.layout, 5);

        // A paragraph inserted between them pushes the second to row 6.
        let new = doc("# T

## Notes

A.

Extra.

## Notes

B.");

        assert_eq!(
            resolve_anchor(&anchor, &new.headings, &new.layout, VIEWPORT),
            6
        );
    }

    #[test]
    fn keeps_the_offset_into_a_section_when_its_heading_moves() {
        // Parked two rows past "## Section" (heading at row 3, scroll 5).
        let old = doc("# Title\n\nIntro.\n\n## Section\n\nLine one.\n\nLine two.\n\nLine three.");
        let anchor = compute_anchor(&old.headings, &old.layout, 5);

        // The heading moves to row 4, so the same two-row offset is row 6.
        let new = doc(
            "# Title\n\nIntro.\n\nExtra.\n\n## Section\n\nLine one.\n\nLine two.\n\nLine three.",
        );

        assert_eq!(
            resolve_anchor(&anchor, &new.headings, &new.layout, VIEWPORT),
            6
        );
    }

    #[test]
    fn is_unmoved_by_a_heading_added_below_the_anchor() {
        let old = doc("# Title\n\nIntro.\n\n## Section\n\nBody text.");
        let anchor = compute_anchor(&old.headings, &old.layout, 3);

        let new = doc("# Title\n\nIntro.\n\n## Section\n\nBody text.\n\n## Later\n\nMore.");

        assert_eq!(
            resolve_anchor(&anchor, &new.headings, &new.layout, VIEWPORT),
            3
        );
    }

    #[test]
    fn falls_back_to_the_previous_scroll_when_the_anchored_heading_is_renamed() {
        let old = doc("# Title\n\nIntro.\n\n## Section\n\nBody text.");
        let anchor = compute_anchor(&old.headings, &old.layout, 3);

        let new = doc("# Title\n\nIntro.\n\n## Renamed\n\nBody text.");

        assert_eq!(
            resolve_anchor(&anchor, &new.headings, &new.layout, VIEWPORT),
            3
        );
    }

    #[test]
    fn clamps_to_the_shortened_document_when_the_anchored_heading_is_deleted() {
        let old = doc("# Title\n\nIntro.\n\n## Section\n\nBody text.");
        let anchor = compute_anchor(&old.headings, &old.layout, 3);

        // Only 3 rows survive, so the deepest legal scroll is 1 — the point
        // is that it lands there rather than snapping back to the top.
        let new = doc("# Title\n\nIntro.");

        assert_eq!(
            resolve_anchor(&anchor, &new.headings, &new.layout, VIEWPORT),
            1
        );
    }

    #[test]
    fn clamps_the_raw_scroll_when_there_is_nothing_to_anchor_to() {
        let old = doc("");
        let anchor = compute_anchor(&old.headings, &old.layout, 0);
        assert_eq!(anchor.kind, AnchorKind::Unanchored);

        let new = doc("# Title

Body.");

        assert_eq!(
            resolve_anchor(&anchor, &new.headings, &new.layout, VIEWPORT),
            0
        );
    }

    #[test]
    fn clamps_when_the_anchored_content_was_deleted() {
        let old = doc("Intro.

Second line.

# Title

Body.");
        let anchor = compute_anchor(&old.headings, &old.layout, 1);

        // "Second line." is gone and only 3 rows survive, so the deepest
        // legal scroll is 1 rather than the top.
        let new = doc("Intro.

# Title");

        assert_eq!(
            resolve_anchor(&anchor, &new.headings, &new.layout, VIEWPORT),
            1
        );
    }

    #[test]
    fn follows_the_content_under_the_viewport_when_there_is_no_heading_above_it() {
        // Rows: "Intro." 0, "Second line." 1, "# Title" 2-3, "Body." 4 —
        // parked on row 1, above the document's first heading.
        let old = doc("Intro.\n\nSecond line.\n\n# Title\n\nBody.");
        let anchor = compute_anchor(&old.headings, &old.layout, 1);

        // A new first paragraph pushes that content down to row 2.
        let new = doc("Preamble added.\n\nIntro.\n\nSecond line.\n\n# Title\n\nBody.");

        assert_eq!(
            resolve_anchor(&anchor, &new.headings, &new.layout, VIEWPORT),
            2
        );
    }

    #[test]
    fn clamps_rather_than_jumping_back_when_the_anchored_duplicate_is_deleted() {
        // Parked on the second "## Notes" (row 5 of 8).
        let old = doc("# T\n\n## Notes\n\nA.\n\n## Notes\n\nB.");
        let anchor = compute_anchor(&old.headings, &old.layout, 5);

        // That copy is gone; the surviving one is an earlier section, so
        // resolving to it would scroll the reader backwards.
        let new = doc("# T\n\n## Notes\n\nA.");

        assert_eq!(
            resolve_anchor(&anchor, &new.headings, &new.layout, VIEWPORT),
            3
        );
    }

    #[test]
    fn follows_the_anchored_heading_when_a_heading_is_added_above_it() {
        let old = doc("# Title\n\nIntro.\n\n## Section\n\nBody text.");
        let anchor = compute_anchor(&old.headings, &old.layout, 3);

        // A whole new section above pushes "## Section" from row 3 to 6.
        let new = doc("# Title\n\nIntro.\n\n## New Section\n\nExtra.\n\n## Section\n\nBody text.");

        assert_eq!(
            resolve_anchor(&anchor, &new.headings, &new.layout, VIEWPORT),
            6
        );
    }

    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mdview-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reloading_after_an_edit_keeps_the_viewport_on_the_same_heading() {
        let dir = scratch_dir("reload");
        let path = dir.join("notes.md");
        std::fs::write(&path, "# Title\n\nIntro.\n\n## Section\n\nBody text.").unwrap();

        let mut document = load(&path, None).unwrap();
        let layout_doc = layout::layout(
            &document.blocks,
            WIDTH,
            &crate::image::Sizing::text_only(),
            Palette::Dark,
        );
        let mut app = App::new(layout_doc.total_rows);
        app.viewport_height = VIEWPORT;
        app.scroll = 3; // the row "## Section" starts on

        std::fs::write(
            &path,
            "# Title\n\nIntro.\n\nExtra.\n\n## Section\n\nBody text.",
        )
        .unwrap();
        reload_preserving_position(
            &path,
            &mut document,
            &mut app,
            &layout_doc,
            WIDTH,
            Palette::Dark,
            &mut Gallery::disabled(),
        );

        assert_eq!(app.scroll, 4, "the heading moved down one row");
        assert_eq!(app.total_rows, 7, "the new document's rows are in effect");
        assert_eq!(document.headings.len(), 2);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_file_that_cannot_be_read_leaves_the_current_document_on_screen() {
        let dir = scratch_dir("reload-unreadable");
        let path = dir.join("notes.md");
        std::fs::write(&path, "# Title\n\nIntro.\n\n## Section\n\nBody text.").unwrap();

        let mut document = load(&path, None).unwrap();
        let layout_doc = layout::layout(
            &document.blocks,
            WIDTH,
            &crate::image::Sizing::text_only(),
            Palette::Dark,
        );
        let mut app = App::new(layout_doc.total_rows);
        app.viewport_height = VIEWPORT;
        app.scroll = 3;
        let block_count = document.blocks.len();

        // Mid-save, an editor can leave the path momentarily missing.
        std::fs::remove_file(&path).unwrap();
        reload_preserving_position(
            &path,
            &mut document,
            &mut app,
            &layout_doc,
            WIDTH,
            Palette::Dark,
            &mut Gallery::disabled(),
        );

        assert_eq!(app.scroll, 3);
        assert_eq!(app.total_rows, layout_doc.total_rows);
        assert_eq!(document.blocks.len(), block_count);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A viewer parked on a match of `query` in a file containing
    /// `source`, ready for the file to be rewritten and reloaded.
    struct SearchSession {
        dir: std::path::PathBuf,
        path: std::path::PathBuf,
        document: Document,
        layout: LayoutDoc,
        app: App,
    }

    impl SearchSession {
        fn open(name: &str, source: &str, query: &str, current: Option<usize>) -> Self {
            let dir = scratch_dir(name);
            let path = dir.join("notes.md");
            std::fs::write(&path, source).unwrap();

            let document = load(&path, None).unwrap();
            let layout = layout::layout(
                &document.blocks,
                WIDTH,
                &crate::image::Sizing::text_only(),
                Palette::Dark,
            );
            let mut app = App::new(layout.total_rows);
            app.viewport_height = VIEWPORT;
            app.search_query = query.to_string();
            app.search_active = true;
            app.current_match = current;

            Self {
                dir,
                path,
                document,
                layout,
                app,
            }
        }

        /// Rewrites the file the way an editor's save would, then reloads.
        fn save_and_reload(&mut self, source: &str) {
            std::fs::write(&self.path, source).unwrap();
            reload_preserving_position(
                &self.path,
                &mut self.document,
                &mut self.app,
                &self.layout,
                WIDTH,
                Palette::Dark,
                &mut Gallery::disabled(),
            );
        }
    }

    impl Drop for SearchSession {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.dir).ok();
        }
    }

    #[test]
    fn reloading_keeps_the_same_search_match_selected() {
        // Parked on the "fox" in "fox two".
        let mut session =
            SearchSession::open("reload-search", "fox one\n\nfox two", "fox", Some(1));

        // A new occurrence above shifts the match list by one.
        session.save_and_reload("fox zero\n\nfox one\n\nfox two");

        assert_eq!(session.app.current_match, Some(2), "same match, new index");
        assert!(!session.app.search_fell_back);
    }

    #[test]
    fn reloading_after_the_selected_match_is_deleted_falls_back_to_the_first() {
        let mut session =
            SearchSession::open("reload-search-lost", "fox one\n\nfox two", "fox", Some(1));

        session.save_and_reload("fox one");

        assert_eq!(session.app.current_match, Some(0));
        assert!(
            session.app.search_fell_back,
            "the status line should note the move"
        );
    }

    #[test]
    fn reloading_a_document_that_no_longer_matches_clears_the_selection() {
        let mut session =
            SearchSession::open("reload-search-gone", "fox one\n\nfox two", "fox", Some(1));

        session.save_and_reload("nothing here now");

        assert_eq!(session.app.current_match, None);
        assert!(
            session.app.search_active,
            "the query stays active, showing no matches"
        );
    }

    #[test]
    fn reloading_selects_a_match_for_a_query_that_previously_matched_nothing() {
        // A confirmed search with no matches leaves nothing selected.
        let mut session = SearchSession::open("reload-search-appears", "nothing here", "fox", None);

        session.save_and_reload("a fox appears");

        assert_eq!(session.app.current_match, Some(0));
        assert!(
            !session.app.search_fell_back,
            "there was no earlier match to lose"
        );
    }

    #[test]
    fn a_missing_file_is_reported_by_name_without_an_os_error_dump() {
        let missing = std::env::temp_dir().join("mdview-does-not-exist-9f3a.md");

        let message = check_readable(&missing)
            .expect_err("a missing file must not be readable")
            .to_string();

        assert!(
            message.contains("no such file"),
            "unhelpful message: {message}"
        );
        assert!(
            message.contains("mdview-does-not-exist-9f3a.md"),
            "the message doesn't say which file: {message}"
        );
        assert!(
            !message.contains("os error"),
            "the raw OS error leaked through: {message}"
        );
    }

    #[test]
    fn a_directory_is_reported_as_a_directory_rather_than_a_permissions_problem() {
        let message = check_readable(&std::env::temp_dir())
            .expect_err("a directory isn't a markdown file")
            .to_string();

        assert!(
            message.contains("is a directory"),
            "unhelpful message: {message}"
        );
    }

    #[test]
    fn an_unreadable_file_names_the_permission_problem() {
        let path = Path::new("locked.md");
        let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);

        let message = describe_failure(path, &denied);

        assert_eq!(message, "permission denied: locked.md");
    }

    #[test]
    fn an_unexpected_failure_still_names_the_file_first() {
        let path = Path::new("odd.md");
        let broken = std::io::Error::from(std::io::ErrorKind::InvalidData);

        let message = describe_failure(path, &broken);

        assert!(message.starts_with("could not read odd.md"), "{message}");
    }

    #[test]
    fn a_readable_file_passes_the_check() {
        let dir = scratch_dir("readable");
        let file = dir.join("doc.md");
        std::fs::write(&file, "# hi").unwrap();

        assert!(check_readable(&file).is_ok());

        std::fs::remove_dir_all(&dir).ok();
    }
}
