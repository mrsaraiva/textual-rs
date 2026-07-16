//! Programmatic system screen layers (Python `Screen.layers`).
//!
//! Python appends the system layers AFTER the CSS-derived tuple
//! (`screen.py:359-371`: `_loading`, then `_toastrack` and `_tooltips`), so a
//! user writing `Screen { layers: below above; }` can never clobber them:
//! they are not part of the cascade at all. `layers` cascades as a single
//! replace-wins field, so declaring the system layers in the Screen default
//! CSS (the pre-hardening approach) broke exactly that way: the user list
//! REPLACED `_toastrack` and the ToastRack fell into the default bucket.
//!
//! Rust mirrors Python: the Screen default CSS declares NO system layers;
//! every layers-ORDER consumer resolves a node's layer list through
//! [`effective_layers`] / [`effective_layers_with`], which append the extras
//! when the node is the walk root of its tree. Layout grouping, dock
//! isolation, and virtual-extent isolation group by layer NAME only and do
//! not consume the order, so they stay on plain `style.layers`.
//!
//! The extras are deliberately NOT reflected in style serialization/debug
//! dumps: `Style` keeps only the cascaded value, exactly like Python where
//! `styles.layers` and `Screen.layers` differ.

use crate::css::{node_selector_meta, resolve_node_style};
use crate::node_id::NodeId;
use crate::widget_tree::WidgetTree;

/// System layers every screen carries above all user layers, bottom to top
/// (Python `Screen.layers` extras, `screen.py:359-371`).
pub(crate) const SYSTEM_SCREEN_LAYERS: [&str; 3] = ["_loading", "_toastrack", "_tooltips"];

/// The node's effective layer order: its cascaded CSS `layers` value plus,
/// when the node is the walk root, the programmatic system extras.
///
/// Resolves the node's style; callers that already hold the resolved
/// `style.layers` should use [`effective_layers_with`] instead.
pub(crate) fn effective_layers(tree: &WidgetTree, node: NodeId) -> Vec<String> {
    let css_layers = tree.get(node).and_then(|_node| {
        let meta = node_selector_meta(tree, node);
        resolve_node_style(tree, node, &meta).layers
    });
    effective_layers_with(tree, node, css_layers)
}

/// [`effective_layers`] for a caller that already resolved the node's
/// cascaded `layers` value.
///
/// Walk-root detection is keyed off the tree root: both compositor walks
/// (`CompositedLayer::AppRoot` and `::Screen`) start from their tree's root
/// node, and the runtime mounts the system ToastRack/Tooltip on that same
/// root (`App::mount_system_toast_rack` / `mount_system_tooltip`). Keying off
/// the walk root instead of a `Screen` type-name check avoids the
/// Node/AppRoot rename hazard.
pub(crate) fn effective_layers_with(
    tree: &WidgetTree,
    node: NodeId,
    css_layers: Option<Vec<String>>,
) -> Vec<String> {
    let mut layers = css_layers.unwrap_or_default();
    if tree.root() != Some(node) {
        return layers;
    }
    if layers.is_empty() {
        // Python `Widget.layers` defaults to `("default",)`, so with no CSS
        // declaration the screen's list is ("default", extras...): children on
        // the implicit default layer sort strictly BELOW the system layers.
        layers.push("default".to_string());
    }
    // Deduplicate: a cascade that (re)declares a system name keeps the
    // programmatic position (Python builds `layers_to_index` as a dict, so a
    // later duplicate index wins).
    layers.retain(|name| !SYSTEM_SCREEN_LAYERS.contains(&name.as_str()));
    for extra in SYSTEM_SCREEN_LAYERS {
        layers.push(extra.to_string());
    }
    layers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::{AppRoot, Button, Label};

    #[test]
    fn effective_layers_appends_system_extras_after_user_layers_on_the_walk_root() {
        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        tree.update_styles(root, |s| {
            s.style.layers = Some(vec!["below".into(), "above".into()]);
        });

        assert_eq!(
            effective_layers(&tree, root),
            vec!["below", "above", "_loading", "_toastrack", "_tooltips"]
        );
    }

    #[test]
    fn effective_layers_defaults_to_default_plus_extras_on_the_walk_root() {
        // No CSS `layers` on the root: Python `Screen.layers` is
        // ("default", "_loading", "_toastrack", "_tooltips").
        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));

        assert_eq!(
            effective_layers(&tree, root),
            vec!["default", "_loading", "_toastrack", "_tooltips"]
        );
    }

    #[test]
    fn effective_layers_deduplicates_a_cascaded_system_name() {
        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        tree.update_styles(root, |s| {
            s.style.layers = Some(vec!["_toastrack".into(), "user".into()]);
        });

        assert_eq!(
            effective_layers(&tree, root),
            vec!["user", "_loading", "_toastrack", "_tooltips"]
        );
    }

    #[test]
    fn effective_layers_adds_no_extras_below_the_walk_root() {
        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let inner = tree.mount(root, Box::new(Label::new("inner")));
        tree.update_styles(inner, |s| {
            s.style.layers = Some(vec!["a".into(), "b".into()]);
        });
        let plain = tree.mount(root, Box::new(Label::new("plain")));

        assert_eq!(effective_layers(&tree, inner), vec!["a", "b"]);
        assert!(effective_layers(&tree, plain).is_empty());
    }

    // ---- Pilot behavioral coverage: hit-testing follows layered paint ----

    struct LayeredButtonsApp;

    /// The `guide/layout/layers` idiom with focusable widgets: `#b1` rides the
    /// TOP layer, `#b2` the bottom layer, shifted by `offset` so they partially
    /// overlap.
    impl crate::TextualApp for LayeredButtonsApp {
        fn configure(&mut self, app: &mut crate::App) -> crate::Result<()> {
            app.load_stylesheet(
                r##"
                Screen { align: center middle; layers: below above; }
                Button { width: 28; height: 8; }
                #b1 { layer: above; }
                #b2 { layer: below; offset: 12 6; }
                "##,
            );
            Ok(())
        }

        fn compose(&mut self) -> AppRoot {
            AppRoot::new()
                .with_child(Button::new("top").id("b1"))
                .with_child(Button::new("bottom").id("b2"))
        }
    }

    fn focused_node(app: &crate::App) -> Option<NodeId> {
        app.active_widget_tree()
            .and_then(super::super::routing::focused_node_id_tree)
    }

    #[test]
    fn click_on_layered_overlap_targets_the_top_layer() {
        crate::run_test(LayeredButtonsApp, |pilot| {
            pilot.pause()?;
            let b1 = pilot.app().query_one("#b1").unwrap();
            let b2 = pilot.app().query_one("#b2").unwrap();
            let (r1, r2) = {
                let tree = pilot.app().active_widget_tree().unwrap();
                (
                    tree.get(b1).unwrap().layout_rect,
                    tree.get(b2).unwrap().layout_rect,
                )
            };

            // The layouts must actually overlap (per-layer arrangement +
            // `offset: 12 6`); the overlap is where paint z-order decides.
            let ox0 = r1.x0.max(r2.x0);
            let oy0 = r1.y0.max(r2.y0);
            let ox1 = r1.x1.min(r2.x1);
            let oy1 = r1.y1.min(r2.y1);
            assert!(
                ox0 < ox1 && oy0 < oy1,
                "layered buttons must overlap, got {r1:?} vs {r2:?}"
            );

            // Click the overlap: the `above`-layer button paints on top, so the
            // painted-frame hit test must target it even though `#b2` is later
            // in DOM order.
            let (cx, cy) = (
                u16::try_from((ox0 + ox1) / 2).unwrap(),
                u16::try_from((oy0 + oy1) / 2).unwrap(),
            );
            pilot.click_at(cx, cy)?;
            assert_eq!(
                focused_node(pilot.app()),
                Some(b1),
                "overlap click must focus the top-layer button"
            );

            // Click inside `#b2`'s OFFSET region outside `#b1`: the offset
            // rect, not the pre-offset rect, receives the hit.
            let bx = r2.x1 - 2;
            let by = r2.y1 - 2;
            assert!(
                bx >= r1.x1 || by >= r1.y1,
                "probe point must be outside the top button, got ({bx}, {by}) vs {r1:?}"
            );
            pilot.click_at(u16::try_from(bx).unwrap(), u16::try_from(by).unwrap())?;
            assert_eq!(
                focused_node(pilot.app()),
                Some(b2),
                "click on the offset-shifted region must focus the below-layer button"
            );
            Ok(())
        })
        .expect("headless run_test must succeed");
    }
}
