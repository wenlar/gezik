//! The Info window's model (spec 9 §4.4): permissions, owner, group and the macOS Hidden and
//! Locked flags of the items it shows, the boxes and fields that show them, and what one change
//! writes to each item. Pure: `gezik_platform::attrs` reads and writes, `gezik_ops::SetAttributesTask`
//! runs the change.

use std::path::PathBuf;

/// The read, write and execute bits of the owner, the group and everyone.
pub const PERMS: u32 = 0o777;
/// setuid, setgid and sticky: shown, never changed here (spec 9 §4.4).
pub const SPECIAL: u32 = 0o7000;
/// macOS `UF_HIDDEN` and `UF_IMMUTABLE`: Finder's Hidden and Locked.
pub const HIDDEN: u32 = 0x8000;
pub const LOCKED: u32 = 0x2;
/// The boxes' bits in the window's order: the owner's read, write, execute, the group's, everyone's.
pub const PERM_BITS: [u32; 9] = [0o400, 0o200, 0o100, 0o040, 0o020, 0o010, 0o004, 0o002, 0o001];

/// What the window shows and changes of one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attrs {
    /// The permission and special bits (`st_mode & 0o7777`).
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    /// `HIDDEN` and `LOCKED` only (always 0 off macOS).
    pub flags: u32,
}

/// Which file a path was when it was read: one saved over or moved in since is another file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Identity {
    pub dev: u64,
    pub ino: u64,
}

/// One item as read with `lstat`: a link itself, never what it leads to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Entry {
    pub id: Identity,
    pub attrs: Attrs,
    pub is_dir: bool,
    pub is_link: bool,
}

/// One item to change: what it must still be (`id`, `from`) and what it becomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    pub path: PathBuf,
    pub id: Identity,
    pub from: Attrs,
    pub to: Attrs,
}

/// A box over several items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tri {
    Off,
    On,
    Mixed,
}

impl Tri {
    /// The window's number for it: 0 off, 1 on, 2 mixed.
    pub fn index(self) -> i32 {
        match self {
            Tri::Off => 0,
            Tri::On => 1,
            Tri::Mixed => 2,
        }
    }

    /// What a click turns it to: on, unless it is on (a mixed box turns on, as in Finder).
    pub fn clicked(self) -> bool {
        self != Tri::On
    }
}

/// On if every value is, off if none is (or there is none), else mixed.
pub fn tri(values: impl IntoIterator<Item = bool>) -> Tri {
    let (mut on, mut off) = (false, false);
    for value in values {
        if value {
            on = true;
        } else {
            off = true;
        }
    }
    match (on, off) {
        (true, false) => Tri::On,
        (true, true) => Tri::Mixed,
        _ => Tri::Off,
    }
}

/// The nine permission boxes over `items`.
pub fn perm_states(items: &[Attrs]) -> [Tri; 9] {
    PERM_BITS.map(|bit| tri(items.iter().map(|a| a.mode & bit != 0)))
}

/// Hidden's or Locked's box over `items`.
pub fn flag_state(items: &[Attrs], flag: u32) -> Tri {
    tri(items.iter().map(|a| a.flags & flag != 0))
}

/// What one change in the window writes: only its own part of each item changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Change {
    /// Permission bits turned on and off (never `SPECIAL`: `apply` masks them out).
    pub set: u32,
    pub clear: u32,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    /// `HIDDEN`/`LOCKED` turned on and off.
    pub flags_set: u32,
    pub flags_clear: u32,
}

impl Change {
    /// One box: `bit` (one of `PERM_BITS`) on or off.
    pub fn bit(bit: u32, on: bool) -> Change {
        let bit = bit & PERMS;
        if on { Change { set: bit, ..Change::default() } } else { Change { clear: bit, ..Change::default() } }
    }

    /// Exactly `perms` (the octal field); special bits stay.
    pub fn mode(perms: u32) -> Change {
        Change { set: perms & PERMS, clear: !perms & PERMS, ..Change::default() }
    }

    pub fn owner(uid: u32) -> Change {
        Change { uid: Some(uid), ..Change::default() }
    }

    pub fn group(gid: u32) -> Change {
        Change { gid: Some(gid), ..Change::default() }
    }

    /// Hidden or Locked on or off; any other flag is ignored.
    pub fn flag(flag: u32, on: bool) -> Change {
        let flag = flag & (HIDDEN | LOCKED);
        if on {
            Change { flags_set: flag, ..Change::default() }
        } else {
            Change { flags_clear: flag, ..Change::default() }
        }
    }

    pub fn apply(&self, attrs: Attrs) -> Attrs {
        let (fset, fclear) = (self.flags_set & (HIDDEN | LOCKED), self.flags_clear & (HIDDEN | LOCKED));
        Attrs {
            mode: (attrs.mode & !(self.clear & PERMS)) | (self.set & PERMS),
            uid: self.uid.unwrap_or(attrs.uid),
            gid: self.gid.unwrap_or(attrs.gid),
            flags: (attrs.flags & !fclear) | fset,
        }
    }
}

/// What `change` writes to each of `items` (the window's items, in its order): only those it
/// changes. A link's permissions and flags never change (only its owner and group).
pub fn wanted(items: &[(PathBuf, Entry)], change: Change) -> Vec<Wanted> {
    items
        .iter()
        .filter_map(|(path, entry)| {
            let change = if entry.is_link {
                Change { set: 0, clear: 0, flags_set: 0, flags_clear: 0, ..change }
            } else {
                change
            };
            let to = change.apply(entry.attrs);
            (to != entry.attrs).then(|| Wanted { path: path.clone(), id: entry.id, from: entry.attrs, to })
        })
        .collect()
}

/// Which fields may change: (permissions: not for links, not while locked; owner and group:
/// not while locked). A locked item is unlocked first.
pub fn editable(items: &[(PathBuf, Entry)]) -> (bool, bool) {
    let locked = items.iter().any(|(_, e)| e.attrs.flags & LOCKED != 0);
    (!locked && !items.iter().any(|(_, e)| e.is_link), !locked)
}

/// "755": the permissions every item has; "" when they differ (or there is none).
pub fn octal_text(items: &[Attrs]) -> String {
    let Some(first) = items.first() else { return String::new() };
    if items.iter().all(|a| a.mode & PERMS == first.mode & PERMS) {
        format!("{:03o}", first.mode & PERMS)
    } else {
        String::new()
    }
}

/// The octal field: three digits from 0 to 7 ("755"), a `0` in front allowed.
pub fn parse_octal(text: &str) -> Result<u32, String> {
    let text = text.trim();
    if !(3..=4).contains(&text.len()) || !text.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
        return Err("Type three digits from 0 to 7, like 755".to_owned());
    }
    let value = text.bytes().fold(0, |acc, b| acc * 8 + u32::from(b - b'0'));
    if value & SPECIAL != 0 {
        return Err("Setuid, setgid and sticky can't be changed here".to_owned());
    }
    Ok(value)
}

/// "setuid, sticky": the special bits any of `items` has; "" for none.
pub fn special_text(items: &[Attrs]) -> String {
    let any = items.iter().fold(0, |acc, a| acc | a.mode);
    let names: Vec<&str> = [(0o4000, "setuid"), (0o2000, "setgid"), (0o1000, "sticky")]
        .into_iter()
        .filter(|(bit, _)| any & bit != 0)
        .map(|(_, name)| name)
        .collect();
    names.join(", ")
}

/// A user or group typed by name or number; names from `known` (name, id). `what`: "user" or "group".
/// A name wins over a number, as in chown; `u32::MAX` is chown's "leave as is", never an id.
pub fn parse_id(text: &str, known: &[(String, u32)], what: &str) -> Result<u32, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(format!("Type a {what} name or number"));
    }
    if let Some((_, id)) = known.iter().find(|(name, _)| name == text) {
        return Ok(*id);
    }
    match text.parse::<u32>() {
        Ok(id) if id != u32::MAX && text.bytes().all(|b| b.is_ascii_digit()) => Ok(id),
        _ => Err(format!("No {what} is named \"{text}\"")),
    }
}

/// `id`'s name in `known`, else the number.
pub fn name_of(id: u32, known: &[(String, u32)]) -> String {
    known.iter().find(|(_, k)| *k == id).map_or_else(|| id.to_string(), |(name, _)| name.clone())
}

/// The one owner (or group) every item has, by name; "" when they differ.
pub fn shared_name(ids: impl IntoIterator<Item = u32>, known: &[(String, u32)]) -> String {
    let mut ids = ids.into_iter();
    let Some(first) = ids.next() else { return String::new() };
    if ids.all(|id| id == first) { name_of(first, known) } else { String::new() }
}

/// "Apply to enclosed items": what an item inside a folder becomes. The folder's owner, group
/// and read and write bits; execute as the folder's for folders, and for files only where they
/// had some (chmod's `X`: no file becomes runnable, none stops being); its special bits and
/// flags stay, and the folder's own special bits do not spread.
pub fn enclosed_target(folder: Attrs, item: Attrs, is_dir: bool) -> Attrs {
    let execute = if is_dir || item.mode & 0o111 != 0 { folder.mode & 0o111 } else { 0 };
    Attrs {
        mode: (item.mode & SPECIAL) | (folder.mode & 0o666) | execute,
        uid: folder.uid,
        gid: folder.gid,
        flags: item.flags,
    }
}

/// Whether the flags are written before the rest: a locked item's permissions and owner cannot
/// change until it is unlocked (and locking comes last).
pub fn flags_first(from: Attrs, to: Attrs) -> bool {
    from.flags & LOCKED != 0 && to.flags & LOCKED == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(mode: u32) -> Attrs {
        Attrs { mode, uid: 501, gid: 20, flags: 0 }
    }

    fn item(name: &str, mode: u32, link: bool) -> (PathBuf, Entry) {
        let id = Identity { dev: 1, ino: name.len() as u64 };
        (PathBuf::from(name), Entry { id, attrs: a(mode), is_dir: false, is_link: link })
    }

    #[test]
    fn boxes_show_each_bit_over_every_item() {
        let states = perm_states(&[a(0o755), a(0o644)]);
        assert_eq!(states.map(Tri::index), [1, 1, 2, 1, 0, 2, 1, 0, 2]);
        assert_eq!(perm_states(&[]).map(Tri::index), [0; 9], "nothing: all off");
        assert_eq!(flag_state(&[Attrs { flags: HIDDEN, ..a(0o644) }, a(0o644)], HIDDEN), Tri::Mixed);
        assert_eq!(flag_state(&[a(0o644)], LOCKED), Tri::Off);
        assert!(Tri::Mixed.clicked() && Tri::Off.clicked() && !Tri::On.clicked(), "a mixed box turns on");
    }

    #[test]
    fn special_bits_never_show_in_the_boxes() {
        assert_eq!(perm_states(&[a(0o7000)]).map(Tri::index), [0; 9]);
        assert_eq!(tri([true, true]), Tri::On);
        assert_eq!(tri([false]), Tri::Off);
        assert_eq!(tri([]), Tri::Off);
    }

    #[test]
    fn a_change_touches_only_its_part() {
        assert_eq!(Change::mode(0o700).apply(Attrs { mode: 0o4755, ..a(0) }).mode, 0o4700, "special bits stay");
        assert_eq!(Change::bit(0o002, true).apply(a(0o644)).mode, 0o646);
        assert_eq!(Change::bit(0o400, false).apply(a(0o644)).mode, 0o244);
        assert_eq!(Change::bit(0o4000, true).apply(a(0o644)).mode, 0o644, "never a special bit");
        let owned = Change::owner(0).apply(a(0o644));
        assert_eq!((owned.uid, owned.gid, owned.mode), (0, 20, 0o644));
        assert_eq!(Change::group(80).apply(a(0o644)).gid, 80);
        let hidden = Change::flag(HIDDEN, true).apply(Attrs { flags: LOCKED, ..a(0o644) });
        assert_eq!(hidden.flags, HIDDEN | LOCKED);
        assert_eq!(Change::flag(LOCKED, false).apply(hidden).flags, HIDDEN);
        assert_eq!(Change::flag(0x4000_0000, true).apply(a(0o644)).flags, 0, "only Hidden and Locked");
        assert_eq!(Change::default().apply(a(0o644)), a(0o644));
    }

    #[test]
    fn no_change_ever_sets_or_clears_a_special_bit() {
        let changes = [
            Change::mode(0o000),
            Change::mode(0o777),
            Change::mode(0o7777),
            Change::bit(0o4000, false),
            Change::bit(0o2000, true),
            Change::bit(0o1000, false),
            Change::bit(0o7777, true),
            Change::bit(0o7777, false),
            Change::owner(0),
            Change::group(0),
            Change::flag(LOCKED, true),
        ];
        for special in [0, 0o4000, 0o2000, 0o1000, 0o7000] {
            for change in changes {
                let to = change.apply(a(special | 0o640));
                assert_eq!(to.mode & SPECIAL, special, "{change:?} on {special:o}");
            }
        }
    }

    #[test]
    fn wanted_keeps_the_window_order_and_skips_what_does_not_change() {
        let items = [item("/d/a", 0o644, false), item("/d/bb", 0o664, false), item("/d/ccc", 0o600, false)];
        let list = wanted(&items, Change::bit(0o020, true));
        let paths: Vec<&str> = list.iter().map(|w| w.path.to_str().unwrap()).collect();
        assert_eq!(paths, ["/d/a", "/d/ccc"], "the one that has it already is left out");
        assert_eq!((list[0].from.mode, list[0].to.mode, list[0].id), (0o644, 0o664, items[0].1.id));
        assert_eq!(list[1].to.mode, 0o620);
        assert!(wanted(&items, Change::default()).is_empty());
    }

    #[test]
    fn a_mixed_selection_gets_one_octal_value_and_keeps_each_special_bit() {
        let items = [item("/d/a", 0o4755, false), item("/d/bb", 0o2700, false), item("/d/ccc", 0o1777, false)];
        let modes: Vec<u32> = wanted(&items, Change::mode(0o750)).iter().map(|w| w.to.mode).collect();
        assert_eq!(modes, [0o4750, 0o2750, 0o1750]);
        let froms: Vec<u32> = wanted(&items, Change::mode(0o750)).iter().map(|w| w.from.mode).collect();
        assert_eq!(froms, [0o4755, 0o2700, 0o1777], "from is what was read");
    }

    #[test]
    fn a_link_keeps_its_permissions_and_flags_but_takes_an_owner() {
        let link = [item("/d/l", 0o777, true)];
        assert!(wanted(&link, Change::bit(0o002, false)).is_empty());
        assert!(wanted(&link, Change::flag(HIDDEN, true)).is_empty());
        let to = wanted(&link, Change::group(80));
        assert_eq!((to.len(), to[0].to.gid, to[0].to.mode), (1, 80, 0o777));
        let both = Change { uid: Some(0), ..Change::mode(0o700) };
        let to = wanted(&link, both);
        assert_eq!((to[0].to.uid, to[0].to.mode), (0, 0o777), "the owner part only");
    }

    #[test]
    fn locked_items_and_links_lock_their_fields() {
        let locked = (PathBuf::from("/d/x"), Entry { attrs: Attrs { flags: LOCKED, ..a(0o644) }, ..Entry::default() });
        assert_eq!(editable(&[item("/d/a", 0o644, false)]), (true, true));
        assert_eq!(editable(&[item("/d/a", 0o644, false), item("/d/l", 0o777, true)]), (false, true));
        assert_eq!(editable(&[item("/d/a", 0o644, false), locked]), (false, false));
        assert_eq!(editable(&[]), (true, true));
    }

    #[test]
    fn octal_field_reads_three_digits_and_never_special_bits() {
        assert_eq!(parse_octal("755"), Ok(0o755));
        assert_eq!(parse_octal(" 0644 "), Ok(0o644));
        assert_eq!(parse_octal("000"), Ok(0));
        assert_eq!(parse_octal("0000"), Ok(0));
        assert_eq!(parse_octal("777"), Ok(0o777));
        assert!(parse_octal("4755").unwrap_err().contains("Setuid"));
        assert!(parse_octal("1777").unwrap_err().contains("Setuid"));
        assert!(parse_octal("7000").unwrap_err().contains("Setuid"));
        for bad in [
            "",
            "  ",
            "75",
            "758",
            "75x",
            "abc",
            "07555",
            "+755",
            "-755",
            "7 55",
            "0o755",
            "٧٥٥",
            "７５５",
            "8",
            "1000000000000",
        ] {
            assert!(parse_octal(bad).is_err(), "{bad}");
        }
        assert_eq!(octal_text(&[a(0o755), a(0o4755)]), "755", "special bits are apart");
        assert_eq!(octal_text(&[a(0o755), a(0o644)]), "");
        assert_eq!(octal_text(&[]), "");
        assert_eq!(octal_text(&[a(0o007)]), "007", "three digits always");
        assert_eq!(special_text(&[a(0o4755), a(0o1777)]), "setuid, sticky");
        assert_eq!(special_text(&[a(0o7755)]), "setuid, setgid, sticky");
        assert_eq!(special_text(&[a(0o755)]), "");
    }

    #[test]
    fn owners_and_groups_by_name_or_number() {
        let users = vec![("root".to_owned(), 0), ("ayse".to_owned(), 501)];
        assert_eq!(parse_id("ayse", &users, "user"), Ok(501));
        assert_eq!(parse_id(" 502 ", &users, "user"), Ok(502), "a number needs no name");
        assert_eq!(parse_id("Ayse", &users, "user"), Err("No user is named \"Ayse\"".to_owned()));
        assert!(parse_id("", &users, "group").unwrap_err().contains("group"));
        assert_eq!(name_of(501, &users), "ayse");
        assert_eq!(name_of(77, &users), "77");
        assert_eq!(shared_name([501, 501], &users), "ayse");
        assert_eq!(shared_name([501, 0], &users), "");
        assert_eq!(shared_name(Vec::<u32>::new(), &users), "");
    }

    #[test]
    fn names_with_digits_or_letters_beyond_ascii() {
        let users = vec![
            ("user1".to_owned(), 1001),
            ("1000".to_owned(), 1002),
            ("ayşe".to_owned(), 1003),
            ("ÇAĞRI".to_owned(), 1004),
            ("www-data".to_owned(), 33),
        ];
        assert_eq!(parse_id("user1", &users, "user"), Ok(1001));
        assert_eq!(parse_id("1000", &users, "user"), Ok(1002), "a name made of digits wins, as in chown");
        assert_eq!(parse_id("ayşe", &users, "user"), Ok(1003));
        assert_eq!(parse_id(" ÇAĞRI ", &users, "user"), Ok(1004));
        assert_eq!(parse_id("www-data", &users, "user"), Ok(33));
        assert!(parse_id("ayse", &users, "user").is_err(), "no folding of ş");
        assert!(parse_id("-1", &users, "user").is_err());
        assert!(parse_id("4294967295", &users, "user").is_err(), "-1 means \"leave as is\" to chown");
        assert!(parse_id("4294967296", &users, "user").is_err());
        assert_eq!(parse_id("4294967294", &users, "user"), Ok(4_294_967_294));
        assert_eq!(name_of(1003, &users), "ayşe");
    }

    #[test]
    fn enclosed_items_follow_chmods_capital_x() {
        let folder = Attrs { mode: 0o750, uid: 501, gid: 30, flags: 0 };
        let file = |mode| enclosed_target(folder, Attrs { mode, uid: 0, gid: 0, flags: HIDDEN }, false);
        assert_eq!(file(0o644), Attrs { mode: 0o640, uid: 501, gid: 30, flags: HIDDEN }, "no file becomes runnable");
        assert_eq!(file(0o700).mode, 0o750, "a runnable file stays runnable, as the folder allows");
        assert_eq!(file(0o4755).mode, 0o4750, "special bits stay");
        assert_eq!(enclosed_target(folder, a(0o700), true).mode, 0o750, "folders as the folder");
        let setgid_folder = Attrs { mode: 0o2775, ..folder };
        assert_eq!(
            enclosed_target(setgid_folder, a(0o644), false).mode,
            0o664,
            "the folder's own special bits do not spread"
        );
        assert_eq!(enclosed_target(setgid_folder, a(0o1700), true).mode, 0o1775);
    }

    #[test]
    fn unlocking_is_written_first() {
        let locked = Attrs { flags: LOCKED, ..a(0o644) };
        assert!(flags_first(locked, a(0o600)));
        assert!(!flags_first(a(0o644), locked), "locking comes last");
        assert!(!flags_first(locked, Attrs { mode: 0o600, ..locked }));
    }
}
