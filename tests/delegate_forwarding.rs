//! Regression: `delegate_widget_to!` must forward the arena composition hooks
//! (`take_child_decl_meta`, `take_child_handle_sinks`, `take_pending_mount_messages`).
//!
//! A delegated wrapper (e.g. `Vertical`, which wraps a `Container`) records
//! per-child CSS id/class metadata and handle-sinks in its inner container via
//! `with_compose`. Before the fix the delegation macro forwarded
//! `take_composed_children` but NOT the metadata/sink/mount-message drains, so a
//! `Vertical::with_compose(...)` silently dropped declared child ids/classes and
//! handle-sinks at mount — affecting even the initial build path.

use textual::compose::{ChildDecl, ComposeResult};
use textual::prelude::*;
use textual::widgets::Widget;

#[test]
fn delegated_wrapper_forwards_child_decl_meta() {
    let children: ComposeResult = vec![
        ChildDecl::new(Box::new(Label::new("first")))
            .with_id("first-id")
            .with_classes(&["alpha", "beta"]),
        ChildDecl::new(Box::new(Label::new("second"))).with_classes(&["gamma"]),
    ];

    let mut wrapper = Vertical::new().with_compose(children);

    // The inner Container recorded the metadata; the Vertical wrapper must
    // surface it through the delegated drain (it returned [] before the fix).
    let meta = wrapper.take_child_decl_meta();
    assert_eq!(
        meta.len(),
        2,
        "delegated wrapper must forward child decl metadata (got {meta:?})"
    );

    let (idx0, id0, classes0) = &meta[0];
    assert_eq!(*idx0, 0);
    assert_eq!(id0.as_deref(), Some("first-id"));
    assert_eq!(classes0, &["alpha".to_string(), "beta".to_string()]);

    let (idx1, id1, classes1) = &meta[1];
    assert_eq!(*idx1, 1);
    assert_eq!(*id1, None);
    assert_eq!(classes1, &["gamma".to_string()]);
}
