//! In which order a set of renames runs so none lands on a name another still holds:
//! chains go from their free end, each cycle (a → b, b → a) goes through one temporary name,
//! and a rename that only changes letter case (where the file system ignores case) always does.

use std::collections::HashMap;
use std::path::PathBuf;

use super::paths::path_key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Rename pair `i` straight to its target.
    Direct(usize),
    /// Rename pair `i` to a temporary name (its source is then free).
    ToTemp(usize),
    /// Rename pair `i` from its temporary name to its target.
    FromTemp(usize),
}

/// The steps for `pairs` (source, target). Pairs whose source and target are the same text
/// are left out. Targets must be distinct (the rename layer refuses duplicates).
pub fn order(pairs: &[(PathBuf, PathBuf)]) -> Vec<Step> {
    let sources: Vec<Vec<String>> = pairs.iter().map(|(s, _)| path_key(s)).collect();
    let targets: Vec<Vec<String>> = pairs.iter().map(|(_, t)| path_key(t)).collect();
    let mut steps = Vec::new();
    let mut pending = vec![false; pairs.len()];
    // Which pending pair still holds each name (by its source).
    let mut holder: HashMap<&[String], usize> = HashMap::new();
    for (i, (source, target)) in pairs.iter().enumerate() {
        if source.as_os_str() == target.as_os_str() {
            continue;
        }
        if sources[i] == targets[i] {
            // Only the letter case changes: through a temporary name, nothing waits on it.
            steps.push(Step::ToTemp(i));
            steps.push(Step::FromTemp(i));
            continue;
        }
        pending[i] = true;
        holder.insert(sources[i].as_slice(), i);
    }
    // Which pending pair wants each name (targets are distinct).
    let waiting: HashMap<&[String], usize> =
        (0..pairs.len()).filter(|&i| pending[i]).map(|i| (targets[i].as_slice(), i)).collect();
    // Pairs whose source went to a temporary name and that still wait for their target.
    let mut in_temp = vec![false; pairs.len()];
    // A stack: reversed, so independent renames pop in their given order.
    let mut ready: Vec<usize> = (0..pairs.len())
        .rev()
        .filter(|&i| pending[i] && holder.get(targets[i].as_slice()).is_none_or(|&j| j == i))
        .collect();
    loop {
        // Everything whose target is free goes; each that goes may free the one waiting for it.
        while let Some(i) = ready.pop() {
            if !pending[i] {
                continue;
            }
            pending[i] = false;
            steps.push(if in_temp[i] { Step::FromTemp(i) } else { Step::Direct(i) });
            if !in_temp[i] {
                holder.remove(sources[i].as_slice());
                if let Some(&next) = waiting.get(sources[i].as_slice())
                    && pending[next]
                {
                    ready.push(next);
                }
            }
        }
        // What is left is in cycles: one of each leaves for a temporary name, which frees
        // the pair waiting for its name; the cycle then unwinds back to it.
        let Some(first) = (0..pairs.len()).find(|&i| pending[i] && !in_temp[i]) else { break };
        steps.push(Step::ToTemp(first));
        in_temp[first] = true;
        holder.remove(sources[first].as_slice());
        if let Some(&next) = waiting.get(sources[first].as_slice())
            && pending[next]
        {
            ready.push(next);
        }
        // `first` itself goes once its target is free: when the pair holding it has gone.
        let blocker = holder.get(targets[first].as_slice()).copied();
        if blocker.is_none() {
            ready.push(first);
        }
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(PathBuf, PathBuf)> {
        list.iter().map(|(a, b)| (PathBuf::from(a), PathBuf::from(b))).collect()
    }

    /// Runs `steps` on a set of names; panics if a step lands on a held name.
    fn simulate(pairs: &[(PathBuf, PathBuf)], steps: &[Step]) -> Vec<String> {
        let mut held: Vec<Vec<String>> = pairs.iter().map(|(s, _)| path_key(s)).collect();
        let mut temp = vec![false; pairs.len()];
        let others = |held: &[Vec<String>], i: usize, key: &Vec<String>| {
            held.iter().enumerate().any(|(j, h)| j != i && h == key)
        };
        for step in steps {
            match *step {
                Step::Direct(i) | Step::FromTemp(i) => {
                    let target = path_key(&pairs[i].1);
                    assert!(!others(&held, i, &target), "{step:?} lands on a held name");
                    assert_eq!(matches!(step, Step::FromTemp(_)), temp[i]);
                    held[i] = target;
                    temp[i] = false;
                }
                Step::ToTemp(i) => {
                    held[i] = vec![format!("temp{i}")];
                    temp[i] = true;
                }
            }
        }
        assert!(temp.iter().all(|t| !t), "something stayed under a temporary name");
        held.iter().map(|k| k.join("/")).collect()
    }

    #[test]
    fn independent_renames_go_straight() {
        let p = pairs(&[("/d/a", "/d/x"), ("/d/b", "/d/y")]);
        let steps = order(&p);
        assert_eq!(steps, [Step::Direct(0), Step::Direct(1)]);
        simulate(&p, &steps);
    }

    #[test]
    fn a_chain_starts_at_its_free_end() {
        // a → b, b → c: b must leave first.
        let p = pairs(&[("/d/a", "/d/b"), ("/d/b", "/d/c")]);
        let steps = order(&p);
        assert_eq!(steps, [Step::Direct(1), Step::Direct(0)]);
        simulate(&p, &steps);
    }

    #[test]
    fn a_swap_goes_through_one_temporary_name() {
        let p = pairs(&[("/d/a", "/d/b"), ("/d/b", "/d/a")]);
        let steps = order(&p);
        assert_eq!(steps.iter().filter(|s| matches!(s, Step::ToTemp(_))).count(), 1);
        simulate(&p, &steps);
    }

    #[test]
    fn a_long_cycle_and_a_chain_into_it() {
        // a → b → c → a, and d → e (free).
        let p = pairs(&[("/d/a", "/d/b"), ("/d/b", "/d/c"), ("/d/c", "/d/a"), ("/d/d", "/d/e")]);
        let steps = order(&p);
        assert_eq!(steps.iter().filter(|s| matches!(s, Step::ToTemp(_))).count(), 1);
        simulate(&p, &steps);
    }

    #[test]
    fn same_text_is_left_out() {
        assert!(order(&pairs(&[("/d/a", "/d/a")])).is_empty());
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn case_only_goes_through_a_temporary_name() {
        let p = pairs(&[("/d/foto.JPG", "/d/foto.jpg")]);
        assert_eq!(order(&p), [Step::ToTemp(0), Step::FromTemp(0)]);
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn case_insensitive_cycle() {
        // a.txt → B.txt while b.txt → a.txt: on these systems that is a swap.
        let p = pairs(&[("/d/a.txt", "/d/B.txt"), ("/d/b.txt", "/d/a.txt")]);
        let steps = order(&p);
        assert_eq!(steps.iter().filter(|s| matches!(s, Step::ToTemp(_))).count(), 1);
        simulate(&p, &steps);
    }
}
