//! Layout solver infrastructure.
//!
//! Ports Python Textual's layout pipeline:
//! - 1D space allocation ([`layout_resolve_1d`])
//! - Vertical stacking ([`layout_vertical`])
//! - Horizontal stacking ([`layout_horizontal`])
//! - Grid layout ([`layout_grid`])
//! - Dock positioning ([`arrange_dock`])
//! - Top-level dispatch ([`resolve_layout`])

use crate::node_id::NodeId;
#[cfg(test)]
use crate::style::Dock;
use crate::style::{Align, Display, HorizontalAlign, Layout, VerticalAlign};
#[cfg(test)]
use crate::widget_tree::Rect;
use crate::widget_tree::WidgetTree;

mod common;
mod dock;
mod grid;
mod horizontal;
mod region;
mod resolve_1d;
mod split;
mod vertical;

pub use dock::arrange_dock;
pub use grid::layout_grid;
pub use horizontal::layout_horizontal;
pub use region::Region;
pub use resolve_1d::{Edge, layout_resolve_1d};
pub use vertical::layout_vertical;

use common::get_node_style;
use dock::layout_dock_fill;
use split::{arrange_split, layout_absolute};

fn shift_rect_x(rect: crate::widget_tree::Rect, delta: i32) -> crate::widget_tree::Rect {
    crate::widget_tree::Rect {
        x0: rect.x0 + delta,
        y0: rect.y0,
        x1: rect.x1 + delta,
        y1: rect.y1,
    }
}

fn shift_rect_y(rect: crate::widget_tree::Rect, delta: i32) -> crate::widget_tree::Rect {
    crate::widget_tree::Rect {
        x0: rect.x0,
        y0: rect.y0 + delta,
        x1: rect.x1,
        y1: rect.y1 + delta,
    }
}

/// Apply the CSS `offset` displacement to flow children AFTER container
/// alignment.
///
/// Mirrors Python: the `offset` is stored on each `WidgetPlacement` and added to
/// the placement region at the very end of arrangement — i.e. *after* the
/// container has centered/aligned the flow group. Applying it earlier (folded
/// into the flow position) would let `apply_parent_align` re-center the
/// offset-shifted box and cancel the displacement. A negative offset moves the
/// box off-viewport (signed coordinate); the render path clips it.
///
/// Percentage offsets resolve against the widget's own (post-align) box size,
/// matching `Styles.offset.resolve(Size(width, height), viewport)`.
fn apply_flow_offsets(tree: &mut WidgetTree, children: &[NodeId], _viewport: (u16, u16)) {
    use crate::style::OffsetValue;
    for &child in children {
        let style = get_node_style(tree, child);
        let Some(off) = style.offset else {
            continue;
        };
        if matches!(off.x, OffsetValue::Cells(0)) && matches!(off.y, OffsetValue::Cells(0)) {
            continue;
        }
        let Some(node) = tree.get(child) else {
            continue;
        };
        let (w, h) = (node.layout_rect.width(), node.layout_rect.height());
        let dx = match off.x {
            OffsetValue::Cells(c) => c as i32,
            OffsetValue::Percent(p) => (f32::from(w) * p / 100.0).round() as i32,
        };
        let dy = match off.y {
            OffsetValue::Cells(c) => c as i32,
            OffsetValue::Percent(p) => (f32::from(h) * p / 100.0).round() as i32,
        };
        if dx == 0 && dy == 0 {
            continue;
        }
        if let Some(node) = tree.get_mut(child) {
            node.layout_rect = shift_rect_y(shift_rect_x(node.layout_rect, dx), dy);
            node.content_rect = shift_rect_y(shift_rect_x(node.content_rect, dx), dy);
        }
    }
}

fn apply_parent_align(
    tree: &mut WidgetTree,
    children: &[NodeId],
    available: Region,
    _strategy: Layout,
    align: Option<Align>,
) {
    let Some(align) = align else {
        return;
    };
    if children.is_empty() {
        return;
    }

    // Per-child margins, resolved once. Python's `WidgetPlacement.get_bounds`
    // grows each placement region by its margin before aligning, so the
    // alignment bounds include margins (otherwise a `margin` + `100%` child is
    // shifted by half its own margins — double-counting the gap it already
    // occupies). Mirror that here.
    let margins: Vec<crate::style::Spacing> = children
        .iter()
        .map(|&child| get_node_style(tree, child).effective_margin())
        .collect();

    // Python parity (`_arrange.py`): alignment translates the WHOLE arrangement
    // by a SINGLE offset, not each child independently. The offset is derived
    // from the bounding box of all (margin-grown) placements vs the parent
    // region (`Styles._align_size`). Computing one offset for both axes and
    // applying it to every child preserves children's relative positions — so a
    // narrow buttons row and a wide content box both shift by the same dx and
    // stay left-aligned with each other (block centering), instead of each being
    // independently centered on the cross axis.
    // Bounding box of all (margin-grown) placements in signed coordinate space
    // (mirrors Python `WidgetPlacement.get_bounds`).
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;
    for (idx, &child) in children.iter().enumerate() {
        if let Some(node) = tree.get(child) {
            let margin = margins[idx];
            let layout = node.layout_rect;
            min_x = min_x.min(layout.x0 - i32::from(margin.left));
            min_y = min_y.min(layout.y0 - i32::from(margin.top));
            max_x = max_x.max(layout.x1 + i32::from(margin.right));
            max_y = max_y.max(layout.y1 + i32::from(margin.bottom));
        }
    }
    if min_x == i32::MAX {
        return;
    }
    let used_w = (max_x - min_x).max(0) as u16;
    let used_h = (max_y - min_y).max(0) as u16;

    let dx = match align.horizontal {
        HorizontalAlign::Left => 0i32,
        HorizontalAlign::Center => {
            (available.x + (available.width.saturating_sub(used_w) / 2) as i32) - min_x
        }
        HorizontalAlign::Right => {
            (available.x + available.width.saturating_sub(used_w) as i32) - min_x
        }
    };
    let dy = match align.vertical {
        VerticalAlign::Top => 0i32,
        VerticalAlign::Middle => {
            (available.y + (available.height.saturating_sub(used_h) / 2) as i32) - min_y
        }
        VerticalAlign::Bottom => {
            (available.y + available.height.saturating_sub(used_h) as i32) - min_y
        }
    };

    if dx != 0 || dy != 0 {
        for &child in children {
            if let Some(node) = tree.get_mut(child) {
                if dx != 0 {
                    node.layout_rect = shift_rect_x(node.layout_rect, dx);
                    node.content_rect = shift_rect_x(node.content_rect, dx);
                }
                if dy != 0 {
                    node.layout_rect = shift_rect_y(node.layout_rect, dy);
                    node.content_rect = shift_rect_y(node.content_rect, dy);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Top-level dispatch
// ---------------------------------------------------------------------------

/// Resolve layout for a subtree rooted at `node`.
///
/// 1. Resolves the node's own style for layout strategy.
/// 2. Separates children into docked vs flow.
/// 3. Calls [`arrange_dock`] for docked children → reduced available region.
/// 4. Dispatches flow children to [`layout_vertical`] / [`layout_horizontal`].
///    Grid dispatches to [`layout_grid`] with parent style for track sizing.
pub fn resolve_layout(
    tree: &mut WidgetTree,
    node: NodeId,
    available: Region,
    viewport: (u16, u16),
) {
    let style = get_node_style(tree, node);
    let strategy = style.layout.unwrap_or(Layout::Vertical);
    let is_dock_parent = tree
        .get(node)
        .map(|n| n.widget.style_type() == "Dock")
        .unwrap_or(false);
    let is_overlay_parent = tree
        .get(node)
        .map(|n| n.widget.style_type() == "Overlay")
        .unwrap_or(false);

    // Collect children (snapshot to avoid borrow conflict).
    let children: Vec<NodeId> = tree.children(node).to_vec();
    if children.is_empty() {
        return;
    }

    // Widgets may reserve internal chrome before child layout (for example
    // tab bars). Convert that chrome into an inset child-available region.
    let child_available = if let Some(node_ref) = tree.get(node) {
        let (ct, cr, cb, cl) = node_ref.widget.tree_child_content_inset();
        let x = available.x + i32::from(cl);
        let y = available.y + i32::from(ct);
        let horizontal = cl.saturating_add(cr);
        let vertical = ct.saturating_add(cb);
        let width = available.width.saturating_sub(horizontal);
        let height = available.height.saturating_sub(vertical);
        Region::new(x, y, width.max(1), height.max(1))
    } else {
        available
    };

    // Overlay children are layered in the same region (base + modal stack),
    // not arranged in normal flow.
    if is_overlay_parent {
        let mut layered = Vec::new();
        for &child in &children {
            if tree.get(child).map(|n| !n.display).unwrap_or(true) {
                continue;
            }
            let child_style = get_node_style(tree, child);
            if child_style.display == Some(Display::None) {
                continue;
            }
            layered.push(child);
        }
        if !layered.is_empty() {
            layout_absolute(tree, &layered, child_available, viewport);
        }
        for child in children {
            let Some(child_node) = tree.get(child) else {
                continue;
            };
            // Only `display:none` removes from layout; `visibility:hidden` keeps
            // its space and still positions descendants (paint-time concern).
            if !child_node.display {
                continue;
            }
            let rect = child_node.content_rect;
            let w = rect.width();
            let h = rect.height();
            if w == 0 || h == 0 {
                continue;
            }
            resolve_layout(tree, child, Region::new(rect.x0, rect.y0, w, h), viewport);
        }
        return;
    }

    // Separate children into categories: split, docked, absolute, and flow.
    let mut split = Vec::new();
    let mut docked = Vec::new();
    let mut absolute = Vec::new();
    let mut flow = Vec::new();
    for &child in &children {
        // Runtime/widget-driven hidden nodes should not participate in layout.
        if tree.get(child).map(|n| !n.display).unwrap_or(true) {
            continue;
        }
        let child_style = get_node_style(tree, child);
        if child_style.display == Some(Display::None) {
            continue;
        }
        if child_style.split.is_some() {
            split.push(child);
        } else if child_style.dock.is_some() {
            docked.push(child);
        } else if child_style.position == Some(crate::style::Position::Absolute) {
            absolute.push(child);
        } else {
            flow.push(child);
        }
    }

    // Arrange split children first → reduced available region.
    let after_split = if split.is_empty() {
        child_available
    } else {
        arrange_split(tree, &split, child_available, viewport)
    };

    // Arrange docked children → further reduced available region.
    //
    // Python parity (`_arrange.py::arrange`): widgets are grouped into LAYERS and
    // each layer is arranged independently, starting from the full region. Dock
    // carving (`dock_region.shrink(dock_spacing)`) happens WITHIN a layer, so a
    // docked widget on a SEPARATE layer (e.g. a `layer: ruler` overlay) does NOT
    // consume space from the flow children on the default layer — it overlays
    // them. Without per-layer isolation a bottom/right-docked Ruler (the
    // width/height_comparison demos) shrinks the flow region and shifts every
    // relative-unit (`%`/`w`/`h`/`vw`/`vh`/`fr`) child, diverging from Python.
    //
    // We split docks into those sharing the flow children's layer (carve, as
    // before) and those on a distinct layer (position only, no carve). Layers are
    // compared by name with `None` == the default layer.
    let inner = if docked.is_empty() {
        after_split
    } else {
        let flow_layers: std::collections::HashSet<Option<String>> = flow
            .iter()
            .map(|&c| get_node_style(tree, c).layer)
            .collect();
        let (docked_carve, docked_overlay): (Vec<NodeId>, Vec<NodeId>) =
            docked.iter().copied().partition(|&c| {
                let layer = get_node_style(tree, c).layer;
                // Carve when this dock shares a layer with a flow child (or there
                // are no flow children to compare against — preserve prior carve
                // behaviour).
                flow_layers.is_empty() || flow_layers.contains(&layer)
            });
        // Overlay docks (distinct layer) are positioned against the full region
        // but do not reduce the flow region.
        if !docked_overlay.is_empty() {
            arrange_dock(tree, &docked_overlay, after_split, viewport);
        }
        if docked_carve.is_empty() {
            after_split
        } else {
            arrange_dock(tree, &docked_carve, after_split, viewport)
        }
    };

    // Dispatch flow children to the appropriate layout.
    if !flow.is_empty() {
        if is_dock_parent && flow.len() == 1 {
            layout_dock_fill(tree, flow[0], inner);
        } else {
            // A SCROLL HOST (a widget that clips its descendants to its content
            // box — the `ScrollView` host behind every `*Scroll`/`Scrollable
            // Container`) keeps its children at their RESOLVED size on a clipped
            // axis instead of WRAPPING them to the viewport. Python establishes a
            // clipping content region for any non-`visible` overflow and never
            // re-wraps a widget's box to its container (`_resolve.resolve_box_
            // models` passes no `constrain_width`); the compositor clips at the
            // content box. So a `VerticalScroll` (overflow-x: HIDDEN, overflow-y:
            // auto) must still let its auto/explicit-width child overflow + clip
            // horizontally — `hidden` differs from `auto`/`scroll` only by hiding
            // the scrollbar, not by re-wrapping content. A plain `Container`
            // (overflow: hidden but NOT a scroll host) keeps the historical
            // wrap-to-fit behavior, so this is scoped to scroll hosts only.
            let is_scroll_host = tree
                .get(node)
                .map(|n| n.widget.clips_descendants_to_content())
                .unwrap_or(false);
            // A horizontally-scrollable parent (overflow-x: auto/scroll), OR a
            // scroll host that clips horizontal overflow (overflow-x: hidden on a
            // `VerticalScroll`), lets its children keep their resolved width.
            let allow_h_overflow = matches!(
                style.overflow_x.or(style.overflow),
                Some(crate::style::Overflow::Auto) | Some(crate::style::Overflow::Scroll)
            ) || (is_scroll_host
                && matches!(
                    style.overflow_x.or(style.overflow),
                    Some(crate::style::Overflow::Hidden)
                ));
            // Same for the vertical axis.
            let allow_v_overflow = matches!(
                style.overflow_y.or(style.overflow),
                Some(crate::style::Overflow::Auto) | Some(crate::style::Overflow::Scroll)
            ) || (is_scroll_host
                && matches!(
                    style.overflow_y.or(style.overflow),
                    Some(crate::style::Overflow::Hidden)
                ));
            // Transparent styling wrappers (`Node`, from `.id()`/`.class()`) stand
            // in for the styled widget itself. Python applies `content-align`
            // directly to that widget to position its (shrink-to-content) content
            // within its content box. In the wrapper split, the content IS the
            // single drained child, so the wrapper's `content-align` becomes the
            // child alignment (mapped to `align`) when no explicit `align` is set.
            let is_transparent_wrapper = tree
                .get(node)
                .map(|n| n.widget.is_transparent_wrapper())
                .unwrap_or(false);
            let effective_align = style
                .align
                .or_else(|| {
                    if is_transparent_wrapper {
                        style.content_align.map(|ca| crate::style::Align {
                            horizontal: ca.horizontal,
                            vertical: ca.vertical,
                        })
                    } else {
                        None
                    }
                })
                // A transparent styling wrapper (`Node`, from `.id()`/`.class()`)
                // that carries an explicit `align` is the Rust stand-in for the
                // styled container itself (e.g. `Horizontal#questions` in Python is
                // a `Node("#questions") > Horizontal` here). The wrapper's single
                // flow child fills the wrapper region, so aligning that child within
                // the wrapper is a no-op — the `align` must instead govern the
                // child's OWN children. When THIS node is a wrapper's sole flow
                // child and has no `align` of its own, inherit the wrapper's
                // explicit `align` so it centers/positions its content like Python.
                .or_else(|| {
                    let parent = tree.parent(node)?;
                    let parent_is_wrapper = tree
                        .get(parent)
                        .map(|n| n.widget.is_transparent_wrapper())
                        .unwrap_or(false);
                    if !parent_is_wrapper {
                        return None;
                    }
                    // Only when this node is the wrapper's single flow child (the
                    // collapsed-region case); otherwise the wrapper's own
                    // `apply_parent_align` is meaningful and must not be duplicated.
                    let parent_flow_children: Vec<NodeId> = tree
                        .children(parent)
                        .iter()
                        .copied()
                        .filter(|&c| {
                            tree.get(c).map(|n| n.display).unwrap_or(false)
                                && get_node_style(tree, c).display != Some(Display::None)
                        })
                        .collect();
                    if parent_flow_children.as_slice() != [node] {
                        return None;
                    }
                    get_node_style(tree, parent).align
                });
            // Python parity (`_arrange.py::arrange` + `_build_layers`): flow
            // children are grouped by CSS `layer` and each layer is arranged
            // INDEPENDENTLY — its own flow-layout pass over the full flow region
            // and its own container alignment. A single combined pass would
            // stack widgets on different layers into one flow and align their
            // UNION: in guide/layout/layers two 28x8 Statics on `below`/`above`
            // under `align: center middle` must EACH center to the same spot
            // (y=11 in 30 rows), not center a 16-row two-box stack (y=7). The
            // unset layer is Python's implicit "default" layer (`Widget.layer`:
            // `styles.layer or "default"`); grouping preserves child order, and
            // paint z-order stays a render-side concern (`sort_children_by_layer`).
            let mut layer_groups: Vec<(String, Vec<NodeId>)> = Vec::new();
            for &child in &flow {
                let layer = get_node_style(tree, child)
                    .layer
                    .unwrap_or_else(|| "default".to_string());
                if let Some((_, group)) = layer_groups.iter_mut().find(|(name, _)| *name == layer)
                {
                    group.push(child);
                } else {
                    layer_groups.push((layer, vec![child]));
                }
            }
            for (_, group) in &layer_groups {
                match strategy {
                    Layout::Vertical => {
                        layout_vertical(tree, group, inner, viewport, allow_h_overflow);
                        apply_parent_align(tree, group, inner, Layout::Vertical, effective_align);
                    }
                    Layout::Grid => {
                        layout_grid(tree, group, inner, viewport, &style);
                        apply_parent_align(tree, group, inner, Layout::Grid, effective_align);
                    }
                    Layout::Horizontal => {
                        layout_horizontal(tree, group, inner, viewport, allow_v_overflow);
                        apply_parent_align(tree, group, inner, Layout::Horizontal, effective_align);
                    }
                }
            }
            // CSS `offset` is applied AFTER alignment (Python WidgetPlacement
            // offset is added post-arrange) so a relative-position offset is not
            // cancelled by container centering.
            apply_flow_offsets(tree, &flow, viewport);
        }
    }

    // Place absolute children on top of the original available region (P2-24).
    if !absolute.is_empty() {
        layout_absolute(tree, &absolute, child_available, viewport);
    }

    // Recurse into all laid-out descendants so every node receives
    // layout/content rectangles, not only one level under `node`.
    //
    // NOTE: only `display:none` removes a node from layout. A
    // `visibility:hidden` node STILL participates in layout (its space is
    // preserved) and its descendants must still be positioned — otherwise a
    // `visibility:visible` descendant of a hidden container (Python
    // `#bot { visibility:hidden }` + `#bot > Placeholder { visibility:visible }`)
    // would get a zero rect and never render. Visibility is a PAINT-time concern
    // (see `render_tree_node`'s `should_render`), not a layout-time one.
    for child in children {
        let Some(child_node) = tree.get(child) else {
            continue;
        };
        if !child_node.display {
            continue;
        }
        let rect = child_node.content_rect;
        let w = rect.width();
        let h = rect.height();
        if w == 0 || h == 0 {
            continue;
        }
        resolve_layout(tree, child, Region::new(rect.x0, rect.y0, w, h), viewport);
    }
}

// ---------------------------------------------------------------------------
// Layout inspection (public API for testing)
// ---------------------------------------------------------------------------

/// Return the layout and content rects for a tree node (clamped to `u16`).
///
/// Returns `Some((layout: (x0,y0,x1,y1), content: (x0,y0,x1,y1)))`,
/// or `None` if the node doesn't exist.
///
/// Placement coordinates are signed internally (a widget may sit partly
/// above/left of the viewport with a negative origin). This helper clamps
/// negatives to `0` for the common non-negative inspection case; use
/// [`inspect_node_rects_signed`] to observe negative placements.
#[allow(clippy::type_complexity)] // return type is simple tuples, just wide
pub fn inspect_node_rects(
    tree: &WidgetTree,
    node: NodeId,
) -> Option<((u16, u16, u16, u16), (u16, u16, u16, u16))> {
    let clamp = |v: i32| v.max(0) as u16;
    tree.get(node).map(|n| {
        (
            (
                clamp(n.layout_rect.x0),
                clamp(n.layout_rect.y0),
                clamp(n.layout_rect.x1),
                clamp(n.layout_rect.y1),
            ),
            (
                clamp(n.content_rect.x0),
                clamp(n.content_rect.y0),
                clamp(n.content_rect.x1),
                clamp(n.content_rect.y1),
            ),
        )
    })
}

/// Like [`inspect_node_rects`] but returns the raw **signed** placement
/// coordinates, so a negative origin (for example a widget with `offset: 0 -3`)
/// is observable. Mirrors Python's signed `Region`.
#[allow(clippy::type_complexity)]
pub fn inspect_node_rects_signed(
    tree: &WidgetTree,
    node: NodeId,
) -> Option<((i32, i32, i32, i32), (i32, i32, i32, i32))> {
    tree.get(node).map(|n| {
        (
            (
                n.layout_rect.x0,
                n.layout_rect.y0,
                n.layout_rect.x1,
                n.layout_rect.y1,
            ),
            (
                n.content_rect.x0,
                n.content_rect.y0,
                n.content_rect.x1,
                n.content_rect.y1,
            ),
        )
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Color, Display, Scalar, Spacing, Style};
    use crate::widget_tree::WidgetTree;
    use crate::widgets::Widget;
    use rich_rs::{Console, ConsoleOptions, Segments};

    // -- Test widget ----------------------------------------------------------

    struct LayoutTestWidget {
        label: &'static str,
        inline_style: Option<Style>,
        intrinsic_height: Option<usize>,
        intrinsic_width: Option<usize>,
        transparent_wrapper: bool,
    }

    impl LayoutTestWidget {
        fn new(label: &'static str) -> Self {
            Self {
                label,
                inline_style: None,
                intrinsic_height: None,
                intrinsic_width: None,
                transparent_wrapper: false,
            }
        }

        fn boxed_wrapper(label: &'static str) -> Box<dyn Widget> {
            Box::new(Self {
                transparent_wrapper: true,
                ..Self::new(label)
            })
        }

        fn boxed_wrapper_with_style(label: &'static str, style: Style) -> Box<dyn Widget> {
            Box::new(Self {
                transparent_wrapper: true,
                ..Self::new(label).with_style(style)
            })
        }

        fn with_style(mut self, style: Style) -> Self {
            self.inline_style = Some(style);
            self
        }

        fn boxed(label: &'static str) -> Box<dyn Widget> {
            Box::new(Self::new(label))
        }

        fn boxed_with_style(label: &'static str, style: Style) -> Box<dyn Widget> {
            Box::new(Self::new(label).with_style(style))
        }

        fn with_intrinsic_height(mut self, height: usize) -> Self {
            self.intrinsic_height = Some(height.max(1));
            self
        }

        fn with_intrinsic_width(mut self, width: usize) -> Self {
            self.intrinsic_width = Some(width.max(1));
            self
        }

        fn boxed_with_style_and_intrinsic_height(
            label: &'static str,
            style: Style,
            height: usize,
        ) -> Box<dyn Widget> {
            Box::new(
                Self::new(label)
                    .with_style(style)
                    .with_intrinsic_height(height),
            )
        }

        fn boxed_with_style_and_intrinsic_width(
            label: &'static str,
            style: Style,
            width: usize,
        ) -> Box<dyn Widget> {
            Box::new(
                Self::new(label)
                    .with_style(style)
                    .with_intrinsic_width(width),
            )
        }
    }

    impl Widget for LayoutTestWidget {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn style_type(&self) -> &'static str {
            self.label
        }

        fn style(&self) -> Option<Style> {
            self.inline_style.clone()
        }

        fn layout_height(&self) -> Option<usize> {
            self.intrinsic_height
        }

        fn content_width(&self) -> Option<usize> {
            self.intrinsic_width
        }

        fn is_transparent_wrapper(&self) -> bool {
            self.transparent_wrapper
        }
    }

    // -- Helpers --------------------------------------------------------------

    fn assert_layout_rect(tree: &WidgetTree, node: NodeId, x0: i32, y0: i32, x1: i32, y1: i32) {
        let n = tree.get(node).unwrap();
        assert_eq!(
            n.layout_rect,
            Rect { x0, y0, x1, y1 },
            "layout_rect mismatch for node"
        );
    }

    fn assert_content_rect(tree: &WidgetTree, node: NodeId, x0: i32, y0: i32, x1: i32, y1: i32) {
        let n = tree.get(node).unwrap();
        assert_eq!(
            n.content_rect,
            Rect { x0, y0, x1, y1 },
            "content_rect mismatch for node"
        );
    }

    // =========================================================================
    // 1D resolver tests
    // =========================================================================

    #[test]
    fn resolve_1d_all_fixed() {
        let edges = vec![
            Edge {
                size: Some(10),
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: Some(20),
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: Some(30),
                fraction: 1,
                min_size: 0,
            },
        ];
        assert_eq!(layout_resolve_1d(100, &edges), vec![10, 20, 30]);
    }

    #[test]
    fn resolve_1d_all_flexible_equal() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
        ];
        // 90 / 3 = 30 each
        assert_eq!(layout_resolve_1d(90, &edges), vec![30, 30, 30]);
    }

    #[test]
    fn resolve_1d_all_flexible_weighted() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 2,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 3,
                min_size: 0,
            },
        ];
        // total_fraction = 6, 60/6 = 10 per fraction
        assert_eq!(layout_resolve_1d(60, &edges), vec![10, 20, 30]);
    }

    #[test]
    fn resolve_1d_mixed_fixed_and_flexible() {
        let edges = vec![
            Edge {
                size: Some(20),
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
        ];
        // remaining = 100 - 20 = 80, split equally: 40 each
        assert_eq!(layout_resolve_1d(100, &edges), vec![20, 40, 40]);
    }

    #[test]
    fn resolve_1d_min_size_kicks_in() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 50,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
        ];
        // 60 total, equal weight. 60/2 = 30 each, but edge 0 needs min 50.
        // Fix edge 0 at 50, remaining = 10, edge 1 gets 10.
        assert_eq!(layout_resolve_1d(60, &edges), vec![50, 10]);
    }

    #[test]
    fn resolve_1d_zero_total() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
        ];
        // remaining = 0 - 0 = 0, which is <= 0. Flexible edges get max(min_size, 1) = 1.
        assert_eq!(layout_resolve_1d(0, &edges), vec![1, 1]);
    }

    #[test]
    fn resolve_1d_single_edge_flexible() {
        let edges = vec![Edge {
            size: None,
            fraction: 1,
            min_size: 0,
        }];
        assert_eq!(layout_resolve_1d(50, &edges), vec![50]);
    }

    #[test]
    fn resolve_1d_single_edge_fixed() {
        let edges = vec![Edge {
            size: Some(30),
            fraction: 1,
            min_size: 0,
        }];
        assert_eq!(layout_resolve_1d(50, &edges), vec![30]);
    }

    #[test]
    fn resolve_1d_empty() {
        assert_eq!(layout_resolve_1d(100, &[]), Vec::<u16>::new());
    }

    #[test]
    fn resolve_1d_remainder_distribution() {
        // 100 / 3 = 33 remainder 1. The remainder cascades forward.
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
        ];
        let sizes = layout_resolve_1d(100, &edges);
        // Sum must equal 100.
        assert_eq!(sizes.iter().copied().sum::<u16>(), 100);
        // First two get 33, last gets 34 (remainder cascades).
        assert_eq!(sizes, vec![33, 33, 34]);
    }

    #[test]
    fn resolve_1d_many_edges() {
        let edges: Vec<Edge> = (0..10)
            .map(|_| Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            })
            .collect();
        let sizes = layout_resolve_1d(100, &edges);
        assert_eq!(sizes.iter().copied().sum::<u16>(), 100);
        assert_eq!(sizes.len(), 10);
        // Each gets 10.
        assert!(sizes.iter().all(|&s| s == 10));
    }

    #[test]
    fn resolve_1d_no_room_assigns_min() {
        let edges = vec![
            Edge {
                size: Some(80),
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 5,
            },
        ];
        // remaining = 100 - 80 = 20, flexible gets 20.
        assert_eq!(layout_resolve_1d(100, &edges), vec![80, 20]);

        // Now with total=80: remaining = 80 - 80 = 0 → flexible gets max(5, 1) = 5.
        assert_eq!(layout_resolve_1d(80, &edges), vec![80, 5]);
    }

    #[test]
    fn resolve_1d_all_min_sizes_forced() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 40,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 40,
            },
        ];
        // 50 total, each needs min 40. 50/2=25 < 40, so first fixed at 40.
        // remaining = 10, total_fr=1, 10*1 < 40*1 → second also fixed at 40.
        // Total (80) exceeds available (50), which the docstring allows.
        let sizes = layout_resolve_1d(50, &edges);
        assert_eq!(sizes, vec![40, 40]);
    }

    #[test]
    fn resolve_1d_fraction_weights_with_remainder() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 0,
            },
            Edge {
                size: None,
                fraction: 3,
                min_size: 0,
            },
        ];
        // total=100, total_fraction=4
        // edge 0: 100*1/4 = 25
        // edge 1: 100*3/4 = 75
        assert_eq!(layout_resolve_1d(100, &edges), vec![25, 75]);
    }

    #[test]
    fn resolve_1d_cascading_min_size() {
        // Three flexible edges, but min_size forces sequential fixation.
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 30,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 25,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 20,
            },
        ];
        // total=60, 60/3=20. Edge 0 needs 30 → fix at 30.
        // remaining=30, total_fr=2, 30/2=15. Edge 1 needs 25 → fix at 25.
        // remaining=5, total_fr=1, edge 2 gets 5 (but min_size 20 is not triggered
        // because we're in the distribution phase now and 5 >= 20 is false...wait)
        // Actually: remaining=5, fraction=1, 5*1 < 20*1 → fix at 20.
        // remaining = 5 - 20 = underflow → 0, no more flexible.
        // Hmm but remaining is u64, 5 < 20 so saturating_sub gives 0.
        // After fixing edge 2 at 20, flexible is empty, loop breaks.
        let sizes = layout_resolve_1d(60, &edges);
        assert_eq!(sizes, vec![30, 25, 20]);
    }

    // =========================================================================
    // Region tests
    // =========================================================================

    #[test]
    fn region_to_rect() {
        let r = Region::new(5, 10, 20, 15);
        let rect = r.to_rect();
        assert_eq!(
            rect,
            Rect {
                x0: 5,
                y0: 10,
                x1: 25,
                y1: 25
            }
        );
    }

    #[test]
    fn region_zero() {
        assert_eq!(Region::ZERO.to_rect(), Rect::ZERO);
    }

    // =========================================================================
    // Vertical layout tests
    // =========================================================================

    #[test]
    fn vertical_basic_stacking() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("A", Style::new().height(Scalar::Cells(10))),
        );
        let b = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("B", Style::new().height(Scalar::Cells(20))),
        );

        let available = Region::new(0, 0, 80, 50);
        layout_vertical(&mut tree, &[a, b], available, (80, 50), false);

        // A: 80x10 at (0,0)
        assert_layout_rect(&tree, a, 0, 0, 80, 10);
        assert_content_rect(&tree, a, 0, 0, 80, 10);

        // B: 80x20 at (0,10)
        assert_layout_rect(&tree, b, 0, 10, 80, 30);
        assert_content_rect(&tree, b, 0, 10, 80, 30);
    }

    #[test]
    fn absolute_offset_composes_with_css_offset() {
        use crate::style::{Offset, OffsetValue, Position};
        // A `position: absolute` node with `width: 20; offset-x: -50%`.
        let style = Style {
            position: Some(Position::Absolute),
            width: Some(Scalar::Cells(20)),
            height: Some(Scalar::Cells(3)),
            offset: Some(Offset {
                x: OffsetValue::Percent(-50.0),
                y: OffsetValue::Cells(0),
            }),
            ..Style::new()
        };
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let anchored = tree.mount(root, LayoutTestWidget::boxed_with_style("A", style.clone()));
        let control = tree.mount(root, LayoutTestWidget::boxed_with_style("B", style));

        // Runtime supplies a screen anchor on the first node only.
        assert!(tree.set_absolute_offset(anchored, Some((50, 10))));

        let available = Region::new(0, 0, 80, 50);
        layout_absolute(&mut tree, &[anchored, control], available, (80, 50));

        // Anchored: base_x = 0 + margin(0) + abs(50) = 50, then offset-x -50% of
        // width 20 = -10 → x0 = 40. base_y = 0 + abs(10) = 10 → y0 = 10.
        assert_layout_rect(&tree, anchored, 40, 10, 60, 13);
        // Control (no absolute_offset): base_x = 0, offset-x -50% = -10 → x0 = -10,
        // y0 = 0. Identical to pre-primitive placement — the term is opt-in.
        assert_layout_rect(&tree, control, -10, 0, 10, 3);
    }

    #[test]
    fn vertical_fixed_plus_flex() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let fixed = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Fixed", Style::new().height(Scalar::Cells(10))),
        );
        // An UNSET height (no style, no intrinsic) is NOT a `1fr` share.
        let unset = tree.mount(root, LayoutTestWidget::boxed("Unset"));

        let available = Region::new(0, 0, 80, 50);
        layout_vertical(&mut tree, &[fixed, unset], available, (80, 50), false);

        assert_layout_rect(&tree, fixed, 0, 0, 80, 10);
        // Python parity (`Widget._get_box_model`): an unset-height child fills the
        // FULL container height (50), independently of its fixed sibling, so it
        // overflows past the bottom (y 10..60) rather than taking the remaining 40.
        // (Verified against Python Textual: `Placeholder` after a `height: 10`
        // sibling in an 80x50 viewport → Region(0, 10, 80, 50).)
        assert_layout_rect(&tree, unset, 0, 10, 80, 60);
    }

    #[test]
    fn vertical_fixed_plus_fraction_fills_remaining() {
        // Distinct from an UNSET height: an explicit `1fr` DOES share the
        // remaining space after fixed siblings.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let fixed = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Fixed", Style::new().height(Scalar::Cells(10))),
        );
        let flex = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Flex", Style::new().height(Scalar::Fraction(1.0))),
        );

        let available = Region::new(0, 0, 80, 50);
        layout_vertical(&mut tree, &[fixed, flex], available, (80, 50), false);

        assert_layout_rect(&tree, fixed, 0, 0, 80, 10);
        // `1fr` flex gets the remaining 40 (50 - 10), placed at y 10..50.
        assert_layout_rect(&tree, flex, 0, 10, 80, 50);
    }

    // Gap 3 (vertical axis): two `1fr` children with `margin: 1 0` must reserve
    // the COLLAPSED total margin before fr distribution. Collapsed margin =
    // first.top(1) + interior max(1,1)=1 + last.bottom(1) = 3. total 30 - 3 = 27
    // → box heights 13 and 14 (deterministic remainder cascade), NOT 12/12.
    #[test]
    fn vertical_two_fr_with_margin_reserve_collapsed_margin() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let style = || {
            let mut s = Style::new();
            s.height = Some(Scalar::Fraction(1.0));
            s.margin = Some(Spacing::new(1, 0, 1, 0));
            s
        };
        let a = tree.mount(root, LayoutTestWidget::boxed_with_style("A", style()));
        let b = tree.mount(root, LayoutTestWidget::boxed_with_style("B", style()));

        let available = Region::new(0, 0, 80, 30);
        layout_vertical(&mut tree, &[a, b], available, (80, 30), false);

        // A: y0 = 0 + top margin 1 = 1, box height 13 → y1 = 14.
        assert_layout_rect(&tree, a, 0, 1, 80, 14);
        // Advance: 14 + bottom margin 1 = 15; collapse with B's top margin
        // (max(1,1)=1) → B top margin starts at 14, y0 = 15. Box height 14 →
        // y1 = 29 (leaves the final bottom margin row).
        assert_layout_rect(&tree, b, 0, 15, 80, 29);
    }

    #[test]
    fn vertical_two_unset_height_children_each_fill_container() {
        // Python parity (`docs/examples/how-to/layout01.py`): two bare,
        // unset-height siblings (e.g. `Placeholder`s) in a Screen each receive the
        // FULL container height and stack — the second lands entirely below the
        // fold — instead of splitting the viewport 50/50.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("Header"));
        let b = tree.mount(root, LayoutTestWidget::boxed("Footer"));

        let available = Region::new(0, 0, 120, 30);
        layout_vertical(&mut tree, &[a, b], available, (120, 30), false);

        // Each fills the full 30-row container; Footer starts at the fold.
        assert_layout_rect(&tree, a, 0, 0, 120, 30);
        assert_layout_rect(&tree, b, 0, 30, 120, 60);
    }

    #[test]
    fn vertical_unset_height_transparent_wrappers_share_track() {
        // A transparent `Node` wrapper with an UNSET height must flex-fill like a
        // `1fr` container (mirroring its wrapped child), NOT adopt the bare-leaf
        // fill-the-container rule. So two `Node`-wrapped `1fr` containers split the
        // viewport 50/50 instead of the first one filling it and pushing the second
        // off-screen. (Regression guard for `docs_containers04`.)
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let wrap_a = tree.mount(root, LayoutTestWidget::boxed_wrapper("WrapA"));
        // Wrapped child carries the real sizing intent (`height: 1fr`).
        tree.mount(
            wrap_a,
            LayoutTestWidget::boxed_with_style(
                "InnerA",
                Style::new().height(Scalar::Fraction(1.0)),
            ),
        );
        let wrap_b = tree.mount(root, LayoutTestWidget::boxed_wrapper("WrapB"));
        tree.mount(
            wrap_b,
            LayoutTestWidget::boxed_with_style(
                "InnerB",
                Style::new().height(Scalar::Fraction(1.0)),
            ),
        );

        let available = Region::new(0, 0, 80, 30);
        layout_vertical(&mut tree, &[wrap_a, wrap_b], available, (80, 30), false);

        // Split 50/50; the second wrapper does NOT overflow off-screen.
        assert_layout_rect(&tree, wrap_a, 0, 0, 80, 15);
        assert_layout_rect(&tree, wrap_b, 0, 15, 80, 30);
    }

    #[test]
    fn vertical_auto_height_uses_intrinsic_layout_height() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_height(
                "A",
                Style::new().height(Scalar::Auto),
                3,
            ),
        );
        let b = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_height(
                "B",
                Style::new().height(Scalar::Auto),
                3,
            ),
        );

        let available = Region::new(0, 0, 80, 20);
        layout_vertical(&mut tree, &[a, b], available, (80, 20), false);

        assert_layout_rect(&tree, a, 0, 0, 80, 3);
        assert_layout_rect(&tree, b, 0, 3, 80, 6);
    }

    #[test]
    fn vertical_auto_width_uses_intrinsic_content_width() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_width(
                "A",
                Style::new().width(Scalar::Auto).height(Scalar::Cells(3)),
                12,
            ),
        );
        let b = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("B", Style::new().height(Scalar::Cells(3))),
        );

        let available = Region::new(0, 0, 80, 20);
        layout_vertical(&mut tree, &[a, b], available, (80, 20), false);

        // Auto-width with intrinsic width should not expand to full parent width.
        assert_layout_rect(&tree, a, 0, 0, 12, 3);
        // Non-auto/flex sibling still uses full width.
        assert_layout_rect(&tree, b, 0, 3, 80, 6);
    }

    #[test]
    fn vertical_with_margin() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let child = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style(
                "Child",
                Style::new()
                    .height(Scalar::Cells(10))
                    .margin(Spacing::new(2, 3, 2, 3)),
            ),
        );

        let available = Region::new(0, 0, 80, 50);
        layout_vertical(&mut tree, &[child], available, (80, 50), false);

        // Edge total = 10 + 2 + 2 = 14, but height_edge has chrome=4 + Cells(10) = 14 via scalar_to_edge.
        // layout_rect: x=0+3=3, y=0+2=2, w=80-3-3=74, h=14-2-2=10
        assert_layout_rect(&tree, child, 3, 2, 77, 12);
        assert_content_rect(&tree, child, 3, 2, 77, 12);
    }

    #[test]
    fn vertical_with_padding_and_border() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let child = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style(
                "Child",
                Style::new()
                    .height(Scalar::Cells(10))
                    .padding(Spacing::new(1, 2, 1, 2))
                    .border_top(Color::rgb(255, 255, 255))
                    .border_bottom(Color::rgb(255, 255, 255))
                    .border_left(Color::rgb(255, 255, 255))
                    .border_right(Color::rgb(255, 255, 255)),
            ),
        );

        let available = Region::new(0, 0, 80, 50);
        layout_vertical(&mut tree, &[child], available, (80, 50), false);

        // Default box-sizing is border-box (Textual parity):
        // explicit height includes border+padding chrome.
        // layout: x=0, y=0, w=80, h=10
        assert_layout_rect(&tree, child, 0, 0, 80, 10);
        // content: x=0+1+2=3, y=0+1+1=2, w=80-1-1-2-2=74, h=10-1-1-1-1=6
        assert_content_rect(&tree, child, 3, 2, 77, 8);
    }

    // Python parity (`Widget._get_box_model`): an explicit size below `min-height`
    // is grown up to the minimum. In a horizontal row this is the cross axis, which
    // the consumer never min-clamped before the `extract_child_spec` fix.
    #[test]
    fn horizontal_min_height_clamps_explicit_cross_axis() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let child = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Child", {
                let mut s = Style::new();
                s.height = Some(Scalar::Percent(50.0)); // 50% of 50 = 25
                s.min_height = Some(Scalar::Cells(30)); // clamps UP to 30
                s
            }),
        );
        let available = Region::new(0, 0, 80, 50);
        layout_horizontal(&mut tree, &[child], available, (80, 50), false);
        let n = tree.get(child).unwrap();
        let h = n.layout_rect.y1 - n.layout_rect.y0;
        assert_eq!(h, 30, "min-height should clamp the explicit 50% (25) up to 30");
    }

    // Counterpart for `min-width` on the cross axis of a vertical layout.
    #[test]
    fn vertical_min_width_clamps_explicit_cross_axis() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let child = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Child", {
                let mut s = Style::new();
                s.width = Some(Scalar::Percent(50.0)); // 50% of 80 = 40
                s.min_width = Some(Scalar::Cells(60)); // clamps UP to 60
                s
            }),
        );
        let available = Region::new(0, 0, 80, 50);
        layout_vertical(&mut tree, &[child], available, (80, 50), false);
        let n = tree.get(child).unwrap();
        let w = n.layout_rect.x1 - n.layout_rect.x0;
        assert_eq!(w, 60, "min-width should clamp the explicit 50% (40) up to 60");
    }

    // Main-axis min on an explicit size: a `height: 2; min-height: 5` child in a
    // vertical layout must occupy 5 rows, not 2.
    #[test]
    fn vertical_min_height_clamps_explicit_main_axis() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let child = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Child", {
                let mut s = Style::new();
                s.height = Some(Scalar::Cells(2));
                s.min_height = Some(Scalar::Cells(5));
                s
            }),
        );
        let available = Region::new(0, 0, 80, 50);
        layout_vertical(&mut tree, &[child], available, (80, 50), false);
        let n = tree.get(child).unwrap();
        let h = n.layout_rect.y1 - n.layout_rect.y0;
        assert_eq!(h, 5, "min-height should clamp the explicit 2 up to 5 on the main axis");
    }

    // Python parity (`docs/examples/styles/min_height.py`): a transparent `Node`
    // wrapper (from `.id()`) carries the `#pN { min-height: … }` id while the
    // wrapped leaf carries `Placeholder { height: 50% }`. Python has no wrapper —
    // both the explicit height AND the min apply to the SAME widget. So the
    // wrapper must adopt the wrapped child's explicit `height: 50%` (not collapse
    // to `1fr`, which discards the explicit height and skips the cross-axis
    // min-clamp), then grow to its own `min-height` on the cross axis.
    #[test]
    fn horizontal_wrapper_adopts_child_height_and_clamps_min() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        // Wrapper: unset height + min-height: 30 (like `#p3 { min-height: 30 }`).
        let wrapper = tree.mount(
            root,
            LayoutTestWidget::boxed_wrapper_with_style("Wrapper", {
                let mut s = Style::new();
                s.min_height = Some(Scalar::Cells(30));
                s
            }),
        );
        // Wrapped leaf: explicit `height: 50%` (like `Placeholder { height: 50% }`).
        tree.mount(
            wrapper,
            LayoutTestWidget::boxed_with_style("Leaf", {
                let mut s = Style::new();
                s.height = Some(Scalar::Percent(50.0)); // 50% of 50 = 25
                s
            }),
        );
        let available = Region::new(0, 0, 80, 50);
        layout_horizontal(&mut tree, &[wrapper], available, (80, 50), false);
        let n = tree.get(wrapper).unwrap();
        let h = n.layout_rect.y1 - n.layout_rect.y0;
        assert_eq!(
            h, 30,
            "wrapper must adopt child height 50% (25) then clamp up to min-height 30"
        );
    }

    // Counterpart: without a `min-height`, the wrapper still adopts the wrapped
    // child's explicit `height: 50%` (not a full-container `1fr` fill).
    #[test]
    fn horizontal_wrapper_adopts_child_explicit_height() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let wrapper = tree.mount(root, LayoutTestWidget::boxed_wrapper("Wrapper"));
        tree.mount(
            wrapper,
            LayoutTestWidget::boxed_with_style("Leaf", {
                let mut s = Style::new();
                s.height = Some(Scalar::Percent(50.0)); // 50% of 50 = 25
                s
            }),
        );
        let available = Region::new(0, 0, 80, 50);
        layout_horizontal(&mut tree, &[wrapper], available, (80, 50), false);
        let n = tree.get(wrapper).unwrap();
        let h = n.layout_rect.y1 - n.layout_rect.y0;
        assert_eq!(h, 25, "wrapper must adopt the wrapped child's explicit 50% height");
    }

    // No min set → the clamp is a strict no-op (explicit size is preserved).
    #[test]
    fn explicit_size_without_min_is_unchanged() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let child = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style(
                "Child",
                Style::new().height(Scalar::Cells(7)).width(Scalar::Cells(13)),
            ),
        );
        let available = Region::new(0, 0, 80, 50);
        layout_vertical(&mut tree, &[child], available, (80, 50), false);
        let n = tree.get(child).unwrap();
        assert_eq!(n.layout_rect.y1 - n.layout_rect.y0, 7);
        assert_eq!(n.layout_rect.x1 - n.layout_rect.x0, 13);
    }

    #[test]
    fn vertical_min_max_constraints() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let child = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Child", {
                let mut s = Style::new();
                s.max_width = Some(Scalar::Cells(40));
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        layout_vertical(&mut tree, &[child], available, (80, 50), false);

        // Max width 40 should constrain the layout width.
        let n = tree.get(child).unwrap();
        let w = n.layout_rect.x1 - n.layout_rect.x0;
        assert_eq!(w, 40);
    }

    // =========================================================================
    // Horizontal layout tests
    // =========================================================================

    #[test]
    fn horizontal_basic_stacking() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("A", Style::new().width(Scalar::Cells(20))),
        );
        let b = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("B", Style::new().width(Scalar::Cells(30))),
        );

        let available = Region::new(0, 0, 80, 50);
        layout_horizontal(&mut tree, &[a, b], available, (80, 50), false);

        // A: 20x50 at (0,0)
        assert_layout_rect(&tree, a, 0, 0, 20, 50);
        // B: 30x50 at (20,0)
        assert_layout_rect(&tree, b, 20, 0, 50, 50);
    }

    // Python parity (`Widget._get_box_model`): an UNSET width with no intrinsic
    // content resolves to the FULL container width — it does NOT shrink to the
    // remaining space like a `1fr` share would. So after a fixed 20-cell
    // sibling, the unset child spans the full 80 cells and overflows the row.
    #[test]
    fn horizontal_fixed_plus_unset_width() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let fixed = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Fixed", Style::new().width(Scalar::Cells(20))),
        );
        let unset = tree.mount(root, LayoutTestWidget::boxed("Unset"));

        let available = Region::new(0, 0, 80, 50);
        layout_horizontal(&mut tree, &[fixed, unset], available, (80, 50), false);

        assert_layout_rect(&tree, fixed, 0, 0, 20, 50);
        // Unset width = full container width (80), placed after the fixed
        // sibling → overflows the 80-cell row (Python: content is scrollable).
        assert_layout_rect(&tree, unset, 20, 0, 100, 50);
    }

    // Python parity: EACH unset-width sibling independently receives the full
    // container width; the row overflows to `n * container_width` instead of
    // splitting the viewport into `1fr` shares (guide/layout
    // horizontal_layout_overflow).
    #[test]
    fn horizontal_unset_width_children_each_fill_container_and_overflow() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));
        let c = tree.mount(root, LayoutTestWidget::boxed("C"));

        let available = Region::new(0, 0, 80, 50);
        layout_horizontal(&mut tree, &[a, b, c], available, (80, 50), false);

        assert_layout_rect(&tree, a, 0, 0, 80, 50);
        assert_layout_rect(&tree, b, 80, 0, 160, 50);
        assert_layout_rect(&tree, c, 160, 0, 240, 50);
    }

    #[test]
    fn horizontal_with_margin() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let child = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style(
                "Child",
                Style::new()
                    .width(Scalar::Cells(20))
                    .margin(Spacing::new(2, 3, 2, 3)),
            ),
        );

        let available = Region::new(0, 0, 80, 50);
        layout_horizontal(&mut tree, &[child], available, (80, 50), false);

        // Edge total width = 20 + 3 + 3 = 26
        // layout: x=0+3=3, y=0+2=2, w=26-3-3=20, h=50-2-2=46
        assert_layout_rect(&tree, child, 3, 2, 23, 48);
    }

    // Gap 3 regression: two `1fr` children with `margin: 0 2` in a horizontal
    // row must each receive a box width of `(total - collapsed_margin) / 2`.
    // Collapsed margin = first.left(2) + interior max(2,2)=2 + last.right(2) = 6.
    // total 118 - 6 = 112 → 56 each (NOT 55, which is the off-by-one bug where
    // the margins were subtracted per-child after splitting the full width).
    #[test]
    fn horizontal_two_fr_with_margin_reserve_collapsed_margin() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let style = || {
            let mut s = Style::new();
            s.width = Some(Scalar::Fraction(1.0));
            s.margin = Some(Spacing::new(0, 2, 0, 2));
            s
        };
        let a = tree.mount(root, LayoutTestWidget::boxed_with_style("A", style()));
        let b = tree.mount(root, LayoutTestWidget::boxed_with_style("B", style()));

        let available = Region::new(0, 0, 118, 10);
        layout_horizontal(&mut tree, &[a, b], available, (118, 10), false);

        // A box: x=0+2=2, width 56 → x1=58.
        assert_layout_rect(&tree, a, 2, 0, 58, 10);
        // Gap between boxes is collapsed max(2,2)=2 → B box starts at 58+2=60.
        assert_layout_rect(&tree, b, 60, 0, 116, 10);
    }

    // =========================================================================
    // Dock tests
    // =========================================================================

    #[test]
    fn dock_top() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Header", {
                let mut s = Style::new().height(Scalar::Cells(3));
                s.dock = Some(Dock::Top);
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Docked at top: 80x3 at (0,0)
        assert_layout_rect(&tree, docked, 0, 0, 80, 3);
        // Remaining region starts at y=3.
        assert_eq!(remaining, Region::new(0, 3, 80, 47));
    }

    #[test]
    fn dock_bottom() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Footer", {
                let mut s = Style::new().height(Scalar::Cells(2));
                s.dock = Some(Dock::Bottom);
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Docked at bottom: 80x2 at (0,48)
        assert_layout_rect(&tree, docked, 0, 48, 80, 50);
        assert_eq!(remaining, Region::new(0, 0, 80, 48));
    }

    #[test]
    fn dock_left() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Sidebar", {
                let mut s = Style::new().width(Scalar::Cells(20));
                s.dock = Some(Dock::Left);
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Docked at left: 20x50 at (0,0)
        assert_layout_rect(&tree, docked, 0, 0, 20, 50);
        assert_eq!(remaining, Region::new(20, 0, 60, 50));
    }

    #[test]
    fn dock_right() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Panel", {
                let mut s = Style::new().width(Scalar::Cells(25));
                s.dock = Some(Dock::Right);
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Docked at right: 25x50 at (55,0)
        assert_layout_rect(&tree, docked, 55, 0, 80, 50);
        assert_eq!(remaining, Region::new(0, 0, 55, 50));
    }

    #[test]
    fn dock_multiple() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let header = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Header", {
                let mut s = Style::new().height(Scalar::Cells(3));
                s.dock = Some(Dock::Top);
                s
            }),
        );
        let footer = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Footer", {
                let mut s = Style::new().height(Scalar::Cells(2));
                s.dock = Some(Dock::Bottom);
                s
            }),
        );
        let sidebar = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Sidebar", {
                let mut s = Style::new().width(Scalar::Cells(20));
                s.dock = Some(Dock::Left);
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[header, footer, sidebar], available, (80, 50));

        // Python parity (`_arrange.py::_arrange_dock_widgets`): every dock is sized
        // and placed against the SAME full region — docks OVERLAP at the corners;
        // they do not consume each other's space. Only the accumulated dock_spacing
        // (max extent per edge) shrinks the region returned for flow children.

        // Header: top, full width, 3 tall.
        assert_layout_rect(&tree, header, 0, 0, 80, 3);

        // Footer: bottom, full width, 2 tall. y1 = 50 → footer at y=48.
        assert_layout_rect(&tree, footer, 0, 48, 80, 50);

        // Sidebar: left, FULL height (0..50) against the original region — it does
        // NOT shrink to fit between header/footer (those overlap it at the corners).
        assert_layout_rect(&tree, sidebar, 0, 0, 20, 50);

        // Remaining (flow region): shrunk by spacing left=20, top=3, bottom=2 →
        // x=20, y=3, w=60, h=45.
        assert_eq!(remaining, Region::new(20, 3, 60, 45));
    }

    // =========================================================================
    // Dock auto-width tests (carve_edge Scalar::Auto handling)
    // =========================================================================

    #[test]
    fn dock_left_auto_width_uses_content_width() {
        // Widget with content_width() = Some(25) docked left with width: auto
        // should get 25 columns, not 0 or full available.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_width(
                "Sidebar",
                {
                    let mut s = Style::new().width(Scalar::Auto);
                    s.dock = Some(crate::style::Dock::Left);
                    s
                },
                25,
            ),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Docked at left: 25x50 at (0,0).
        assert_layout_rect(&tree, docked, 0, 0, 25, 50);
        assert_eq!(remaining, Region::new(25, 0, 55, 50));
    }

    #[test]
    fn dock_left_auto_width_falls_back_to_available() {
        // Widget without content_width() (returns None) docked left with
        // width: auto should fall back to current available width.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Sidebar", {
                let mut s = Style::new().width(Scalar::Auto);
                s.dock = Some(crate::style::Dock::Left);
                s
            }),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // No intrinsic width → falls back to full available (80).
        assert_layout_rect(&tree, docked, 0, 0, 80, 50);
        assert_eq!(remaining, Region::new(80, 0, 0, 50));
    }

    #[test]
    fn dock_left_explicit_width_unchanged() {
        // Widget with explicit width: 20 (Cells) should still get 20,
        // regardless of content_width.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_width(
                "Sidebar",
                {
                    let mut s = Style::new().width(Scalar::Cells(20));
                    s.dock = Some(crate::style::Dock::Left);
                    s
                },
                50,
            ), // intrinsic_width=50, but explicit Cells(20) should win
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Docked at left: 20x50, NOT 50.
        assert_layout_rect(&tree, docked, 0, 0, 20, 50);
        assert_eq!(remaining, Region::new(20, 0, 60, 50));
    }

    #[test]
    fn dock_top_auto_height_uses_intrinsic_height() {
        // Widget with layout_height() = Some(5) docked top with height: auto
        // should get 5 rows.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_height(
                "Header",
                {
                    let mut s = Style::new().height(Scalar::Auto);
                    s.dock = Some(crate::style::Dock::Top);
                    s
                },
                5,
            ),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Docked at top: 80x5 at (0,0).
        assert_layout_rect(&tree, docked, 0, 0, 80, 5);
        assert_eq!(remaining, Region::new(0, 5, 80, 45));
    }

    #[test]
    fn dock_top_unset_height_uses_intrinsic_height() {
        // Regression: carve_edge previously defaulted to h=1 when style.height
        // is None (unset), ignoring the widget's layout_height(). This caused
        // docked widgets with no explicit CSS height (e.g. an Input inside a Node
        // with `dock: top`) to be sized as 1 row instead of their natural height.
        // The fix aligns None with Some(Scalar::Auto): both use layout_height().
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_height(
                "Header",
                {
                    let mut s = Style::new();
                    // height is intentionally NOT set (None)
                    s.dock = Some(crate::style::Dock::Top);
                    s
                },
                3,
            ),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // Must use intrinsic height 3, not the old default of 1.
        assert_layout_rect(&tree, docked, 0, 0, 80, 3);
        assert_eq!(remaining, Region::new(0, 3, 80, 47));
    }

    #[test]
    fn dock_left_auto_width_with_max_width_clamp() {
        // Widget with content_width()=60 but max_width=40 — max_width should clamp.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let docked = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_width(
                "Sidebar",
                {
                    let mut s = Style::new().width(Scalar::Auto);
                    s.dock = Some(crate::style::Dock::Left);
                    s.max_width = Some(Scalar::Cells(40));
                    s
                },
                60,
            ),
        );

        let available = Region::new(0, 0, 80, 50);
        let remaining = arrange_dock(&mut tree, &[docked], available, (80, 50));

        // content_width=60 clamped to max_width=40.
        assert_layout_rect(&tree, docked, 0, 0, 40, 50);
        assert_eq!(remaining, Region::new(40, 0, 40, 50));
    }

    #[test]
    fn dock_plus_layout_children() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed_with_style("Container", {
            let mut s = Style::new();
            s.layout = Some(Layout::Vertical);
            s
        }));
        let header = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Header", {
                let mut s = Style::new().height(Scalar::Cells(3));
                s.dock = Some(Dock::Top);
                s
            }),
        );
        let body = tree.mount(root, LayoutTestWidget::boxed("Body"));

        let available = Region::new(0, 0, 80, 50);
        resolve_layout(&mut tree, root, available, (80, 50));

        // Header docked at top.
        assert_layout_rect(&tree, header, 0, 0, 80, 3);

        // Body fills remaining space: y=3, h=47.
        assert_layout_rect(&tree, body, 0, 3, 80, 50);
    }

    #[test]
    fn dock_parent_top_plus_fill_uses_remaining_region() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Dock"));
        let header = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Header", {
                let mut s = Style::new().height(Scalar::Cells(3));
                s.dock = Some(Dock::Top);
                s
            }),
        );
        let fill = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Fill", Style::new().height(Scalar::Cells(1))),
        );

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        assert_layout_rect(&tree, header, 0, 0, 80, 3);
        assert_layout_rect(&tree, fill, 0, 3, 80, 50);
    }

    // =========================================================================
    // resolve_layout dispatch tests
    // =========================================================================

    #[test]
    fn resolve_layout_vertical_default() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("A", Style::new().height(Scalar::Cells(10))),
        );
        let b = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("B", Style::new().height(Scalar::Cells(20))),
        );

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        assert_layout_rect(&tree, a, 0, 0, 80, 10);
        assert_layout_rect(&tree, b, 0, 10, 80, 30);
    }

    #[test]
    fn resolve_layout_horizontal() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed_with_style("Container", {
            let mut s = Style::new();
            s.layout = Some(Layout::Horizontal);
            s
        }));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("A", Style::new().width(Scalar::Cells(30))),
        );
        let b = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("B", Style::new().width(Scalar::Cells(50))),
        );

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        assert_layout_rect(&tree, a, 0, 0, 30, 50);
        assert_layout_rect(&tree, b, 30, 0, 80, 50);
    }

    #[test]
    fn resolve_layout_grid_dispatch() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed_with_style("Container", {
            let mut s = Style::new();
            s.layout = Some(Layout::Grid);
            s.grid_size_columns = Some(2);
            s
        }));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));
        let c = tree.mount(root, LayoutTestWidget::boxed("C"));
        let d = tree.mount(root, LayoutTestWidget::boxed("D"));

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        // 2 cols × 2 rows, all 1fr.
        // col_widths = [40, 40], row_heights = [25, 25].
        assert_layout_rect(&tree, a, 0, 0, 40, 25);
        assert_layout_rect(&tree, b, 40, 0, 80, 25);
        assert_layout_rect(&tree, c, 0, 25, 40, 50);
        assert_layout_rect(&tree, d, 40, 25, 80, 50);
    }

    #[test]
    fn resolve_layout_display_none_excluded() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let visible = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Visible", Style::new().height(Scalar::Cells(10))),
        );
        let hidden = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Hidden", {
                let mut s = Style::new().height(Scalar::Cells(10));
                s.display = Some(Display::None);
                s
            }),
        );

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        // Visible child gets laid out.
        assert_layout_rect(&tree, visible, 0, 0, 80, 10);
        // Hidden child's rects should remain at ZERO (never touched by layout).
        assert_layout_rect(&tree, hidden, 0, 0, 0, 0);
    }

    #[test]
    fn visibility_inherits_to_descendants_with_explicit_override() {
        // Mirrors Python `DOMNode.visible`: a `visibility:hidden` container hides
        // its descendants by inheritance, but a descendant with an explicit
        // `visibility:visible` re-shows (e.g. `#bot { visibility:hidden }` +
        // `#bot > Placeholder { visibility:visible }`).
        use crate::style::Visibility;
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Screen"));

        let hidden_parent = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Hidden", {
                let mut s = Style::new();
                s.visibility = Some(Visibility::Hidden);
                s
            }),
        );
        // Inherits hidden (no own rule).
        let inherits = tree.mount(hidden_parent, LayoutTestWidget::boxed("Inherits"));
        // Explicit visible overrides inherited hidden.
        let override_visible = tree.mount(
            hidden_parent,
            LayoutTestWidget::boxed_with_style("Override", {
                let mut s = Style::new();
                s.visibility = Some(Visibility::Visible);
                s
            }),
        );
        // A sibling under the visible root stays visible.
        let sibling = tree.mount(root, LayoutTestWidget::boxed("Sibling"));

        let _guard = crate::css::set_style_context(crate::css::StyleSheet::parse(""));
        crate::css::apply_display_visibility_to_tree(&mut tree);

        assert_eq!(
            tree.get(hidden_parent).unwrap().visibility,
            Visibility::Hidden
        );
        assert_eq!(
            tree.get(inherits).unwrap().visibility,
            Visibility::Hidden,
            "child with no own rule inherits parent's hidden visibility"
        );
        assert_eq!(
            tree.get(override_visible).unwrap().visibility,
            Visibility::Visible,
            "explicit visibility:visible overrides inherited hidden"
        );
        assert_eq!(tree.get(sibling).unwrap().visibility, Visibility::Visible);
    }

    // Python parity (`_arrange.py::arrange` + `_build_layers`): flow children on
    // DIFFERENT layers are arranged independently — each layer gets its own flow
    // pass and its own container alignment. Two 28x8 Statics on `above`/`below`
    // under `align: center middle` must EACH center to (46, 11) in 120x30
    // (guide/layout/layers), not be stacked into a 16-row flow whose union
    // centers at y=7.
    #[test]
    fn resolve_layout_arranges_and_aligns_each_layer_independently() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed_with_style("Screen", {
            let mut s = Style::new();
            s.align = Some(crate::style::Align {
                horizontal: crate::style::HorizontalAlign::Center,
                vertical: crate::style::VerticalAlign::Middle,
            });
            s.layers = Some(vec!["below".to_string(), "above".to_string()]);
            s
        }));
        let box1 = tree.mount(root, LayoutTestWidget::boxed_with_style("Box1", {
            let mut s = Style::new()
                .width(Scalar::Cells(28))
                .height(Scalar::Cells(8));
            s.layer = Some("above".to_string());
            s
        }));
        let box2 = tree.mount(root, LayoutTestWidget::boxed_with_style("Box2", {
            let mut s = Style::new()
                .width(Scalar::Cells(28))
                .height(Scalar::Cells(8));
            s.layer = Some("below".to_string());
            s
        }));

        resolve_layout(&mut tree, root, Region::new(0, 0, 120, 30), (120, 30));

        // Each layer centers independently: x = (120-28)/2 = 46, y = (30-8)/2 = 11.
        assert_layout_rect(&tree, box1, 46, 11, 74, 19);
        assert_layout_rect(&tree, box2, 46, 11, 74, 19);
    }

    #[test]
    fn resolve_layout_no_children_is_noop() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Empty"));
        // Should not panic.
        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));
    }

    #[test]
    fn resolve_layout_recurses_into_grandchildren() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Root"));
        let parent = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("Parent", Style::new().height(Scalar::Cells(20))),
        );
        let child = tree.mount(
            parent,
            LayoutTestWidget::boxed_with_style("Child", Style::new().height(Scalar::Cells(5))),
        );

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        assert_layout_rect(&tree, parent, 0, 0, 80, 20);
        assert_layout_rect(&tree, child, 0, 0, 80, 5);
    }

    // Gap 2 regression: `align: center middle` on a horizontal container must
    // vertically center its auto-height children, not leave them at the top.
    // Mirrors guide/css/nesting01: a Horizontal (#questions) with two auto-height
    // buttons. Each button is intrinsic height 3 (1 line + tall border), so the
    // row used height is 3 in a 50-row container → centered at y0 = (50-3)/2 = 23.
    #[test]
    fn resolve_layout_align_middle_centers_auto_height_children() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed_with_style("Container", {
            let mut s = Style::new();
            s.layout = Some(Layout::Horizontal);
            s.align = Some(crate::style::Align {
                horizontal: crate::style::HorizontalAlign::Center,
                vertical: crate::style::VerticalAlign::Middle,
            });
            s
        }));
        // Two fixed-width, intrinsic-height-3 children (auto height).
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_height(
                "A",
                Style::new().width(Scalar::Cells(20)),
                3,
            ),
        );
        let b = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style_and_intrinsic_height(
                "B",
                Style::new().width(Scalar::Cells(20)),
                3,
            ),
        );

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        // Used height = 3; container height 50 → top offset (50-3)/2 = 23.
        // Used width = 40; container width 80 → left offset (80-40)/2 = 20.
        assert_layout_rect(&tree, a, 20, 23, 40, 26);
        assert_layout_rect(&tree, b, 40, 23, 60, 26);
    }

    // Gap 2 (wrapper case): `align: center middle` on a transparent `Node` wrapper
    // (the Rust stand-in for `Horizontal#questions` in Python's nesting01) must
    // govern the wrapped Horizontal's buttons, not the (full-region) Horizontal
    // itself. Mirrors `guide/css/nesting01`: Node(#questions, align: center
    // middle) > Horizontal > two fixed-width auto-height buttons.
    #[test]
    fn resolve_layout_wrapper_align_middle_centers_grandchildren() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        // Transparent wrapper carrying the explicit align.
        let wrapper = tree.mount(
            root,
            LayoutTestWidget::boxed_wrapper_with_style("Node", {
                let mut s = Style::new();
                s.align = Some(crate::style::Align {
                    horizontal: crate::style::HorizontalAlign::Center,
                    vertical: crate::style::VerticalAlign::Middle,
                });
                s
            }),
        );
        let row = tree.mount(
            wrapper,
            LayoutTestWidget::boxed_with_style("Horizontal", {
                let mut s = Style::new();
                s.layout = Some(Layout::Horizontal);
                s
            }),
        );
        let a = tree.mount(
            row,
            LayoutTestWidget::boxed_with_style_and_intrinsic_height(
                "A",
                Style::new().width(Scalar::Cells(20)),
                3,
            ),
        );
        let b = tree.mount(
            row,
            LayoutTestWidget::boxed_with_style_and_intrinsic_height(
                "B",
                Style::new().width(Scalar::Cells(20)),
                3,
            ),
        );

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 50), (80, 50));

        // Buttons centered vertically (top offset (50-3)/2 = 23) and the 40-wide
        // row centered horizontally (left offset (80-40)/2 = 20).
        assert_layout_rect(&tree, a, 20, 23, 40, 26);
        assert_layout_rect(&tree, b, 40, 23, 60, 26);
    }

    // =========================================================================
    // Edge case tests
    // =========================================================================

    #[test]
    fn resolve_1d_large_min_exceeds_total() {
        let edges = vec![
            Edge {
                size: None,
                fraction: 1,
                min_size: 60,
            },
            Edge {
                size: None,
                fraction: 1,
                min_size: 60,
            },
        ];
        // Both need 60, only 50 available. First gets fixed at 60,
        // remaining = 0 (saturating_sub). Second also triggers min_size (0 < 60).
        // Total (120) exceeds available (50), which the docstring allows.
        let sizes = layout_resolve_1d(50, &edges);
        assert_eq!(sizes, vec![60, 60]);
    }

    #[test]
    fn vertical_offset_region() {
        // Layout at a non-zero starting position.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("A", Style::new().height(Scalar::Cells(5))),
        );

        let available = Region::new(10, 20, 60, 30);
        layout_vertical(&mut tree, &[a], available, (80, 50), false);

        // A should be at x=10, y=20.
        assert_layout_rect(&tree, a, 10, 20, 70, 25);
    }

    // =========================================================================
    // Grid layout tests
    // =========================================================================

    #[test]
    fn grid_basic_2x2() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));
        let c = tree.mount(root, LayoutTestWidget::boxed("C"));
        let d = tree.mount(root, LayoutTestWidget::boxed("D"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s
        };
        let available = Region::new(0, 0, 80, 50);
        layout_grid(&mut tree, &[a, b, c, d], available, (80, 50), &parent_style);

        // 2 cols × 2 rows, all 1fr.
        // col_widths = [40, 40], row_heights = [25, 25].
        assert_layout_rect(&tree, a, 0, 0, 40, 25);
        assert_layout_rect(&tree, b, 40, 0, 80, 25);
        assert_layout_rect(&tree, c, 0, 25, 40, 50);
        assert_layout_rect(&tree, d, 40, 25, 80, 50);
    }

    #[test]
    fn grid_3x2() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let children: Vec<NodeId> = (0..6)
            .map(|_| tree.mount(root, LayoutTestWidget::boxed("Cell")))
            .collect();

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(3);
            s
        };
        let available = Region::new(0, 0, 90, 60);
        layout_grid(&mut tree, &children, available, (90, 60), &parent_style);

        // 3 cols × 2 rows. col_widths = [30, 30, 30], row_heights = [30, 30].
        assert_layout_rect(&tree, children[0], 0, 0, 30, 30);
        assert_layout_rect(&tree, children[1], 30, 0, 60, 30);
        assert_layout_rect(&tree, children[2], 60, 0, 90, 30);
        assert_layout_rect(&tree, children[3], 0, 30, 30, 60);
        assert_layout_rect(&tree, children[4], 30, 30, 60, 60);
        assert_layout_rect(&tree, children[5], 60, 30, 90, 60);
    }

    #[test]
    fn grid_non_uniform_columns() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s.grid_columns = Some(vec![Scalar::Fraction(1.0), Scalar::Fraction(2.0)]);
            s
        };
        let available = Region::new(0, 0, 90, 30);
        layout_grid(&mut tree, &[a, b], available, (90, 30), &parent_style);

        // 1fr + 2fr = 3fr. 90/3 = 30 per fr.
        // col_widths = [30, 60].
        assert_layout_rect(&tree, a, 0, 0, 30, 30);
        assert_layout_rect(&tree, b, 30, 0, 90, 30);
    }

    #[test]
    fn grid_with_gutter() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));
        let c = tree.mount(root, LayoutTestWidget::boxed("C"));
        let d = tree.mount(root, LayoutTestWidget::boxed("D"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s.grid_gutter_horizontal = Some(2);
            s.grid_gutter_vertical = Some(4);
            s
        };
        let available = Region::new(0, 0, 84, 52);
        layout_grid(&mut tree, &[a, b, c, d], available, (84, 52), &parent_style);

        // col_budget = 84 - 4 = 80. col_widths = [40, 40].
        // row_budget = 52 - 2 = 50. row_heights = [25, 25].
        // col_offsets = [0, 44] (40 + 4 gutter).
        // row_offsets = [0, 27] (25 + 2 gutter).
        assert_layout_rect(&tree, a, 0, 0, 40, 25);
        assert_layout_rect(&tree, b, 44, 0, 84, 25);
        assert_layout_rect(&tree, c, 0, 27, 40, 52);
        assert_layout_rect(&tree, d, 44, 27, 84, 52);
    }

    #[test]
    fn grid_overflow_extra_rows() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let children: Vec<NodeId> = (0..4)
            .map(|_| tree.mount(root, LayoutTestWidget::boxed("Cell")))
            .collect();

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s.grid_size_rows = Some(1); // only 1 row configured
            s
        };
        let available = Region::new(0, 0, 80, 40);
        layout_grid(&mut tree, &children, available, (80, 40), &parent_style);

        // 4 children, 2 cols, 1 explicit row → actual rows = max(1, 2) = 2.
        // col_widths = [40, 40], row_heights = [20, 20].
        assert_layout_rect(&tree, children[0], 0, 0, 40, 20);
        assert_layout_rect(&tree, children[1], 40, 0, 80, 20);
        assert_layout_rect(&tree, children[2], 0, 20, 40, 40);
        assert_layout_rect(&tree, children[3], 40, 20, 80, 40);
    }

    #[test]
    fn grid_fewer_children_than_cells() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(3);
            s
        };
        let available = Region::new(0, 0, 90, 30);
        layout_grid(&mut tree, &[a, b], available, (90, 30), &parent_style);

        // 2 children in 3-col grid → 1 row. col_widths = [30, 30, 30].
        // A in (0,0), B in (1,0). Third cell empty.
        assert_layout_rect(&tree, a, 0, 0, 30, 30);
        assert_layout_rect(&tree, b, 30, 0, 60, 30);
    }

    #[test]
    fn grid_explicit_rows() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s.grid_size_rows = Some(3);
            s
        };
        let available = Region::new(0, 0, 80, 60);
        layout_grid(&mut tree, &[a, b], available, (80, 60), &parent_style);

        // 2 children, 3 explicit rows → 3 rows (even though only 1 needed).
        // row_heights: 60/3 = [20, 20, 20].
        // A in col 0/row 0, B in col 1/row 0.
        assert_layout_rect(&tree, a, 0, 0, 40, 20);
        assert_layout_rect(&tree, b, 40, 0, 80, 20);
    }

    #[test]
    fn grid_default_single_column() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));
        let c = tree.mount(root, LayoutTestWidget::boxed("C"));

        // No grid properties set → default 1 column.
        let parent_style = Style::new();
        let available = Region::new(0, 0, 80, 90);
        layout_grid(&mut tree, &[a, b, c], available, (80, 90), &parent_style);

        // 1 col, 3 rows. col_widths = [80], row_heights = [30, 30, 30].
        assert_layout_rect(&tree, a, 0, 0, 80, 30);
        assert_layout_rect(&tree, b, 0, 30, 80, 60);
        assert_layout_rect(&tree, c, 0, 60, 80, 90);
    }

    #[test]
    fn grid_fixed_column_widths() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s.grid_columns = Some(vec![Scalar::Cells(20), Scalar::Cells(30)]);
            s
        };
        let available = Region::new(0, 0, 80, 40);
        layout_grid(&mut tree, &[a, b], available, (80, 40), &parent_style);

        // Fixed columns: 20 + 30 = 50 (leaves 30 unused).
        assert_layout_rect(&tree, a, 0, 0, 20, 40);
        assert_layout_rect(&tree, b, 20, 0, 50, 40);
    }

    #[test]
    fn grid_column_scalar_cycling() {
        // grid-columns: 1fr 2fr applied to a 4-column grid cycles as 1fr 2fr 1fr 2fr.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let children: Vec<NodeId> = (0..4)
            .map(|_| tree.mount(root, LayoutTestWidget::boxed("Cell")))
            .collect();

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(4);
            s.grid_columns = Some(vec![Scalar::Fraction(1.0), Scalar::Fraction(2.0)]);
            s
        };
        let available = Region::new(0, 0, 60, 20);
        layout_grid(&mut tree, &children, available, (60, 20), &parent_style);

        // 1fr 2fr 1fr 2fr → total 6fr. 60/6 = 10 per fr.
        // col_widths = [10, 20, 10, 20].
        assert_layout_rect(&tree, children[0], 0, 0, 10, 20);
        assert_layout_rect(&tree, children[1], 10, 0, 30, 20);
        assert_layout_rect(&tree, children[2], 30, 0, 40, 20);
        assert_layout_rect(&tree, children[3], 40, 0, 60, 20);
    }

    #[test]
    fn grid_with_child_max_width() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(
            root,
            LayoutTestWidget::boxed_with_style("A", {
                let mut s = Style::new();
                s.max_width = Some(Scalar::Cells(15));
                s
            }),
        );
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s
        };
        let available = Region::new(0, 0, 80, 30);
        layout_grid(&mut tree, &[a, b], available, (80, 30), &parent_style);

        // Cell width = 40, but A has max_width=15.
        let n = tree.get(a).unwrap();
        let w = n.layout_rect.x1 - n.layout_rect.x0;
        assert_eq!(w, 15);

        // B fills its cell normally.
        assert_layout_rect(&tree, b, 40, 0, 80, 30);
    }

    #[test]
    fn grid_empty_children() {
        let mut tree = WidgetTree::new();
        let _root = tree.set_root(LayoutTestWidget::boxed("Container"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s
        };
        // Should not panic with empty children.
        layout_grid(
            &mut tree,
            &[],
            Region::new(0, 0, 80, 50),
            (80, 50),
            &parent_style,
        );
    }

    #[test]
    fn grid_offset_region() {
        // Grid at a non-zero starting position.
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Container"));
        let a = tree.mount(root, LayoutTestWidget::boxed("A"));
        let b = tree.mount(root, LayoutTestWidget::boxed("B"));

        let parent_style = {
            let mut s = Style::new();
            s.grid_size_columns = Some(2);
            s
        };
        let available = Region::new(10, 20, 60, 40);
        layout_grid(&mut tree, &[a, b], available, (80, 50), &parent_style);

        // col_widths = [30, 30], row_heights = [40].
        // A: x=10+0=10, B: x=10+30=40.
        assert_layout_rect(&tree, a, 10, 20, 40, 60);
        assert_layout_rect(&tree, b, 40, 20, 70, 60);
    }

    #[test]
    fn overlay_parent_layers_base_and_modal_in_same_region() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(LayoutTestWidget::boxed("Root"));
        let overlay = tree.mount(root, LayoutTestWidget::boxed("Overlay"));
        let base = tree.mount(overlay, LayoutTestWidget::boxed("Base"));
        let modal = tree.mount(overlay, LayoutTestWidget::boxed("Modal"));

        resolve_layout(&mut tree, root, Region::new(0, 0, 80, 20), (80, 20));

        assert_layout_rect(&tree, overlay, 0, 0, 80, 20);
        assert_layout_rect(&tree, base, 0, 0, 80, 20);
        assert_layout_rect(&tree, modal, 0, 0, 80, 20);
    }
}
