//! Undo and redo stacks.

use std::collections::VecDeque;

/// What can be undone and redone, at most `cap` deep (the oldest falls off).
pub struct UndoStack<T> {
    undo: VecDeque<T>,
    redo: Vec<T>,
    cap: usize,
    /// Counts of what was pushed so far (see [`Stamp`]).
    counts: Stamp,
}

/// How many entries went on each stack, and how many new actions there were, up to a moment:
/// taken when an entry is popped, so it can be put back where it was.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stamp {
    undo_pushes: u64,
    redo_pushes: u64,
    new_actions: u64,
}

impl<T> UndoStack<T> {
    pub fn new(cap: usize) -> UndoStack<T> {
        UndoStack { undo: VecDeque::new(), redo: Vec::new(), cap: cap.max(1), counts: Stamp::default() }
    }

    /// A new action: it can be undone; what was undone before can no longer be redone.
    pub fn push_new(&mut self, item: T) {
        self.redo.clear();
        self.counts.new_actions += 1;
        self.push_undo(item);
    }

    /// The result of an undo: it can be redone.
    pub fn push_undone(&mut self, item: T) {
        self.counts.redo_pushes += 1;
        self.redo.push(item);
    }

    /// The result of a redo: back on the undo stack; the rest stays redoable.
    pub fn push_redone(&mut self, item: T) {
        self.push_undo(item);
    }

    pub fn pop_undo(&mut self) -> Option<T> {
        self.undo.pop_back()
    }

    pub fn pop_redo(&mut self) -> Option<T> {
        self.redo.pop()
    }

    pub fn peek_undo(&self) -> Option<&T> {
        self.undo.back()
    }

    pub fn peek_redo(&self) -> Option<&T> {
        self.redo.last()
    }

    /// Take it before popping an entry that may have to be put back.
    pub fn stamp(&self) -> Stamp {
        self.counts
    }

    /// An undo that did nothing: its entry goes back where it was, below what was pushed on
    /// the undo stack since `stamp`.
    pub fn put_back_undo(&mut self, item: T, stamp: Stamp) {
        let newer = usize::try_from(self.counts.undo_pushes - stamp.undo_pushes).unwrap_or(usize::MAX);
        self.undo.insert(self.undo.len().saturating_sub(newer), item);
        if self.undo.len() > self.cap {
            self.undo.pop_front();
        }
    }

    /// A redo that did nothing: its entry goes back where it was, unless a new action since
    /// `stamp` ended redo.
    pub fn put_back_redo(&mut self, item: T, stamp: Stamp) {
        if self.counts.new_actions != stamp.new_actions {
            return;
        }
        let newer = usize::try_from(self.counts.redo_pushes - stamp.redo_pushes).unwrap_or(usize::MAX);
        self.redo.insert(self.redo.len().saturating_sub(newer), item);
    }

    fn push_undo(&mut self, item: T) {
        self.counts.undo_pushes += 1;
        self.undo.push_back(item);
        if self.undo.len() > self.cap {
            self.undo.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_then_redo() {
        let mut s = UndoStack::new(10);
        s.push_new("a");
        s.push_new("b");
        assert_eq!(s.pop_undo(), Some("b"));
        s.push_undone("b'");
        assert_eq!(s.peek_undo(), Some(&"a"));
        assert_eq!(s.pop_redo(), Some("b'"));
        s.push_redone("b''");
        assert_eq!(s.peek_undo(), Some(&"b''"));
        assert_eq!(s.peek_redo(), None);
    }

    #[test]
    fn a_new_action_drops_the_redo_stack() {
        let mut s = UndoStack::new(10);
        s.push_new(1);
        let undone = s.pop_undo().unwrap();
        s.push_undone(undone);
        s.push_new(2);
        assert_eq!(s.pop_redo(), None);
    }

    #[test]
    fn an_undo_that_did_nothing_goes_back_on_top() {
        let mut s = UndoStack::new(10);
        s.push_new("a");
        s.push_new("b");
        let stamp = s.stamp();
        let b = s.pop_undo().unwrap();
        s.put_back_undo(b, stamp);
        assert_eq!(s.pop_undo(), Some("b"));
        assert_eq!(s.pop_undo(), Some("a"));
    }

    #[test]
    fn an_undo_put_back_goes_below_actions_made_meanwhile() {
        let mut s = UndoStack::new(10);
        s.push_new("a");
        let stamp = s.stamp();
        let a = s.pop_undo().unwrap();
        s.push_new("b");
        s.put_back_undo(a, stamp);
        assert_eq!(s.pop_undo(), Some("b"));
        assert_eq!(s.pop_undo(), Some("a"));
    }

    #[test]
    fn a_redo_that_did_nothing_goes_back_unless_a_new_action_ended_redo() {
        let mut s = UndoStack::new(10);
        s.push_new("a");
        let a = s.pop_undo().unwrap();
        s.push_undone(a);
        let stamp = s.stamp();
        let a = s.pop_redo().unwrap();
        s.put_back_redo(a, stamp);
        assert_eq!(s.peek_redo(), Some(&"a"));

        let stamp = s.stamp();
        let a = s.pop_redo().unwrap();
        s.push_new("b");
        s.put_back_redo(a, stamp);
        assert_eq!(s.peek_redo(), None);
    }

    #[test]
    fn the_oldest_falls_off() {
        let mut s = UndoStack::new(2);
        for i in 0..5 {
            s.push_new(i);
        }
        assert_eq!((s.pop_undo(), s.pop_undo(), s.pop_undo()), (Some(4), Some(3), None));
    }
}
