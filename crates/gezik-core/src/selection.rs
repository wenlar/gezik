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
