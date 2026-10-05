//! The fix in `vendor/i-slint-core` (see vendor/README.md). Slint's text layout cache drops
//! entries it has not used lately; each entry holds a tracker the renderer depends on, and
//! dropping a tracker unlinked that dependency without a word, so a later change of the
//! text never repainted it (the status bar kept an old selection count).

use i_slint_core::properties::{Property, PropertyTracker};

#[test]
fn dropping_a_tracker_dirties_its_dependents() {
    let outer = Box::pin(<PropertyTracker>::default());
    let inner = Box::pin(<PropertyTracker>::default());
    let text = Box::pin(Property::new(1));

    outer.as_ref().evaluate(|| inner.as_ref().evaluate(|| text.as_ref().get()));
    assert!(!outer.is_dirty());
    drop(inner);
    assert!(outer.is_dirty(), "the outer tracker lost a dependency and may be stale");
}
