//! Automatic view rules (spec 10 §8): `[[view-rules]]` in settings.toml pick a folder's view when
//! it has none of its own. Pure and in memory: a rule's path glob is compiled once when the
//! settings load, a place's kinds come from its location and the drive and cloud lists the app
//! already has, and a content share is counted from the listing's names. Nothing here asks the
//! disk anything.

use std::path::{Path, PathBuf};

use crate::Entry;
use crate::group::GroupBy;
use crate::kind::Kind;
use crate::nav::Location;
use crate::pattern::{Token, exact_tokens, glob_case};
use crate::view::{ColumnKey, ColumnState, GridSize, SortDir, SortKey, SortSpec, ViewMode, ViewSettings};

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    /// Globs compiled (spec 10 §13.1: none when a folder opens).
    static COMPILED: Cell<usize> = const { Cell::new(0) };
    /// Rules tried (the first match ends the search).
    static TRIED: Cell<usize> = const { Cell::new(0) };
}

/// Path parts compare case-insensitively on Windows only (spec 10 §8.1).
const FOLD: bool = cfg!(windows);

/// What kind of place a folder is (`kind = "…"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceKind {
    Drives,
    Trash,
    Search,
    Flat,
    Network,
    Removable,
    Cloud,
}

impl PlaceKind {
    pub const ALL: [PlaceKind; 7] = [
        PlaceKind::Drives,
        PlaceKind::Trash,
        PlaceKind::Search,
        PlaceKind::Flat,
        PlaceKind::Network,
        PlaceKind::Removable,
        PlaceKind::Cloud,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PlaceKind::Drives => "drives",
            PlaceKind::Trash => "trash",
            PlaceKind::Search => "search",
            PlaceKind::Flat => "flat",
            PlaceKind::Network => "network",
            PlaceKind::Removable => "removable",
            PlaceKind::Cloud => "cloud",
        }
    }

    pub fn parse(text: &str) -> Option<PlaceKind> {
        PlaceKind::ALL.into_iter().find(|k| k.as_str() == text)
    }

    fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// The place shown, as the rules see it: its folder (none for This PC, the trash and results)
/// and its kinds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Place {
    pub path: Option<PathBuf>,
    kinds: u8,
}

impl Place {
    /// `location`'s place. `drives`: the network and removable drives the app has listed (their
    /// roots); `clouds`: the cloud roots. Only these lists and the path's text are looked at.
    pub fn of<'a>(
        location: &Location,
        drives: impl IntoIterator<Item = (&'a Path, PlaceKind)>,
        clouds: impl IntoIterator<Item = &'a Path>,
    ) -> Place {
        let path = match location {
            Location::Path(path) => path,
            Location::Drives => return Place::with(PlaceKind::Drives),
            Location::Trash => return Place::with(PlaceKind::Trash),
            Location::Flat(_) => return Place::with(PlaceKind::Flat),
            Location::Search(spec) => return Place::with(if spec.flat { PlaceKind::Flat } else { PlaceKind::Search }),
        };
        let mut kinds = 0;
        let text = path.to_string_lossy();
        if text.starts_with(r"\\") || text.starts_with("//") {
            kinds |= PlaceKind::Network.bit();
        }
        for (root, kind) in drives {
            if path.starts_with(root) {
                kinds |= kind.bit();
            }
        }
        if clouds.into_iter().any(|root| path.starts_with(root)) {
            kinds |= PlaceKind::Cloud.bit();
        }
        Place { path: Some(path.clone()), kinds }
    }

    fn with(kind: PlaceKind) -> Place {
        Place { path: None, kinds: kind.bit() }
    }

    pub fn is(&self, kind: PlaceKind) -> bool {
        self.kinds & kind.bit() != 0
    }
}

/// What a folder's items are mostly (`content = "pictures >= 50%"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentClass {
    Pictures,
    Videos,
    Audio,
    Documents,
    Archives,
    Code,
    Folders,
}

impl ContentClass {
    pub const ALL: [ContentClass; 7] = [
        ContentClass::Pictures,
        ContentClass::Videos,
        ContentClass::Audio,
        ContentClass::Documents,
        ContentClass::Archives,
        ContentClass::Code,
        ContentClass::Folders,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ContentClass::Pictures => "pictures",
            ContentClass::Videos => "videos",
            ContentClass::Audio => "audio",
            ContentClass::Documents => "documents",
            ContentClass::Archives => "archives",
            ContentClass::Code => "code",
            ContentClass::Folders => "folders",
        }
    }

    pub fn parse(text: &str) -> Option<ContentClass> {
        ContentClass::ALL.into_iter().find(|c| c.as_str() == text)
    }

    fn of(kind: Kind) -> Option<ContentClass> {
        Some(match kind {
            Kind::Folder => ContentClass::Folders,
            Kind::Image => ContentClass::Pictures,
            Kind::Video => ContentClass::Videos,
            Kind::Audio => ContentClass::Audio,
            Kind::Document | Kind::Spreadsheet | Kind::Presentation | Kind::Pdf | Kind::Text => ContentClass::Documents,
            Kind::Archive => ContentClass::Archives,
            Kind::Code => ContentClass::Code,
            _ => return None,
        })
    }
}

/// `content = "<class> >= <n>%"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Content {
    pub class: ContentClass,
    /// 0–100.
    pub percent: u8,
}

impl Content {
    pub fn parse(text: &str) -> Option<Content> {
        let (class, share) = text.split_once(">=")?;
        let class = ContentClass::parse(class.trim())?;
        let percent: u8 = share.trim().strip_suffix('%')?.trim_end().parse().ok().filter(|p| *p <= 100)?;
        Some(Content { class, percent })
    }
}

/// How many of a listing's entries are of each class: one pass over the names, no I/O.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    of: [u32; 7],
    total: u32,
}

impl Counts {
    pub fn of(entries: &[Entry]) -> Counts {
        let mut counts = Counts::default();
        for entry in entries {
            counts.total += 1;
            if let Some(class) = ContentClass::of(Kind::of(&entry.name, entry.is_dir)) {
                counts.of[class as usize] += 1;
            }
        }
        counts
    }

    /// Whether at least `content.percent` % of the entries are of its class; an empty folder
    /// is of none.
    pub fn reaches(&self, content: Content) -> bool {
        self.total > 0
            && u64::from(self.of[content.class as usize]) * 100 >= u64::from(content.percent) * u64::from(self.total)
    }
}

/// The columns a rule shows (`columns = [...]`): which, not in what order (the list's order is
/// fixed) nor how wide (the list's widths).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Columns(u8);

impl Columns {
    pub fn of(keys: &[ColumnKey]) -> Columns {
        Columns(keys.iter().fold(0, |bits, key| bits | 1u8 << key.index()))
    }

    pub fn has(self, key: ColumnKey) -> bool {
        self.0 & (1u8 << key.index()) != 0
    }
}

/// `columns` as shown: with a rule's `set`, its visibility (spec 10 §8.1, §17 decision 36).
pub fn shown_columns(columns: &[ColumnState], set: Option<Columns>) -> Vec<ColumnState> {
    columns.iter().map(|c| ColumnState { visible: set.map_or(c.visible, |set| set.has(c.key)), ..*c }).collect()
}

/// What a rule sets; what it leaves out stays `[view]`'s.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RuleView {
    pub mode: Option<ViewMode>,
    pub sort: Option<SortKey>,
    pub sort_dir: Option<SortDir>,
    pub group: Option<GroupBy>,
    pub grid_size: Option<GridSize>,
    pub columns: Option<Columns>,
}

impl RuleView {
    pub fn is_empty(&self) -> bool {
        *self == RuleView::default()
    }

    pub fn over(&self, base: ViewSettings) -> ViewSettings {
        ViewSettings {
            mode: self.mode.unwrap_or(base.mode),
            sort: SortSpec { key: self.sort.unwrap_or(base.sort.key), dir: self.sort_dir.unwrap_or(base.sort.dir) },
            grid_size: self.grid_size.unwrap_or(base.grid_size),
            group: self.group.unwrap_or(base.group),
        }
    }
}

/// One `[[view-rules]]` entry as settings.toml has it; `path` still has its `{token}`s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSpec {
    /// Its place among the `[[view-rules]]` (1-based), left-out ones counted: "View rule 2".
    pub number: usize,
    pub path: Option<String>,
    pub kind: Option<PlaceKind>,
    pub content: Option<Content>,
    pub set: RuleView,
}

/// A path glob, compiled: `**` takes any number of folders, other parts are names with `*` and `?`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Any,
    Name(Vec<Token>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Glob(Vec<Part>);

impl Glob {
    fn compile(path: &Path) -> Glob {
        #[cfg(test)]
        COMPILED.with(|c| c.set(c.get() + 1));
        let text = slashed(&path.to_string_lossy());
        let mut parts: Vec<Part> = text
            .split('/')
            .map(|part| if part == "**" { Part::Any } else { Part::Name(exact_tokens(part, FOLD)) })
            .collect();
        // `**/**` is `**` (and keeps matching from going exponential).
        parts.dedup_by(|a, b| *a == Part::Any && *b == Part::Any);
        Glob(parts)
    }

    fn matches(&self, parts: &[&str]) -> bool {
        matches_parts(&self.0, parts)
    }
}

fn matches_parts(glob: &[Part], parts: &[&str]) -> bool {
    match glob.split_first() {
        None => parts.is_empty(),
        Some((Part::Any, rest)) => (0..=parts.len()).any(|skip| matches_parts(rest, &parts[skip..])),
        Some((Part::Name(tokens), rest)) => parts
            .split_first()
            .is_some_and(|(first, after)| glob_case(tokens, first, FOLD) && matches_parts(rest, after)),
    }
}

/// A path as globs see it: `/` between parts, no trailing `/` (a root keeps its own).
fn slashed(text: &str) -> String {
    let mut text = if cfg!(windows) { text.replace('\\', "/") } else { text.to_owned() };
    while text.len() > 1 && text.ends_with('/') {
        text.pop();
    }
    text
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    number: usize,
    kind: Option<PlaceKind>,
    path: Option<Glob>,
    content: Option<Content>,
    set: RuleView,
}

/// The rules, compiled once when the settings load (spec 10 §8.3). Empty, they hold nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViewRules(Vec<Rule>);

impl ViewRules {
    /// `expand` turns a path's `{token}` into the folder (the app's `KnownDirs::expand`).
    pub fn compile(specs: &[RuleSpec], expand: &dyn Fn(&str) -> PathBuf) -> ViewRules {
        ViewRules(
            specs
                .iter()
                .map(|spec| Rule {
                    number: spec.number,
                    kind: spec.kind,
                    path: spec.path.as_deref().map(|text| Glob::compile(&expand(text))),
                    content: spec.content,
                    set: spec.set,
                })
                .collect(),
        )
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Rule `rule`'s number in settings.toml.
    pub fn number(&self, rule: usize) -> usize {
        self.0.get(rule).map_or(0, |r| r.number)
    }

    /// The view rule `rule` gives (`None`: `defaults`).
    pub fn view(&self, rule: Option<usize>, defaults: ViewSettings) -> ViewSettings {
        rule.and_then(|i| self.0.get(i)).map_or(defaults, |r| r.set.over(defaults))
    }

    /// The columns rule `rule` shows, if it names them.
    pub fn columns(&self, rule: Option<usize>) -> Option<Columns> {
        rule.and_then(|i| self.0.get(i)).and_then(|r| r.set.columns)
    }

    /// The first rule that matches `place`, whose listing is `entries` (`None`: not a folder's).
    pub fn pick(&self, place: &Place, entries: Option<&[Entry]>) -> Option<usize> {
        self.pick_by(place, &mut || entries.map(Counts::of))
    }

    /// [`pick`](Self::pick) with the count made by `counts`, called at most once and only when a
    /// rule's kind and path match and it has a content condition.
    pub fn pick_by(&self, place: &Place, counts: &mut dyn FnMut() -> Option<Counts>) -> Option<usize> {
        if self.0.is_empty() {
            return None;
        }
        // The folder's parts, split once for all path rules (not at all without one).
        let text = place
            .path
            .as_deref()
            .filter(|_| self.0.iter().any(|r| r.path.is_some()))
            .map(|p| slashed(&p.to_string_lossy()));
        let parts: Vec<&str> = text.as_deref().map_or_else(Vec::new, |t| t.split('/').collect());
        let mut counted: Option<Option<Counts>> = None;
        self.0.iter().position(|rule| {
            #[cfg(test)]
            TRIED.with(|t| t.set(t.get() + 1));
            // Cheapest first (spec 10 §8.3): a bit, the path's parts, then the names' count.
            rule.kind.is_none_or(|kind| place.is(kind))
                && rule.path.as_ref().is_none_or(|glob| text.is_some() && glob.matches(&parts))
                && rule.content.is_none_or(|content| {
                    (*counted.get_or_insert_with(&mut *counts)).is_some_and(|c| c.reaches(content))
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::{Scope, SearchSpec};
    use crate::view::{ColumnKey, default_columns};
    use std::time::{Duration, Instant};

    fn entry(name: &str, is_dir: bool) -> Entry {
        Entry { name: name.to_owned(), is_dir, flags: 0, size: 0, modified: None, created: None }
    }

    /// A rule that turns the grid on when it matches.
    fn rule(number: usize, path: Option<&str>, kind: Option<PlaceKind>, content: Option<&str>) -> RuleSpec {
        RuleSpec {
            number,
            path: path.map(str::to_owned),
            kind,
            content: content.map(|text| Content::parse(text).unwrap()),
            set: RuleView { mode: Some(ViewMode::Grid), ..RuleView::default() },
        }
    }

    /// `{home}` is `/u`; other text as it is.
    fn expand(text: &str) -> PathBuf {
        PathBuf::from(text.replace("{home}", "/u"))
    }

    fn hits(glob: &Glob, path: &str) -> bool {
        let text = slashed(path);
        glob.matches(&text.split('/').collect::<Vec<_>>())
    }

    fn glob(text: &str) -> Glob {
        Glob::compile(Path::new(text))
    }

    #[test]
    fn globs_match_like_the_folder_shows() {
        let h = if cfg!(windows) { "C:/u" } else { "/u" };
        let pictures = glob(&format!("{h}/Pictures/**"));
        assert!(hits(&pictures, &format!("{h}/Pictures")), "** is no folder too");
        assert!(hits(&pictures, &format!("{h}/Pictures/2024/Summer")));
        assert!(hits(&pictures, &format!("{h}/Pictures/")), "a trailing separator is ignored");
        assert!(!hits(&pictures, &format!("{h}/PicturesOld")));
        assert_eq!(hits(&pictures, &format!("{h}/pictures/x")), cfg!(windows), "case: ignored on Windows only");
        let downloads = glob(&format!("{h}/Downloads"));
        assert!(hits(&downloads, &format!("{h}/Downloads")));
        assert!(!hits(&downloads, &format!("{h}/Downloads/x")), "without ** that folder only");
        let anywhere = glob("**/node_modules");
        assert!(hits(&anywhere, &format!("{h}/src/app/node_modules")));
        assert!(!hits(&anywhere, &format!("{h}/node_modules/x")));
        let wild = glob(&format!("{h}/Proj*/?rc"));
        assert!(hits(&wild, &format!("{h}/Projects/src")) && !hits(&wild, &format!("{h}/Projects/source")));
        let deep = glob(&format!("{h}/**/**/x"));
        assert!(hits(&deep, &format!("{h}/x")) && hits(&deep, &format!("{h}/a/b/c/x")));
        if cfg!(windows) {
            assert!(hits(&glob("//nas/photos/**"), r"\\nas\photos\2024"), "UNC paths, either separator");
            assert!(hits(&glob("D:/"), r"D:\"), "a drive's root");
            assert!(hits(&glob(r"D:\Work\**"), r"d:\work\gezik"));
            assert!(hits(&glob("C:/İndirilenler"), "c:/indirilenler"), "the Turkish i folds as names do");
        } else {
            assert!(hits(&glob("/"), "/"), "the root");
            assert!(!hits(&glob("/"), "/u"));
        }
    }

    #[test]
    fn globs_compile_once_when_the_rules_load() {
        COMPILED.with(|c| c.set(0));
        let specs = [
            rule(1, Some("{home}/Pictures/**"), None, None),
            rule(2, None, Some(PlaceKind::Network), None),
            rule(3, Some("{home}/Downloads"), None, None),
        ];
        let rules = ViewRules::compile(&specs, &expand);
        assert_eq!(COMPILED.with(Cell::get), 2, "one per path rule, tokens resolved");
        let place = Place { path: Some(PathBuf::from("/u/Pictures/2024")), kinds: 0 };
        for _ in 0..100 {
            assert_eq!(rules.pick(&place, None), Some(0));
        }
        assert_eq!(COMPILED.with(Cell::get), 2, "opening folders compiles nothing");
    }

    #[test]
    fn places_know_their_kinds() {
        let (usb, nas, cloud) = (PathBuf::from("/media/usb"), PathBuf::from("/mnt/nas"), PathBuf::from("/u/OneDrive"));
        let drives = [(usb.as_path(), PlaceKind::Removable), (nas.as_path(), PlaceKind::Network)];
        let of = |location: Location| Place::of(&location, drives, [cloud.as_path()]);
        let only = |place: &Place, kind: PlaceKind| PlaceKind::ALL.iter().all(|k| place.is(*k) == (*k == kind));
        assert!(only(&of(Location::Drives), PlaceKind::Drives) && of(Location::Drives).path.is_none());
        assert!(only(&of(Location::Trash), PlaceKind::Trash));
        let search = SearchSpec::new(Scope::Folder(PathBuf::from("/u")));
        assert!(only(&of(Location::Search(Box::new(search))), PlaceKind::Search));
        assert!(only(&of(Location::Flat(PathBuf::from("/u"))), PlaceKind::Flat));
        let flat = SearchSpec::flat_view(PathBuf::from("/u"));
        assert!(only(&of(Location::Search(Box::new(flat))), PlaceKind::Flat));
        assert!(only(&of(Location::Path(PathBuf::from("/media/usb/DCIM"))), PlaceKind::Removable));
        assert!(only(&of(Location::Path(PathBuf::from("/mnt/nas"))), PlaceKind::Network));
        assert!(only(&of(Location::Path(PathBuf::from("/u/OneDrive/Docs"))), PlaceKind::Cloud));
        let home = of(Location::Path(PathBuf::from("/u/Documents")));
        assert_eq!((home.path.as_deref(), home.kinds), (Some(Path::new("/u/Documents")), 0));
        assert!(of(Location::Path(PathBuf::from(r"\\nas\share\x"))).is(PlaceKind::Network), "a UNC path");
        assert!(of(Location::Path(PathBuf::from("//nas/share"))).is(PlaceKind::Network));
        assert!(!of(Location::Path(PathBuf::from("/media/usbstick"))).is(PlaceKind::Removable), "whole parts only");
    }

    #[test]
    fn content_counts_the_names_shown() {
        assert_eq!(Content::parse("pictures >= 50%"), Some(Content { class: ContentClass::Pictures, percent: 50 }));
        assert_eq!(Content::parse(" videos>=5 % "), Some(Content { class: ContentClass::Videos, percent: 5 }));
        for bad in ["pictures > 50%", "pictures >= 50", "pictures >= 101%", "photos >= 50%", "pictures >= -1%", ""] {
            assert_eq!(Content::parse(bad), None, "{bad}");
        }
        let share = |text: &str| Content::parse(text).unwrap();
        let half = [entry("a.JPG", false), entry("b.heic", false), entry("c.txt", false), entry("d", true)];
        let counts = Counts::of(&half);
        assert!(counts.reaches(share("pictures >= 50%")), "exactly half reaches 50%");
        assert!(!counts.reaches(share("pictures >= 51%")));
        assert!(counts.reaches(share("folders >= 25%")), "folders count, and are counted in");
        assert!(counts.reaches(share("documents >= 25%")), "a .txt is a document");
        let docs = [entry("a.pdf", false), entry("b.xlsx", false), entry("c.pptx", false), entry("d.docx", false)];
        assert!(Counts::of(&docs).reaches(share("documents >= 100%")));
        let mixed =
            [entry("main.rs", false), entry("x.tar.gz", false), entry("song.FLAC", false), entry("clip.mkv", false)];
        let mixed = Counts::of(&mixed);
        for class in ["code", "archives", "audio", "videos"] {
            assert!(
                mixed.reaches(share(&format!("{class} >= 25%"))) && !mixed.reaches(share(&format!("{class} >= 26%")))
            );
        }
        assert!(!Counts::of(&[]).reaches(share("pictures >= 0%")), "an empty folder is no kind");
    }

    #[test]
    fn the_first_rule_that_matches_wins_and_cheap_checks_go_first() {
        let specs = [
            rule(1, None, Some(PlaceKind::Network), Some("pictures >= 1%")),
            rule(2, Some("/u/Music/**"), None, Some("audio >= 1%")),
            rule(3, None, None, Some("pictures >= 50%")),
            rule(4, None, None, Some("pictures >= 10%")),
            rule(5, Some("/u/**"), None, None),
        ];
        let rules = ViewRules::compile(&specs, &expand);
        let place = Place { path: Some(PathBuf::from("/u/Photos")), kinds: 0 };
        let shown = [entry("a.jpg", false), entry("b.txt", false), entry("c.txt", false), entry("d.txt", false)];
        let mut counted = 0;
        TRIED.with(|t| t.set(0));
        let picked = rules.pick_by(&place, &mut || {
            counted += 1;
            Some(Counts::of(&shown))
        });
        assert_eq!(picked, Some(3), "rule 4 (25% pictures reaches 10%)");
        assert_eq!(TRIED.with(Cell::get), 4, "rule 5 is not tried");
        assert_eq!(counted, 1, "kind and path fail before counting; the count is made once");
        assert_eq!(rules.number(3), 4);
    }

    #[test]
    fn results_and_this_pc_match_by_kind_only() {
        let specs = [
            rule(1, Some("/u/**"), None, None),
            rule(2, None, None, Some("pictures >= 1%")),
            rule(3, None, Some(PlaceKind::Search), None),
        ];
        let rules = ViewRules::compile(&specs, &expand);
        assert_eq!(rules.pick(&Place::with(PlaceKind::Search), None), Some(2), "no folder, no names: only the kind");
        assert_eq!(rules.pick(&Place::with(PlaceKind::Drives), None), None);
    }

    #[test]
    fn a_rule_sets_only_what_it_names() {
        let defaults = ViewSettings { grid_size: GridSize::Small, ..ViewSettings::default() };
        let set = RuleView {
            sort: Some(SortKey::Modified),
            sort_dir: Some(SortDir::Desc),
            group: Some(GroupBy::Date),
            ..RuleView::default()
        };
        let view = set.over(defaults);
        let sort = SortSpec { key: SortKey::Modified, dir: SortDir::Desc };
        assert_eq!(view, ViewSettings { sort, group: GroupBy::Date, ..defaults });
        assert!(RuleView::default().is_empty() && !set.is_empty());
        let size = Columns::of(&[ColumnKey::Size]);
        let specs =
            [RuleSpec { set: RuleView { columns: Some(size), ..set }, ..rule(7, None, Some(PlaceKind::Trash), None) }];
        let rules = ViewRules::compile(&specs, &expand);
        assert_eq!(rules.view(Some(0), defaults), view);
        assert_eq!(rules.view(None, defaults), defaults);
        assert_eq!((rules.number(0), rules.columns(Some(0)), rules.columns(None)), (7, Some(size), None));
    }

    #[test]
    fn a_rules_columns_are_what_shows() {
        let columns = default_columns();
        let shown = shown_columns(&columns, Some(Columns::of(&[ColumnKey::Created, ColumnKey::Size])));
        let visible: Vec<ColumnKey> = shown.iter().filter(|c| c.visible).map(|c| c.key).collect();
        assert_eq!(visible, [ColumnKey::Created, ColumnKey::Size], "the list's order, the rule's choice");
        let widths = |list: &[ColumnState]| list.iter().map(|c| c.width).collect::<Vec<_>>();
        assert_eq!(widths(&shown), widths(&columns), "widths stay the list's");
        assert_eq!(shown_columns(&columns, None), columns);
        let results = Columns::of(&[ColumnKey::Folder, ColumnKey::Match]);
        assert!(
            shown_columns(&columns, Some(results)).iter().all(|c| !c.visible),
            "results' columns are not a folder's"
        );
    }

    #[test]
    fn no_rules_cost_nothing() {
        let rules = ViewRules::compile(&[], &expand);
        assert_eq!(rules, ViewRules::default());
        assert_eq!(rules.0.capacity(), 0, "nothing allocated");
        let mut counted = false;
        let picked = rules.pick_by(&Place::with(PlaceKind::Drives), &mut || {
            counted = true;
            None
        });
        assert_eq!(picked, None);
        assert!(!counted);
        let many: Vec<Entry> = (0..1000).map(|i| entry(&format!("{i}.jpg"), false)).collect();
        assert_eq!(rules.pick(&Place::default(), Some(&many)), None);
    }

    #[test]
    fn nothing_here_touches_the_file_system() {
        let source = include_str!("view_rules.rs");
        let code = source.split("mod tests {").next().unwrap();
        for call in [
            "std::fs",
            "fs::",
            "metadata",
            "canonicalize",
            ".exists(",
            "read_dir",
            "read_link",
            "is_file()",
            "is_dir()",
        ] {
            assert!(!code.contains(call), "{call}");
        }
        // A rule matches a folder that is not there: the text is all it looks at.
        let rules = ViewRules::compile(&[rule(1, Some("/no/such/**"), None, None)], &expand);
        let place = Place { path: Some(PathBuf::from("/no/such/folder")), kinds: 0 };
        assert_eq!(rules.pick(&place, None), Some(0));
    }

    #[test]
    fn twenty_rules_on_a_hundred_thousand_entries_are_quick() {
        // Debug builds are many times slower; scripts/perf/rules.ps1 checks the release numbers.
        let slow: u32 = if cfg!(debug_assertions) { 25 } else { 1 };
        let mut specs = Vec::new();
        for i in 0..10 {
            specs.push(rule(specs.len() + 1, Some(&format!("/u/Folder {i}/**")), None, None));
        }
        for kind in [PlaceKind::Drives, PlaceKind::Trash, PlaceKind::Search, PlaceKind::Network, PlaceKind::Cloud] {
            specs.push(rule(specs.len() + 1, None, Some(kind), None));
        }
        for class in ["videos", "audio", "archives", "code", "documents"] {
            specs.push(rule(specs.len() + 1, None, None, Some(&format!("{class} >= 50%"))));
        }
        let rules = ViewRules::compile(&specs, &expand);
        let entries: Vec<Entry> = (0..100_000)
            .map(|i| match i % 10 {
                0 => entry(&format!("Folder {i}"), true),
                1 => entry(&format!("report {i}.pdf"), false),
                _ => entry(&format!("IMG_{i:06}.jpg"), false),
            })
            .collect();
        let place = Place { path: Some(PathBuf::from("/u/Pictures/2024")), kinds: 0 };
        let started = Instant::now();
        let counts = Counts::of(&entries);
        let counting = started.elapsed();
        assert!(counting < Duration::from_millis(2) * slow, "count: {counting:?}");
        let started = Instant::now();
        for _ in 0..1000 {
            assert_eq!(rules.pick_by(&place, &mut || Some(counts)), None);
        }
        let each = started.elapsed() / 1000;
        assert!(each < Duration::from_micros(50) * slow, "evaluation: {each:?}");
    }
}
