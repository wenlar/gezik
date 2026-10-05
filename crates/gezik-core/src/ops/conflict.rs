//! What happens when the target of a copy or move already exists.

use std::time::{Duration, SystemTime};

/// What is known about one side of a conflict (or about an item being copied).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Facts {
    pub is_dir: bool,
    /// 0 for folders.
    pub size: u64,
    pub modified: Option<SystemTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    /// Two files.
    File,
    /// Two folders: they are merged, their contents meet one by one.
    Folder,
    /// A file and a folder with the same name.
    Mismatch,
}

pub fn kind_of(source: Facts, target: Facts) -> ConflictKind {
    match (source.is_dir, target.is_dir) {
        (false, false) => ConflictKind::File,
        (true, true) => ConflictKind::Folder,
        _ => ConflictKind::Mismatch,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Decision {
    Replace,
    #[default]
    Skip,
    /// Keep the existing one; the new one gets `name (2)`.
    KeepBoth,
    /// Replace only if the source is newer.
    IfNewer,
    /// Folders only: merge (the only choice).
    Merge,
}

impl Decision {
    pub fn label(self) -> &'static str {
        match self {
            Decision::Replace => "Replace",
            Decision::Skip => "Skip",
            Decision::KeepBoth => "Keep both",
            Decision::IfNewer => "If newer",
            Decision::Merge => "Merge",
        }
    }
}

/// Modification times this close count as the same: FAT and exFAT store them in 2 second steps.
const SAME_TIME: Duration = Duration::from_secs(2);

fn same_time(a: SystemTime, b: SystemTime) -> bool {
    let gap = a.duration_since(b).or_else(|_| b.duration_since(a)).unwrap_or(Duration::MAX);
    gap <= SAME_TIME
}

/// Two files of the same size changed at the same time: very likely the same file.
pub fn identical(source: Facts, target: Facts) -> bool {
    !source.is_dir
        && !target.is_dir
        && source.size == target.size
        && matches!((source.modified, target.modified), (Some(a), Some(b)) if same_time(a, b))
}

/// `Some(true)` if the source is newer, `Some(false)` if the target is; `None` if they are
/// the same age or a time is unknown.
pub fn source_newer(source: Facts, target: Facts) -> Option<bool> {
    let (a, b) = (source.modified?, target.modified?);
    if same_time(a, b) { None } else { Some(a > b) }
}

/// The decisions offered for a conflict of `kind`.
pub fn choices(kind: ConflictKind) -> &'static [Decision] {
    match kind {
        ConflictKind::File => &[Decision::Replace, Decision::Skip, Decision::KeepBoth, Decision::IfNewer],
        ConflictKind::Folder => &[Decision::Merge],
        ConflictKind::Mismatch => &[Decision::Skip, Decision::KeepBoth],
    }
}

/// What a conflict starts with: nothing is ever replaced unless the user says so.
pub fn default_decision(kind: ConflictKind, _source: Facts, _target: Facts) -> Decision {
    match kind {
        ConflictKind::Folder => Decision::Merge,
        ConflictKind::File | ConflictKind::Mismatch => Decision::Skip,
    }
}

/// What the engine does with an item once its conflict is decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// Go ahead (a merged folder: its contents are handled one by one).
    Write,
    /// Move the existing target out of the way (to the trash), then go ahead.
    Replace,
    Skip,
    /// Go ahead under a free `name (n)`.
    Rename,
}

pub fn resolve(decision: Decision, kind: ConflictKind, source: Facts, target: Facts) -> Resolution {
    match (kind, decision) {
        (ConflictKind::Folder, _) => Resolution::Write,
        (_, Decision::KeepBoth) => Resolution::Rename,
        (ConflictKind::File, Decision::Replace) => Resolution::Replace,
        (ConflictKind::File, Decision::IfNewer) if source_newer(source, target) == Some(true) => Resolution::Replace,
        _ => Resolution::Skip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(size: u64, secs: u64) -> Facts {
        Facts { is_dir: false, size, modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs)) }
    }

    fn dir() -> Facts {
        Facts { is_dir: true, size: 0, modified: None }
    }

    #[test]
    fn kinds() {
        assert_eq!(kind_of(file(1, 0), file(2, 0)), ConflictKind::File);
        assert_eq!(kind_of(dir(), dir()), ConflictKind::Folder);
        assert_eq!(kind_of(file(1, 0), dir()), ConflictKind::Mismatch);
        assert_eq!(kind_of(dir(), file(1, 0)), ConflictKind::Mismatch);
    }

    #[test]
    fn identical_allows_fat_time_steps() {
        assert!(identical(file(10, 100), file(10, 101)));
        assert!(!identical(file(10, 100), file(10, 103)));
        assert!(!identical(file(10, 100), file(11, 100)));
        assert!(!identical(Facts { modified: None, ..file(10, 0) }, file(10, 0)));
        assert!(!identical(dir(), dir()));
    }

    #[test]
    fn newer_side() {
        assert_eq!(source_newer(file(1, 200), file(1, 100)), Some(true));
        assert_eq!(source_newer(file(1, 100), file(1, 200)), Some(false));
        assert_eq!(source_newer(file(1, 100), file(1, 101)), None);
        assert_eq!(source_newer(Facts { modified: None, ..file(1, 0) }, file(1, 0)), None);
    }

    #[test]
    fn default_never_replaces() {
        assert_eq!(default_decision(ConflictKind::File, file(1, 2), file(1, 1)), Decision::Skip);
        assert_eq!(default_decision(ConflictKind::Mismatch, file(1, 0), dir()), Decision::Skip);
        assert_eq!(default_decision(ConflictKind::Folder, dir(), dir()), Decision::Merge);
        assert!(!choices(ConflictKind::Mismatch).contains(&Decision::Replace));
    }

    #[test]
    fn resolutions() {
        let (new, old) = (file(1, 200), file(1, 100));
        assert_eq!(resolve(Decision::Replace, ConflictKind::File, old, new), Resolution::Replace);
        assert_eq!(resolve(Decision::IfNewer, ConflictKind::File, new, old), Resolution::Replace);
        assert_eq!(resolve(Decision::IfNewer, ConflictKind::File, old, new), Resolution::Skip);
        assert_eq!(resolve(Decision::KeepBoth, ConflictKind::Mismatch, old, dir()), Resolution::Rename);
        assert_eq!(resolve(Decision::Replace, ConflictKind::Mismatch, old, dir()), Resolution::Skip);
        assert_eq!(resolve(Decision::Skip, ConflictKind::Folder, dir(), dir()), Resolution::Write);
        assert_eq!(Decision::KeepBoth.label(), "Keep both");
    }
}
