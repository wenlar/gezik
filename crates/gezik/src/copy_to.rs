//! The Copy/Move question (spec 11 §12.1): F5/F6 and Copy To…/Move To… ask the same way. The
//! field is the target folder; its last part may be a name mask (`D:\Yedek\*.bak`), and
//! Filter… asks next for what to take (`*.jpg | thumbs\`). Split File… and Join Files start
//! here too. Nothing here is kept but the filter typed last.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gezik_batch::tasks::{is_part, join_chain, join_label, split_chain, split_label};
use gezik_core::file_filter::FileFilter;
use gezik_core::ops::names::{mask_error, split_mask};
use gezik_core::ops::paths::same_path;
use gezik_core::ops::split::{MAX_PARTS, parse_size, part_count};
use gezik_ops::Pick;

use crate::operations::{After, with_current};
use crate::{dual, panes};

thread_local! {
    /// The filter typed last, offered again (this session only).
    static LAST_FILTER: RefCell<String> = const { RefCell::new(String::new()) };
}

const FILTER: &str = "Filter…";
const DONT_ASK: &str = "Don't Ask Again";

/// What the question asks about.
#[derive(Clone)]
pub(crate) struct Ask {
    pub moving: bool,
    /// A relative path typed is under it.
    pub base: PathBuf,
    /// The field's first text.
    pub field: String,
    /// F5/F6: a "Don't Ask Again" button, which runs this before going on.
    pub dont_ask: Option<fn()>,
    /// Whether Filter… is offered (not when F6 puts items back from the trash).
    pub filter: bool,
    /// The Verify copies box and its mark (spec 11 §5.1); `None`: no box (nothing is copied).
    pub verify: Option<bool>,
    /// The chosen items (their names: the title, and a mask is checked against them; a mask
    /// is not applied to folders, but a folder's name is a fair sample of what is inside).
    pub items: Vec<PathBuf>,
}

/// The question's title for what is typed: `Copy "a.txt" to D:\Yedek?`, `… to …?` while no
/// folder is typed.
fn title_for(verb: &str, items: &[PathBuf], to: Option<&Path>) -> String {
    dual::question_title(verb, items, to.unwrap_or(Path::new("…")))
}

/// Whether `path` is a folder that is there: a last part with `*` or `?` that names one is
/// that folder, not a mask (only Unix allows such names).
fn real_folder(path: &Path) -> bool {
    // shortcut: a stat on the UI thread, only for a typed `*`/`?` on Unix.
    cfg!(unix) && path.is_dir()
}

/// The folder typed and the name mask at its end; a mask alone is for `base` (if it is a
/// folder). Nothing typed: nothing (as F5/F6 always said).
fn resolve<'t>(base: &Path, typed: &'t str, is_dir: impl Fn(&Path) -> bool) -> Option<(PathBuf, Option<&'t str>)> {
    let (folder, mask) = split_mask(typed);
    if mask.is_some()
        && let Some(whole) = dual::typed_target(base, typed)
        && is_dir(&whole)
    {
        return Some((whole, None));
    }
    let to = if folder.trim().is_empty() && mask.is_some() {
        base.is_absolute().then(|| base.to_path_buf())
    } else {
        dual::typed_target(base, folder)
    };
    Some((to?, mask))
}

/// The line under the field: where the items go, and under which names; an error for no
/// folder or a mask that makes unusable names.
fn note_for(base: &Path, typed: &str, names: &[String], is_dir: impl Fn(&Path) -> bool) -> (String, bool) {
    let Some((to, mask)) = resolve(base, typed, is_dir) else { return ("Type a folder".to_owned(), true) };
    if let Some(mask) = mask {
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        if let Some(err) = mask_error(mask, &names) {
            return (err, true);
        }
    }
    let into = (!same_path(&to, base)).then(|| format!("Into {}", to.display()));
    let line = match (into, mask) {
        (Some(into), Some(mask)) => format!("{into}, names as {mask}"),
        (None, Some(mask)) => format!("Names as {mask}"),
        (into, None) => into.unwrap_or_default(),
    };
    (line, false)
}

/// The filter field's line: `>` only when copying.
fn filter_note(typed: &str, moving: bool) -> (String, bool) {
    match FileFilter::parse(typed, !moving) {
        Ok(_) => (String::new(), false),
        Err(err) => (err, true),
    }
}

/// A move whose items are all in the target folder already: nothing would happen.
pub(crate) fn nothing_to_move(paths: &[PathBuf], to: &Path) -> bool {
    paths.iter().all(|path| path.parent().is_some_and(|parent| same_path(parent, to)))
}

/// `[files] verify`: what the Verify copies box starts with.
pub(crate) fn verify_default() -> bool {
    let mut on = false;
    with_current(|ops| on = ops.verify_default());
    on
}

/// Asks; `go` gets the target folder, what to take (a mask, a filter) and the Verify copies
/// box (`None`: none) once answered. An answer the line under the field calls wrong asks again
/// with what was typed (the question stays, red).
pub(crate) fn ask(ask: Ask, go: impl FnOnce(PathBuf, Pick, Option<bool>) + 'static) {
    let again = ask.clone();
    let verb = if ask.moving { "Move" } else { "Copy" };
    let mut buttons = vec![verb];
    buttons.extend(ask.filter.then_some(FILTER));
    buttons.extend(ask.dont_ask.map(|_| DONT_ASK));
    buttons.push("Cancel");
    let chosen: Vec<&'static str> = buttons.clone();
    let message = "Into this folder (a relative path is under it, a missing folder is made; end it with a mask \
                   like *.bak to rename):";
    let (base, noted) = (ask.base.clone(), ask.base);
    let (moving, dont_ask) = (ask.moving, ask.dont_ask);
    let names = Rc::new(item_names(&ask.items));
    let checked = names.clone();
    let items = ask.items;
    let checks: Vec<(String, bool)> = ask.verify.map(|on| ("Verify copies".to_owned(), on)).into_iter().collect();
    with_current(|ops| {
        let dialogs = ops.dialogs();
        // Holds the dialogs while the question is open: the line's closure goes when it closes.
        let titled = dialogs.clone();
        dialogs.ask_form(
            title_for(verb, &items, None),
            message,
            (ask.field, checks),
            &buttons,
            move |typed| {
                let to = resolve(&noted, typed, real_folder).map(|(to, _)| to);
                titled.retitle(&title_for(verb, &items, to.as_deref()));
                note_for(&noted, typed, &checked, real_folder)
            },
            move |answer| {
                let Some((choice, typed, marks)) = answer else { return };
                let verify = marks.first().copied();
                if note_for(&base, &typed, &names, real_folder).1 {
                    return self::ask(Ask { field: typed, verify, ..again }, go);
                }
                let Some((to, mask)) = resolve(&base, &typed, real_folder) else { return };
                let pick = Pick { filter: None, mask: mask.map(str::to_owned) };
                match chosen.get(choice).copied() {
                    Some(FILTER) => {
                        let last = LAST_FILTER.with(|last| last.borrow().clone());
                        ask_filter(moving, to, pick, last, verify, go);
                    }
                    Some(DONT_ASK) => {
                        if let Some(dont_ask) = dont_ask {
                            dont_ask();
                        }
                        go(to, pick, verify);
                    }
                    _ => go(to, pick, verify),
                }
            },
        );
    });
}

/// Filter…: what to take, then the job.
fn ask_filter(
    moving: bool,
    to: PathBuf,
    pick: Pick,
    typed: String,
    verify: Option<bool>,
    go: impl FnOnce(PathBuf, Pick, Option<bool>) + 'static,
) {
    let verb = if moving { "Move" } else { "Copy" };
    let message = if moving {
        "Only these files: patterns split by spaces, what follows | is left out, a name ending in \\ is a folder:"
    } else {
        "Only these files: patterns split by spaces, what follows | is left out, a name ending in \\ is a folder, \
         *.jpg>*.cr2 takes each .jpg's .cr2 along:"
    };
    let note = move |typed: &str| filter_note(typed, moving);
    with_current(|ops| {
        ops.dialogs().ask_text_noted(format!("{verb} only…"), message, typed, &[verb, "Cancel"], note, move |text| {
            let Some(text) = text else { return };
            match FileFilter::parse(&text, !moving) {
                Ok(filter) => {
                    LAST_FILTER.with(|last| *last.borrow_mut() = text);
                    go(to, Pick { filter: Some(filter), ..pick }, verify);
                }
                // Wrong: asked again with what was typed, the line red.
                Err(_) => ask_filter(moving, to, pick, text, verify, go),
            }
        });
    });
}

/// The other pane's folder, when two panes are open.
fn other_folder() -> Option<PathBuf> {
    if !dual::is_open() {
        return None;
    }
    panes::at(i32::from(panes::active_index() == 0)).and_then(|pane| pane.view.folder())
}

/// `copy-to` / `move-to`: `paths` into a typed folder (the other pane's with two panes, else
/// empty), with a mask and a filter if given.
pub fn copy_to_paths(paths: Vec<PathBuf>, moving: bool) {
    if paths.is_empty() {
        return;
    }
    let other = other_folder();
    let base = other.clone().or_else(|| panes::active_view().folder()).unwrap_or_default();
    let field = other.map(|folder| folder.display().to_string()).unwrap_or_default();
    let shown = base.clone();
    let verify = Some(verify_default());
    let question = Ask { moving, base, field, dont_ask: None, filter: true, verify, items: paths.clone() };
    ask(question, move |to, pick, verify| {
        if moving && nothing_to_move(&paths, &to) {
            return panes::active_view().note("Nothing to move: same folder".to_owned());
        }
        // shortcut: one stat per missing level, on the UI thread (as F5/F6's).
        let anchor = dual::anchor(&to, shown, |path| path.exists());
        with_current(|ops| ops.transfer_making(paths, &to, anchor, moving, pick, verify));
    });
}

/// The last parts of `paths`, for checking a mask.
pub(crate) fn item_names(paths: &[PathBuf]) -> Vec<String> {
    paths.iter().filter_map(|path| path.file_name()).map(|name| name.to_string_lossy().into_owned()).collect()
}

/// `copy-to` / `move-to` on the view's selection (else its focused item).
pub fn copy_to(view: &crate::view::View, moving: bool) {
    copy_to_paths(view.selected_entries().into_iter().map(|(path, _)| path).collect(), moving);
}

/// Where Split and Join put what they make: the other pane's folder, else `path`'s own.
fn out_dir(path: &Path) -> PathBuf {
    other_folder().unwrap_or_else(|| path.parent().map(Path::to_path_buf).unwrap_or_default())
}

/// The one file the view acts on: the selection if it is one item, else the focused item
/// with nothing selected (several selected: none, rather than a surprise).
fn one_file(view: &crate::view::View) -> Option<PathBuf> {
    if view.selected_items().len() > 1 {
        return None;
    }
    let index = view.single_selected().or_else(|| view.focus())?;
    view.entry_path(index).filter(|(_, is_dir)| !is_dir).map(|(path, _)| path)
}

/// `split-file`: asks the part size, then splits.
pub fn split(view: &crate::view::View) {
    match one_file(view) {
        Some(path) => split_path(path),
        None => view.note("Select one file to split".to_owned()),
    }
}

pub fn split_path(path: PathBuf) {
    let dir = out_dir(&path);
    // shortcut: a stat and a drive query on the UI thread (a dead network path blocks); ask
    // them off the thread if that bites.
    let len = std::fs::metadata(&path).map_or(0, |meta| meta.len());
    let fat = gezik_platform::fs::drive_facts(&dir).ok().and_then(|facts| facts.max_file).is_some_and(|max| len > max);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let message = format!(
        "Parts of this size (100 MB, 650 MB, 700 MB, 4095 MB for FAT32, 4.7 GB for a DVD), as {name}.001… in {}:",
        dir.display()
    );
    let note = move |typed: &str| match parse_size(typed).map(|size| part_count(len, size)) {
        Ok(parts) if parts > MAX_PARTS => (format!("{parts} parts: at most {MAX_PARTS}"), true),
        Ok(1) => ("1 part".to_owned(), false),
        Ok(parts) => (format!("{parts} parts"), false),
        Err(err) => (err, true),
    };
    let initial = if fat { "4095 MB" } else { "100 MB" };
    with_current(|ops| {
        ops.dialogs().ask_text_noted(
            format!("Split \"{name}\""),
            message,
            initial,
            &["Split", "Cancel"],
            note,
            move |text| {
                let Some(text) = text else { return };
                match parse_size(&text) {
                    Ok(size) => submit_split(path, dir, size),
                    Err(err) => panes::active_view().note(err),
                }
            },
        );
    });
}

fn submit_split(path: PathBuf, dir: PathBuf, size: u64) {
    let again: Rc<dyn Fn()> = Rc::new({
        let (path, dir) = (path.clone(), dir.clone());
        move || submit_split(path.clone(), dir.clone(), size)
    });
    let label = split_label(&path);
    with_current(|ops| {
        ops.submit_chain(split_chain(path, dir, size), Some(label), Some(again), After::Select);
    });
}

/// `join-files`: the set of the selected part (or `.crc`) into one file.
pub fn join(view: &crate::view::View) {
    match one_file(view).filter(|path| is_part(path)) {
        Some(path) => join_path(path),
        None => view.note("Select one .001 part or its .crc file to join".to_owned()),
    }
}

pub fn join_path(path: PathBuf) {
    let dir = out_dir(&path);
    let again: Rc<dyn Fn()> = Rc::new({
        let path = path.clone();
        move || join_path(path.clone())
    });
    let label = join_label(&path);
    with_current(|ops| {
        ops.submit_chain(join_chain(path, dir), Some(label), Some(again), After::Select);
    });
}

/// The row menu's tools for `rows` (spec 11 §3.3: what does not fit is left out): Copy to…,
/// Move to…, Split file… for one file, Join files for one part or `.crc`.
pub fn menu_items(rows: &[(PathBuf, bool)]) -> Vec<(u32, String)> {
    use crate::context_menu::{TOOLS_COPY_TO, TOOLS_JOIN, TOOLS_MOVE_TO, TOOLS_SPLIT};
    if rows.is_empty() {
        return Vec::new();
    }
    let mut items = vec![(TOOLS_COPY_TO, "Copy to…".to_owned()), (TOOLS_MOVE_TO, "Move to…".to_owned())];
    if let [(path, false)] = rows {
        items.push((TOOLS_SPLIT, "Split file…".to_owned()));
        if is_part(path) {
            items.push((TOOLS_JOIN, "Join files".to_owned()));
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_under_the_field_says_where_and_how_named() {
        let base = std::env::temp_dir().join("Yedek");
        let sub = base.join("sub");
        let note = |typed: &str| note_for(&base, typed, &[], |_: &Path| false);
        assert_eq!(note(&base.display().to_string()), (String::new(), false));
        assert_eq!(note("sub"), (format!("Into {}", sub.display()), false));
        let masked = format!("{}{}*.bak", base.display(), std::path::MAIN_SEPARATOR);
        assert_eq!(note(&masked), ("Names as *.bak".to_owned(), false));
        assert_eq!(note("sub/*.bak"), (format!("Into {}, names as *.bak", sub.display()), false));
        assert_eq!(note("*.bak"), ("Names as *.bak".to_owned(), false), "the mask alone: base");
        assert_eq!(note_for(&PathBuf::new(), "*.bak", &[], |_: &Path| false), ("Type a folder".to_owned(), true));
        assert_eq!(note("  "), ("Type a folder".to_owned(), true), "nothing typed, as F5/F6 said");
    }

    #[test]
    fn the_title_follows_the_folder_typed() {
        let base = std::env::temp_dir().join("Yedek");
        let items = [PathBuf::from("a.txt")];
        let to = resolve(&base, "sub", |_: &Path| false).map(|(to, _)| to);
        assert_eq!(
            title_for("Copy", &items, to.as_deref()),
            format!("Copy \"a.txt\" to {}?", base.join("sub").display())
        );
        assert_eq!(title_for("Move", &[PathBuf::from("a"), PathBuf::from("b")], None), "Move 2 items to …?");
    }

    #[test]
    fn a_mask_that_makes_unusable_names_is_refused() {
        let base = std::env::temp_dir().join("Yedek");
        let names = vec!["a.txt".to_owned(), "con".to_owned()];
        assert!(!note_for(&base, "sub/*.bak", &names[..1], |_: &Path| false).1);
        // Only Windows has names a mask can spoil (`:`, a trailing space, device names).
        if cfg!(windows) {
            let (line, bad) = note_for(&base, "sub/*:x", &names, |_: &Path| false);
            assert!(bad && line.starts_with("The mask *:x"), "{line}");
            let (line, bad) = note_for(&base, "*", &names, |_: &Path| false);
            assert!(bad && line.ends_with("of con"), "a chosen item's name: {line}");
            assert!(note_for(&base, "* ", &names, |_: &Path| false).1, "a trailing space");
            // `D:*.bak` is not the folder shown with a mask: the drive alone has no root.
            assert_eq!(note_for(&base, "D:*.bak", &names, |_: &Path| false), ("Type a folder".to_owned(), true));
        }
    }

    #[test]
    fn a_real_folder_with_a_star_is_a_folder_not_a_mask() {
        let base = std::env::temp_dir().join("Yedek");
        let starred = base.join("a*b");
        let is = |path: &Path| path == starred;
        assert_eq!(resolve(&base, "a*b", is), Some((starred.clone(), None)));
        assert_eq!(resolve(&base, "a*c", is), Some((base.clone(), Some("a*c"))));
    }

    #[test]
    fn moving_needs_a_filter_without_related_files() {
        assert!(filter_note("*.jpg>*.cr2", true).1, "> only when copying");
        assert_eq!(filter_note("*.jpg>*.cr2", false), (String::new(), false));
        assert_eq!(filter_note("*.jpg | thumbs\\", true), (String::new(), false));
    }

    #[test]
    fn a_move_into_the_folder_the_items_are_in_does_nothing() {
        let dir = std::env::temp_dir().join("Yedek");
        let paths = [dir.join("a.txt"), dir.join("b.txt")];
        assert!(nothing_to_move(&paths, &dir));
        assert!(!nothing_to_move(&paths, &dir.join("sub")));
        assert!(!nothing_to_move(&[dir.join("a.txt"), dir.join("x").join("b.txt")], &dir));
    }

    #[test]
    fn the_menu_offers_split_for_one_file_and_join_for_a_part() {
        use crate::context_menu::{TOOLS_COPY_TO, TOOLS_JOIN, TOOLS_MOVE_TO, TOOLS_SPLIT};
        let ids = |rows: &[(PathBuf, bool)]| -> Vec<u32> { menu_items(rows).into_iter().map(|(id, _)| id).collect() };
        assert_eq!(ids(&[(PathBuf::from("a.mkv"), false)]), [TOOLS_COPY_TO, TOOLS_MOVE_TO, TOOLS_SPLIT]);
        assert_eq!(
            ids(&[(PathBuf::from("a.mkv.001"), false)]),
            [TOOLS_COPY_TO, TOOLS_MOVE_TO, TOOLS_SPLIT, TOOLS_JOIN]
        );
        assert_eq!(
            ids(&[(PathBuf::from("a.mkv.crc"), false)]),
            [TOOLS_COPY_TO, TOOLS_MOVE_TO, TOOLS_SPLIT, TOOLS_JOIN]
        );
        assert_eq!(ids(&[(PathBuf::from("d"), true)]), [TOOLS_COPY_TO, TOOLS_MOVE_TO], "a folder");
        let two = [(PathBuf::from("a"), false), (PathBuf::from("b"), false)];
        assert_eq!(ids(&two), [TOOLS_COPY_TO, TOOLS_MOVE_TO]);
        assert!(ids(&[]).is_empty());
    }
}
