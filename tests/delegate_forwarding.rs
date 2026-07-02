//! Regression: `delegate_widget_to!` must forward `compose` so a delegated
//! wrapper surfaces its inner container's per-child declaration metadata.
//!
//! A delegated wrapper (e.g. `Vertical`, which wraps a `Container`) receives
//! per-child CSS id/class metadata via `with_compose`. Under RA2.1 that metadata
//! is bundled directly into each `ChildDecl` its `compose()` emits (there is no
//! separate side-channel drain), and the delegation macro forwards `compose`, so
//! a `Vertical::with_compose(...)` must yield fully-formed child declarations.

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

    // The Vertical wrapper must surface its inner Container's declared children
    // through the forwarded compose(), each ChildDecl carrying its own id/classes.
    let out = wrapper.compose();
    assert_eq!(
        out.len(),
        2,
        "delegated wrapper must forward composed child declarations (got {})",
        out.len()
    );

    assert_eq!(out[0].id(), Some("first-id"));
    assert_eq!(out[0].classes(), &["alpha".to_string(), "beta".to_string()]);

    assert_eq!(out[1].id(), None);
    assert_eq!(out[1].classes(), &["gamma".to_string()]);
}
