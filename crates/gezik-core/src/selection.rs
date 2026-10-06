//! Which entries are selected, plus the anchor (where a Shift range starts) and the
//! keyboard focus. One bit per entry, so 100k entries take about 12 KB. Every change
//! returns the rows whose look changed (selection or focus), so the UI redraws only those.

use std::ops::Range;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    bits: Vec<u64>,
    len: usize,
    count: usize,
    anchor: Option<usize>,
    focus: Option<usize>,
}

impl Selection {
    pub fn new(len: usize) -> Selection {
        Selection { bits: vec![0; len.div_ceil(64)], len, ..Selection::default() }
    }

    /// `indices` selected (those out of range are ignored), focus and anchor on `focus`.
    pub fn from_indices(len: usize, indices: impl IntoIterator<Item = usize>, focus: Option<usize>) -> Selection {
        let mut s = Selection::new(len);
        for i in indices {
            if i < len {
                s.set(i, true);
            }
        }
        s.focus = focus.filter(|f| *f < len);
        s.anchor = s.focus;
        s
    }

    /// This selection moved to another list of the same entries whose entry `k` is entry
    /// `from[k]` of this one (filtered, sorted again, or with entries taken out): what was
    /// selected stays selected, and the focus (and anchor) follows its entry, or goes to the
    /// first selected one if it is gone. Indices past this list are skipped. No name is
    /// compared, and with nothing selected only the focus is looked for.
    pub fn carried(&self, from: &[usize]) -> Selection {
        let mut s = Selection::new(from.len());
        let mut focus = None;
        if self.count > 0 || self.focus.is_some() {
            for (k, &i) in from.iter().enumerate() {
                if self.count > 0 && self.is_selected(i) {
                    s.set(k, true);
                }
                if self.focus == Some(i) {
                    focus = Some(k);
                }
            }
        }
        s.focus = focus.or_else(|| s.iter().next());
        s.anchor = s.focus;
        s
    }

    /// This selection of a list whose entry `i` is entry `rows[i]` of a list of `len`
    /// entries (a filtered list and the full one), as a selection of that list; focus and
    /// anchor follow their entries.
    pub fn spread(&self, rows: &[usize], len: usize) -> Selection {
        let mut s = Selection::new(len);
        for i in self.iter() {
            if let Some(&row) = rows.get(i)
                && row < len
            {
                s.set(row, true);
            }
        }
        let map = |i: Option<usize>| i.and_then(|i| rows.get(i).copied()).filter(|&row| row < len);
        s.focus = map(self.focus);
        s.anchor = map(self.anchor);
        s
    }

    /// The same selection with the focus and anchor on `focus` (ignored past the end).
    pub fn focused_at(mut self, focus: Option<usize>) -> Selection {
        self.focus = focus.filter(|&f| f < self.len);
        self.anchor = self.focus;
        self
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// How many entries are selected.
    pub fn count(&self) -> usize {
        self.count
    }

    pub fn focus(&self) -> Option<usize> {
        self.focus
    }

    pub fn anchor(&self) -> Option<usize> {
        self.anchor
    }

    pub fn is_selected(&self, i: usize) -> bool {
        i < self.len && self.bits[i / 64] & (1 << (i % 64)) != 0
    }

    /// Selected indices, ascending.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.bits.iter().enumerate().flat_map(|(w, &word)| {
            let mut word = word;
            std::iter::from_fn(move || {
                if word == 0 {
                    return None;
                }
                let bit = word.trailing_zeros() as usize;
                word &= word - 1;
                Some(w * 64 + bit)
            })
        })
    }

    /// Unselects everything; focus and anchor stay.
    pub fn clear(&mut self) -> Vec<Range<usize>> {
        self.change(|s| s.fill(false))
    }

    /// A plain click or arrow key: only `i`, which becomes focus and anchor.
    pub fn select_only(&mut self, i: usize) -> Vec<Range<usize>> {
        if i >= self.len {
            return Vec::new();
        }
        self.change(|s| {
            s.fill(false);
            s.set(i, true);
            s.anchor = Some(i);
            s.focus = Some(i);
        })
    }

    /// Ctrl+click: flips `i`, which becomes focus and anchor.
    pub fn toggle(&mut self, i: usize) -> Vec<Range<usize>> {
        if i >= self.len {
            return Vec::new();
        }
        self.change(|s| {
            let on = !s.is_selected(i);
            s.set(i, on);
            s.anchor = Some(i);
            s.focus = Some(i);
        })
    }

    /// Shift+click or Shift+arrow: the range from the anchor to `i` (from `i` alone if
    /// there is no anchor yet). `keep_others` (Ctrl+Shift) adds the range to the selection
    /// instead of replacing it. The anchor stays; the focus moves to `i`.
    pub fn extend_to(&mut self, i: usize, keep_others: bool) -> Vec<Range<usize>> {
        if i >= self.len {
            return Vec::new();
        }
        self.change(|s| {
            let anchor = s.anchor.filter(|a| *a < s.len).unwrap_or(i);
            if !keep_others {
                s.fill(false);
            }
            for j in anchor.min(i)..=anchor.max(i) {
                s.set(j, true);
            }
            s.anchor = Some(anchor);
            s.focus = Some(i);
        })
    }

    pub fn select_all(&mut self) -> Vec<Range<usize>> {
        self.change(|s| s.fill(true))
    }

    /// Flips every entry; focus and anchor stay.
    pub fn invert(&mut self) -> Vec<Range<usize>> {
        self.change(|s| {
            for word in &mut s.bits {
                *word = !*word;
            }
            // Bits past the end stay clear.
            if !s.len.is_multiple_of(64)
                && let Some(last) = s.bits.last_mut()
            {
                *last &= (1u64 << (s.len % 64)) - 1;
            }
            s.count = s.len - s.count;
        })
    }

    /// Selects (`on`) or unselects each entry `matches` says yes to; the others, focus and
    /// anchor stay.
    pub fn set_where(&mut self, on: bool, mut matches: impl FnMut(usize) -> bool) -> Vec<Range<usize>> {
        self.change(|s| {
            for i in 0..s.len {
                if matches(i) {
                    s.set(i, on);
                }
            }
        })
    }

    /// Becomes `new` (selection, focus and anchor); returns only the rows whose selection or
    /// focus changed (all of them if the length differs).
    pub fn replace(&mut self, new: Selection) -> Vec<Range<usize>> {
        if new.len != self.len {
            let len = self.len.max(new.len);
            *self = new;
            return std::iter::once(0..len).filter(|r| !r.is_empty()).collect();
        }
        self.change(|s| *s = new)
    }

    /// Ctrl+arrow: moves the focus only.
    pub fn set_focus(&mut self, i: usize) -> Vec<Range<usize>> {
        if i >= self.len {
            return Vec::new();
        }
        self.change(|s| s.focus = Some(i))
    }

    /// Ctrl+Space: flips the focused entry, which becomes the anchor.
    pub fn toggle_focus(&mut self) -> Vec<Range<usize>> {
        let Some(f) = self.focus else { return Vec::new() };
        self.change(|s| {
            let on = !s.is_selected(f);
            s.set(f, on);
            s.anchor = Some(f);
        })
    }

    /// A rubber-band drag: `base` (what was selected when the drag started, or nothing)
    /// plus every index in `hits`. Focus and anchor stay.
    pub fn set_rect(&mut self, base: &Selection, hits: &[Range<usize>]) -> Vec<Range<usize>> {
        self.change(|s| {
            s.fill(false);
            let len = s.len;
            for i in base.iter().filter(|i| *i < len) {
                s.set(i, true);
            }
            for range in hits {
                for i in range.start..range.end.min(s.len) {
                    s.set(i, true);
                }
            }
        })
    }

    fn set(&mut self, i: usize, on: bool) {
        let (word, mask) = (i / 64, 1u64 << (i % 64));
        let was = self.bits[word] & mask != 0;
        if on && !was {
            self.bits[word] |= mask;
            self.count += 1;
        } else if !on && was {
            self.bits[word] &= !mask;
            self.count -= 1;
        }
    }

    fn fill(&mut self, on: bool) {
        self.bits.fill(if on { u64::MAX } else { 0 });
        if on
            && !self.len.is_multiple_of(64)
            && let Some(last) = self.bits.last_mut()
        {
            *last = (1u64 << (self.len % 64)) - 1;
        }
        self.count = if on { self.len } else { 0 };
    }

    /// Runs `f` and returns the rows whose selection or focus it changed.
    fn change(&mut self, f: impl FnOnce(&mut Selection)) -> Vec<Range<usize>> {
        let (bits, focus) = (self.bits.clone(), self.focus);
        f(self);
        let mut rows = Vec::new();
        for (w, (old, new)) in bits.iter().zip(&self.bits).enumerate() {
            let mut changed = old ^ new;
            while changed != 0 {
                let i = w * 64 + changed.trailing_zeros() as usize;
                push_index(&mut rows, i);
                changed &= changed - 1;
            }
        }
        if focus != self.focus {
            for i in [focus, self.focus].into_iter().flatten() {
                rows.push(i..i + 1);
            }
        }
        merge(rows)
    }
}

fn push_index(rows: &mut Vec<Range<usize>>, i: usize) {
    match rows.last_mut() {
        Some(last) if last.end == i => last.end += 1,
        _ => rows.push(i..i + 1),
    }
}

/// Sorted, with overlapping or touching ranges joined.
fn merge(mut rows: Vec<Range<usize>>) -> Vec<Range<usize>> {
    rows.sort_by_key(|r| r.start);
    let mut out: Vec<Range<usize>> = Vec::with_capacity(rows.len());
    for r in rows {
        match out.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => out.push(r),
        }
    }
    out
}

/// A left press waiting for its release. Pressing an entry that is already selected (with
/// no Shift) changes nothing yet, so the whole selection can be dragged; a release without a
/// drag then does what the press would have done (plain: only that entry; Ctrl: flip it).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PendingPress {
    waiting: Option<(usize, bool)>,
}

impl PendingPress {
    /// A left press on entry `index`; returns the rows whose look changed.
    pub fn press(&mut self, selection: &mut Selection, index: usize, ctrl: bool, shift: bool) -> Vec<Range<usize>> {
        self.waiting = None;
        match (ctrl, shift) {
            (_, true) => selection.extend_to(index, ctrl),
            (_, false) if selection.is_selected(index) => {
                self.waiting = Some((index, ctrl));
                selection.set_focus(index)
            }
            (true, false) => selection.toggle(index),
            (false, false) => selection.select_only(index),
        }
    }

    /// The left button came up over entry `index`; `dragged`: the press became a drag.
    pub fn release(&mut self, selection: &mut Selection, index: usize, dragged: bool) -> Vec<Range<usize>> {
        match self.waiting.take() {
            Some((pressed, ctrl)) if pressed == index && !dragged => {
                if ctrl {
                    selection.toggle(index)
                } else {
                    selection.select_only(index)
                }
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;

    fn selected(s: &Selection) -> Vec<usize> {
        s.iter().collect()
    }

    #[test]
    fn a_selection_is_carried_by_position_not_by_name() {
        // Shown rows 0..4 are full entries [1, 3, 4, 7] of 9; rows 1 and 3 selected, focus 3.
        let shown = Selection::from_indices(4, [1, 3], Some(3));
        let full = shown.spread(&[1, 3, 4, 7], 9);
        assert_eq!(selected(&full), [3, 7]);
        assert_eq!((full.focus(), full.anchor()), (Some(7), Some(7)));
        // A new filter shows full entries [0, 3, 5, 6, 7].
        let next = full.carried(&[0, 3, 5, 6, 7]);
        assert_eq!((next.len(), selected(&next)), (5, vec![1, 4]));
        assert_eq!(next.focus(), Some(4), "the focus follows its entry");
        // Its focused entry filtered out: the focus goes to the first selected one.
        let gone = full.carried(&[3, 5]);
        assert_eq!((selected(&gone), gone.focus()), (vec![0], Some(0)));
        // A sort is a permutation: entry k now was entry from[k] before.
        let sorted = full.carried(&[8, 7, 6, 5, 4, 3, 2, 1, 0]);
        assert_eq!((selected(&sorted), sorted.focus()), (vec![1, 5], Some(1)));
        let none = Selection::new(9).focused_at(Some(4)).carried(&[4, 5]);
        assert_eq!((none.count(), none.focus()), (0, Some(0)), "nothing selected: only the focus");
        assert_eq!(Selection::new(9).carried(&[1, 2]).focus(), None);
        assert_eq!(Selection::new(3).focused_at(Some(5)).focus(), None, "past the end");
    }

    #[test]
    fn pressing_a_selected_entry_waits_for_the_release() {
        let mut s = Selection::new(5);
        s.select_only(0);
        s.extend_to(2, false);
        let mut pending = PendingPress::default();
        pending.press(&mut s, 1, false, false);
        assert_eq!(s.count(), 3, "nothing is unselected yet");
        assert_eq!(s.focus(), Some(1));
        pending.release(&mut s, 1, true);
        assert_eq!(s.count(), 3, "a drag keeps the selection");
        pending.press(&mut s, 1, false, false);
        pending.release(&mut s, 1, false);
        assert_eq!(selected(&s), vec![1]);
        s.select_all();
        pending.press(&mut s, 3, true, false);
        assert!(s.is_selected(3));
        pending.release(&mut s, 3, false);
        assert!(!s.is_selected(3) && s.count() == 4, "ctrl+click on a selected entry drops it on release");
        pending.press(&mut s, 3, false, false);
        assert_eq!(selected(&s), vec![3], "an unselected entry is selected at once");
        pending.release(&mut s, 3, false);
        assert_eq!(selected(&s), vec![3]);
    }

    #[test]
    fn a_release_elsewhere_does_nothing() {
        let mut s = Selection::new(5);
        s.select_all();
        let mut pending = PendingPress::default();
        pending.press(&mut s, 1, false, false);
        pending.release(&mut s, 2, false);
        assert_eq!(s.count(), 5, "released over another entry: no click");
        pending.release(&mut s, 1, false);
        assert_eq!(s.count(), 5, "a release only answers one press");
    }

    #[test]
    fn click_selects_only_one_and_sets_focus_and_anchor() {
        let mut s = Selection::from_indices(10, [1, 2], Some(2));
        let changed = s.select_only(5);
        assert_eq!(selected(&s), [5]);
        assert_eq!((s.focus(), s.anchor(), s.count()), (Some(5), Some(5), 1));
        assert_eq!(changed, [1..3, 5..6]);
    }

    #[test]
    fn ctrl_click_toggles() {
        let mut s = Selection::new(5);
        s.toggle(1);
        s.toggle(3);
        assert_eq!(selected(&s), [1, 3]);
        assert_eq!(s.toggle(1), [1..2, 3..4], "1 flips; focus moves from 3 to 1");
        assert_eq!(selected(&s), [3]);
    }

    #[test]
    fn shift_click_selects_from_the_anchor() {
        let mut s = Selection::new(10);
        s.select_only(3);
        s.extend_to(6, false);
        assert_eq!(selected(&s), [3, 4, 5, 6]);
        s.extend_to(1, false);
        assert_eq!(selected(&s), [1, 2, 3], "the anchor stays at 3");
        assert_eq!((s.focus(), s.anchor()), (Some(1), Some(3)));
    }

    #[test]
    fn ctrl_shift_click_adds_a_range() {
        let mut s = Selection::new(10);
        s.select_only(0);
        s.toggle(5);
        s.extend_to(7, true);
        assert_eq!(selected(&s), [0, 5, 6, 7]);
    }

    #[test]
    fn shift_without_anchor_starts_at_the_target() {
        let mut s = Selection::new(4);
        s.extend_to(2, false);
        assert_eq!(selected(&s), [2]);
        assert_eq!(s.anchor(), Some(2));
    }

    #[test]
    fn replacing_reports_only_the_rows_that_changed() {
        let mut s = Selection::from_indices(200, [3, 4, 150], Some(3));
        let rows = s.replace(Selection::from_indices(200, [4, 150, 151], Some(4)));
        assert_eq!(rows, [3..5, 151..152], "3 off and its focus moved to 4; 151 on");
        assert_eq!((s.count(), s.focus(), s.anchor()), (3, Some(4), Some(4)));
        assert!(s.replace(s.clone()).is_empty(), "the same: nothing");
        let rows = s.replace(Selection::from_indices(10, [1], None));
        assert_eq!(rows, [0..200], "another length: everything");
        assert_eq!(s.len(), 10);
    }

    #[test]
    fn invert_flips_every_entry_and_reports_the_rows() {
        let mut s = Selection::from_indices(130, [0, 1, 129], Some(1));
        assert_eq!(s.invert(), [0..130]);
        assert_eq!(s.count(), 127);
        assert!(!s.is_selected(0) && s.is_selected(2) && !s.is_selected(129) && !s.is_selected(130));
        assert_eq!((s.focus(), s.anchor()), (Some(1), Some(1)), "focus and anchor stay");
        s.invert();
        assert_eq!(selected(&s), [0, 1, 129]);
        let mut full = Selection::new(128);
        full.select_all();
        full.invert();
        assert_eq!(full.count(), 0);
        assert!(Selection::new(0).invert().is_empty());
    }

    #[test]
    fn set_where_adds_or_removes_only_the_matches() {
        let mut s = Selection::from_indices(10, [1], Some(1));
        assert_eq!(s.set_where(true, |i| i % 3 == 0), [0..1, 3..4, 6..7, 9..10]);
        assert_eq!(selected(&s), [0, 1, 3, 6, 9]);
        assert_eq!(s.set_where(false, |i| i < 4), [0..2, 3..4]);
        assert_eq!(selected(&s), [6, 9]);
        assert!(s.set_where(true, |i| i == 6).is_empty(), "already selected: nothing changes");
        assert_eq!((s.focus(), s.count()), (Some(1), 2));
    }

    #[test]
    fn select_all_reports_one_range_and_counts() {
        let mut s = Selection::new(130);
        assert_eq!(s.select_all(), [0..130]);
        assert_eq!(s.count(), 130);
        assert!(s.is_selected(129) && !s.is_selected(130));
        assert_eq!(s.clear(), [0..130]);
        assert_eq!(s.count(), 0);
    }

    #[test]
    fn focus_moves_without_changing_the_selection() {
        let mut s = Selection::new(5);
        s.select_only(1);
        assert_eq!(s.set_focus(3), [1..2, 3..4], "old and new focus rows redraw");
        assert_eq!(selected(&s), [1]);
        assert_eq!(s.toggle_focus(), [3..4]);
        assert_eq!(selected(&s), [1, 3]);
        assert_eq!(s.anchor(), Some(3));
    }

    #[test]
    fn rect_adds_hits_to_the_base() {
        let base = Selection::from_indices(20, [0], None);
        let mut s = base.clone();
        s.set_rect(&base, &[4..6, 10..12]);
        assert_eq!(selected(&s), [0, 4, 5, 10, 11]);
        let changed = s.set_rect(&base, &[5..6]);
        assert_eq!(selected(&s), [0, 5]);
        assert_eq!(changed, [4..5, 10..12]);
        s.set_rect(&Selection::new(20), &[18..40]);
        assert_eq!(selected(&s), [18, 19], "hits past the end are ignored");
    }

    #[test]
    fn from_indices_ignores_out_of_range() {
        let s = Selection::from_indices(3, [0, 2, 7], Some(9));
        assert_eq!(selected(&s), [0, 2]);
        assert_eq!(s.focus(), None);
        assert_eq!(s.count(), 2);
    }

    #[test]
    fn out_of_range_changes_do_nothing() {
        let mut s = Selection::new(2);
        assert!(s.select_only(2).is_empty());
        assert!(s.toggle(5).is_empty());
        assert!(s.extend_to(9, false).is_empty());
        assert!(s.set_focus(2).is_empty());
        assert!(s.toggle_focus().is_empty());
        let mut empty = Selection::new(0);
        assert!(empty.select_all().is_empty());
        assert!(empty.is_empty());
    }

    #[test]
    fn iterates_across_words() {
        let s = Selection::from_indices(200, [0, 63, 64, 199], None);
        assert_eq!(selected(&s), [0, 63, 64, 199]);
    }
}
