//! Names for new things: the user's templates as New ▸ lists them (spec 8.1), pasted files
//! (spec 9.1) and links (spec 9.2).

use crate::batch::date::DateParts;
use crate::ops::names::split_name;

/// The most templates New ▸ lists.
pub const TEMPLATE_MAX: usize = 50;

/// An entry of the templates folder: its file name, its menu label, whether it is a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub name: String,
    pub label: String,
    pub is_dir: bool,
}

/// The menu label of template `name`: a file without its extension (`Report.docx` →
/// `Report`, `backup.tar.gz` → `backup`), a folder as it is.
pub fn template_label(name: &str, is_dir: bool) -> String {
    split_name(name, is_dir).0.to_owned()
}

/// What New ▸ lists from the templates folder's entries, given as (name, is a folder, has
/// the hidden attribute): no names starting with a dot, no hidden ones; by label, case
/// ignored; at most `TEMPLATE_MAX`.
pub fn template_list(entries: impl IntoIterator<Item = (String, bool, bool)>) -> Vec<Template> {
    let mut list: Vec<Template> = entries
        .into_iter()
        .filter(|(name, _, hidden)| !hidden && !name.is_empty() && !name.starts_with('.'))
        .map(|(name, is_dir, _)| Template { label: template_label(&name, is_dir), name, is_dir })
        .collect();
    list.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()).then_with(|| a.name.cmp(&b.name)));
    list.truncate(TEMPLATE_MAX);
    list
}

/// What the clipboard holds besides files, that paste can write as a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteKind {
    Image,
    Text,
}

/// `Pasted image 2026-10-08 14.05.09.png`: local time, `.` for `:` (Windows takes no `:` in
/// a name).
pub fn pasted_name(kind: PasteKind, at: &DateParts) -> String {
    let (what, ext) = match kind {
        PasteKind::Image => ("image", "png"),
        PasteKind::Text => ("text", "txt"),
    };
    format!(
        "Pasted {what} {:04}-{:02}-{:02} {:02}.{:02}.{:02}.{ext}",
        at.year, at.month, at.day, at.hour, at.minute, at.second
    )
}

/// A link Gezik makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// A Windows shortcut (`.lnk`).
    Shortcut,
    /// A Windows junction: a folder that leads to another folder.
    Junction,
    Symlink,
}

impl LinkKind {
    /// What a drop with the link keys makes: a shortcut on Windows (as Explorer), a symbolic
    /// link elsewhere.
    pub fn for_drops() -> LinkKind {
        if cfg!(windows) { LinkKind::Shortcut } else { LinkKind::Symlink }
    }
}

/// A link's name for an item named `name`: `rapor.pdf - Shortcut.lnk` (Explorer),
/// `Link to rapor.pdf` (Nautilus; the extension stays at the end).
pub fn link_name(name: &str, kind: LinkKind) -> String {
    match kind {
        LinkKind::Shortcut => format!("{name} - Shortcut.lnk"),
        LinkKind::Junction | LinkKind::Symlink => format!("Link to {name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::names::numbered;

    fn parts() -> DateParts {
        DateParts { year: 2026, month: 10, day: 8, hour: 14, minute: 5, second: 9 }
    }

    #[test]
    fn templates_are_listed_by_label_without_hidden_ones() {
        let entries = [
            ("Report.docx", false, false),
            (".git", true, false),
            ("desktop.ini", false, true),
            ("Project", true, false),
            ("budget.xlsx", false, false),
            ("backup.tar.gz", false, false),
            ("v1.2", true, false),
        ];
        let list = template_list(entries.iter().map(|(n, d, h)| ((*n).to_owned(), *d, *h)));
        let labels: Vec<&str> = list.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(labels, ["backup", "budget", "Project", "Report", "v1.2"], "a folder keeps its dots");
        assert_eq!(list[3].name, "Report.docx");
        assert!(list[2].is_dir);
        let many = (0..60).map(|i| (format!("t{i:02}.txt"), false, false));
        let list = template_list(many);
        assert_eq!(list.len(), TEMPLATE_MAX);
        assert_eq!(list.last().unwrap().label, "t49");
    }

    #[test]
    fn pasted_names_carry_the_local_time_without_colons() {
        assert_eq!(pasted_name(PasteKind::Image, &parts()), "Pasted image 2026-10-08 14.05.09.png");
        assert_eq!(pasted_name(PasteKind::Text, &parts()), "Pasted text 2026-10-08 14.05.09.txt");
        assert!(!pasted_name(PasteKind::Text, &parts()).contains(':'));
        // A taken name gets its number before the extension (the engine's Keep both).
        assert_eq!(
            numbered(&pasted_name(PasteKind::Image, &parts()), false, 2),
            "Pasted image 2026-10-08 14.05.09 (2).png"
        );
    }

    #[test]
    fn links_are_named_as_explorer_and_nautilus_name_them() {
        assert_eq!(link_name("rapor.pdf", LinkKind::Shortcut), "rapor.pdf - Shortcut.lnk");
        assert_eq!(link_name("Docs", LinkKind::Junction), "Link to Docs");
        assert_eq!(link_name("rapor.pdf", LinkKind::Symlink), "Link to rapor.pdf");
        assert_eq!(numbered("Link to rapor.pdf", false, 2), "Link to rapor (2).pdf");
        assert_eq!(numbered("rapor.pdf - Shortcut.lnk", false, 2), "rapor.pdf - Shortcut (2).lnk");
        let expected = if cfg!(windows) { LinkKind::Shortcut } else { LinkKind::Symlink };
        assert_eq!(LinkKind::for_drops(), expected);
    }
}
