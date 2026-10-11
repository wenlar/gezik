//! Choosing duplicates to delete (spec 11 §9.3): the rows "select all but one" picks in each
//! group. Pure: the rows come in as their group, folder, name and time.

use std::collections::BTreeMap;
use std::time::SystemTime;

/// A row of a duplicates search, as shown.
#[derive(Debug, Clone, Copy)]
pub struct Member<'a> {
    pub row: usize,
    pub group: u32,
    pub folder: &'a str,
    pub name: &'a str,
    pub modified: Option<SystemTime>,
}

/// Which copy of each group stays unselected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    Newest,
    Oldest,
    /// The first by path (folder, then name).
    First,
    /// Every copy in the folder of this row is selected, none elsewhere.
    ThisFolder(usize),
    /// In this row's group, every copy but this row.
    ThisOne(usize),
}

/// The rows `keep` selects (ascending) and how many groups had every copy in the folder (one
/// of each then stays unselected). No group ever has every copy selected.
pub fn pick(members: &[Member<'_>], keep: Keep) -> (Vec<usize>, usize) {
    let mut groups: BTreeMap<u32, Vec<&Member<'_>>> = BTreeMap::new();
    for m in members {
        groups.entry(m.group).or_default().push(m);
    }
    let at = |row: usize| members.iter().find(|m| m.row == row);
    let mut picked = Vec::new();
    let mut whole = 0;
    for group in groups.values() {
        let kept: Option<usize> = match keep {
            Keep::Newest => group.iter().max_by_key(|m| m.modified).map(|m| m.row),
            Keep::Oldest => group.iter().min_by_key(|m| m.modified).map(|m| m.row),
            Keep::First => group.iter().min_by_key(|m| (m.folder, m.name)).map(|m| m.row),
            Keep::ThisOne(row) => {
                if at(row).is_none_or(|m| m.group != group[0].group) {
                    continue;
                }
                Some(row)
            }
            Keep::ThisFolder(row) => {
                let Some(folder) = at(row).map(|m| m.folder) else { continue };
                let here: Vec<usize> = group.iter().filter(|m| m.folder == folder).map(|m| m.row).collect();
                if here.len() == group.len() {
                    whole += 1;
                    picked.extend(&here[..here.len() - 1]);
                } else {
                    picked.extend(here);
                }
                continue;
            }
        };
        picked.extend(group.iter().map(|m| m.row).filter(|row| Some(*row) != kept));
    }
    picked.sort_unstable();
    (picked, whole)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(secs: u64) -> Option<SystemTime> {
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
    }

    /// Group 0: a/x (old), b/x (new), a/y; group 1: b/z, c/z (no time); group 2: d/w alone.
    fn rows() -> Vec<Member<'static>> {
        let m = |row, group, folder, name, modified| Member { row, group, folder, name, modified };
        vec![
            m(0, 0, "a", "x", at(10)),
            m(1, 0, "b", "x", at(30)),
            m(2, 0, "a", "y", at(20)),
            m(3, 1, "b", "z", at(5)),
            m(4, 1, "c", "z", None),
            m(5, 2, "d", "w", at(1)),
        ]
    }

    #[test]
    fn all_but_the_newest_oldest_or_first() {
        assert_eq!(pick(&rows(), Keep::Newest), (vec![0, 2, 4], 0));
        assert_eq!(pick(&rows(), Keep::Oldest), (vec![1, 2, 3], 0), "no time counts as oldest");
        assert_eq!(pick(&rows(), Keep::First), (vec![1, 2, 4], 0), "a/x first by path");
    }

    #[test]
    fn this_folder_never_takes_every_copy() {
        assert_eq!(pick(&rows(), Keep::ThisFolder(0)), (vec![0, 2], 0), "group 0 keeps b/x");
        // b holds b/x (group 0) and b/z (group 1): neither group is whole there.
        assert_eq!(pick(&rows(), Keep::ThisFolder(1)), (vec![1, 3], 0));
        let mut all_in_a = rows();
        all_in_a[1].folder = "a";
        assert_eq!(pick(&all_in_a, Keep::ThisFolder(0)), (vec![0, 1], 1), "the last copy stays");
        assert_eq!(pick(&rows(), Keep::ThisFolder(5)), (vec![], 1), "a lone copy is kept");
    }

    #[test]
    fn keep_this_one_takes_the_rest_of_its_group() {
        assert_eq!(pick(&rows(), Keep::ThisOne(2)), (vec![0, 1], 0));
        assert_eq!(pick(&rows(), Keep::ThisOne(4)), (vec![3], 0));
        assert_eq!(pick(&rows(), Keep::ThisOne(9)), (vec![], 0), "no such row");
        assert_eq!(pick(&[], Keep::Newest), (vec![], 0));
    }
}
