//! Undo and redo stacks.

use std::collections::VecDeque;

/// What can be undone and redone, at most `cap` deep (the oldest falls off).
pub struct UndoStack<T> {
    undo: VecDeque<T>,
    redo: Vec<T>,
    cap: usize,
}

impl<T> UndoStack<T> {
    pub fn new(cap: usize) -> UndoStack<T> {
        UndoStack { undo: VecDeque::new(), redo: Vec::new(), cap: cap.max(1) }
    }

    /// A new action: it can be undone; what was undone before can no longer be redone.
    pub fn push_new(&mut self, item: T) {
        self.redo.clear();
        self.push_undo(item);
    }

    /// The result of an undo: it can be redone.
    pub fn push_undone(&mut self, item: T) {
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

    fn push_undo(&mut self, item: T) {
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
    fn the_oldest_falls_off() {
        let mut s = UndoStack::new(2);
        for i in 0..5 {
            s.push_new(i);
        }
        assert_eq!((s.pop_undo(), s.pop_undo(), s.pop_undo()), (Some(4), Some(3), None));
    }
}
