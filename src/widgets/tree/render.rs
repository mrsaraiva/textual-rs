//! Tree flatten + paint: `VisibleNode` projection and the `Render` impl.
//!
//! Split out of the former monolithic `tree.rs` (mechanical move, no behavior
//! change).

use rich_rs::{Console, ConsoleOptions, MetaValue, Renderable, Segment, Segments};

use crate::widgets::Widget;
use crate::widgets::helpers::adjust_line_length_no_bg;

use super::Tree;
use super::node::TreeNodeId;

/// Tag a segment with `textual:no_style = true` so `apply_style_to_segments`
/// leaves it untouched. Used for the cursor row's label/twisty cells, whose
/// background/foreground are already fully composed here (matching Python's
/// per-component `get_component_rich_style`). Without this, the widget-level
/// `background-tint: $foreground 5%` from `Tree:focus` would be re-applied to
/// the opaque `$block-cursor-background` fill, shifting `#0178d4` to `#0c7dd4`
/// (Python does not tint component-painted backgrounds).
fn tag_segment_no_style(seg: &mut Segment) {
    let mut meta = seg.meta.take().unwrap_or_default();
    let mut map: std::collections::BTreeMap<String, MetaValue> = meta
        .meta
        .as_ref()
        .map(|m| (**m).clone())
        .unwrap_or_default();
    map.insert("textual:no_style".to_string(), MetaValue::Bool(true));
    meta.meta = Some(std::sync::Arc::new(map));
    seg.meta = Some(meta);
}

#[derive(Debug, Clone)]
pub(super) struct VisibleNode {
    /// Stable id of the underlying arena node.
    pub(super) id: TreeNodeId,
    pub(super) path: Vec<usize>,
    pub(super) depth: usize,
    pub(super) label: String,
    pub(super) expanded: bool,
    pub(super) disabled: bool,
    pub(super) expandable: bool,
    pub(super) component_classes: Vec<String>,
    /// Optional user data associated with the underlying TreeNode.
    pub(super) data: Option<String>,
    /// For each visual depth level, whether the ancestor at that level is the last sibling.
    /// Used for rendering tree guide lines (│, ├, └).
    pub(super) is_last_at_depth: Vec<bool>,
}

impl Tree {
    pub(super) fn visible_nodes(&self) -> Vec<VisibleNode> {
        let depth_offset: usize = if self.show_root { 0 } else { 1 };

        fn walk(
            tree: &Tree,
            ids: &[TreeNodeId],
            tree_depth: usize,
            depth_offset: usize,
            path: &mut Vec<usize>,
            is_last: &mut Vec<bool>,
            out: &mut Vec<VisibleNode>,
        ) {
            let count = ids.len();
            for (idx, &node_id) in ids.iter().enumerate() {
                let Some(node) = tree.nodes.get(node_id) else {
                    continue;
                };
                let last = idx == count - 1;
                path.push(idx);
                is_last.push(last);

                if tree_depth >= depth_offset {
                    let visual_depth = tree_depth - depth_offset;
                    let visual_is_last = is_last[depth_offset..].to_vec();
                    out.push(VisibleNode {
                        id: node_id,
                        path: path.clone(),
                        depth: visual_depth,
                        label: node.label.clone(),
                        expanded: node.expanded,
                        disabled: node.disabled,
                        expandable: node.is_expandable(),
                        component_classes: node.component_classes.clone(),
                        data: node.data.clone(),
                        is_last_at_depth: visual_is_last,
                    });
                }
                if node.expanded {
                    walk(
                        tree,
                        &node.children,
                        tree_depth + 1,
                        depth_offset,
                        path,
                        is_last,
                        out,
                    );
                }
                path.pop();
                is_last.pop();
            }
        }

        let mut out = Vec::new();
        let mut path = Vec::new();
        let mut is_last = Vec::new();
        walk(
            self,
            &self.roots,
            0,
            depth_offset,
            &mut path,
            &mut is_last,
            &mut out,
        );
        out
    }

    pub(super) fn max_line_width(&self) -> usize {
        let mut max_width = 1usize;
        for node in self.visible_nodes() {
            let prefix =
                Self::row_prefix(&node, false, self.show_guides, self.guide_depth, self.hide_twisty);
            let width = rich_rs::cell_len(&prefix).saturating_add(rich_rs::cell_len(&node.label));
            max_width = max_width.max(width);
        }
        max_width
    }

    #[allow(dead_code)] // Used by tests; render now resolves per-component styles directly.
    pub(super) fn node_classes(
        node: &VisibleNode,
        highlighted: bool,
        hovered: bool,
        focused: bool,
    ) -> Vec<String> {
        let mut classes = vec!["tree--node".to_string()];
        if highlighted {
            classes.push("-highlighted".to_string());
        }
        if hovered && !highlighted {
            classes.push("-hover".to_string());
        }
        if highlighted && focused {
            classes.push("-focus".to_string());
        }
        if node.expandable {
            classes.push("-branch".to_string());
        } else {
            classes.push("-leaf".to_string());
        }
        if node.expanded {
            classes.push("-expanded".to_string());
        } else {
            classes.push("-collapsed".to_string());
        }
        if node.disabled {
            classes.push("-disabled".to_string());
        }
        classes.extend(node.component_classes.iter().cloned());
        classes
    }

    /// Render a tree node label, parsing Rich markup if present.
    ///
    /// Falls back to plain styled text if the label contains no markup or
    /// if parsing fails. Mirrors Python's `rich.text.Text` label storage
    /// where per-character styling is preserved.
    fn render_label_markup(
        label: &str,
        base_style: rich_rs::Style,
        console: &Console,
    ) -> Vec<Segment> {
        // Avoid parsing arbitrary bracketed labels as markup.
        // Parse only when the label has an explicit closing tag pattern.
        if !(label.contains('[') && label.contains("[/")) {
            return vec![Segment::styled(label.to_string(), base_style)];
        }
        match rich_rs::markup::render(label, false) {
            Ok(text) => {
                // Merge the base label style (cursor/highlight/component) with
                // any inline markup styles. The base style applies to unstyled
                // portions; markup styles layer on top.
                let opts = ConsoleOptions {
                    size: (label.len().max(1) + 20, 1),
                    max_width: label.len().max(1) + 20,
                    no_wrap: true,
                    ..console.options().clone()
                };
                let rendered: Vec<Segment> = text.render(console, &opts).into_iter().collect();
                // Apply base style to segments that have no explicit style.
                rendered
                    .into_iter()
                    .map(|seg| match seg.style {
                        Some(s) => Segment::styled(seg.text, base_style + s),
                        None => Segment::styled(seg.text, base_style),
                    })
                    .collect()
            }
            Err(_) => vec![Segment::styled(label.to_string(), base_style)],
        }
    }

    fn twisty(node: &VisibleNode, hide_twisty: bool) -> &'static str {
        if hide_twisty || !node.expandable {
            ""
        } else if node.expanded {
            "▼ "
        } else {
            "▶ "
        }
    }

    fn guide_prefix(node: &VisibleNode, show_guides: bool, guide_depth: usize) -> String {
        if node.depth == 0 {
            return String::new();
        }
        let gd = guide_depth.clamp(2, 10);
        let mut prefix = String::new();

        // Ancestor continuation lines for visual depths 1..depth-1
        for level in 1..node.depth {
            if show_guides && !node.is_last_at_depth[level] {
                prefix.push('│');
                for _ in 0..gd - 1 {
                    prefix.push(' ');
                }
            } else {
                for _ in 0..gd {
                    prefix.push(' ');
                }
            }
        }

        // Branch connector for this node
        if show_guides {
            if node.is_last_at_depth[node.depth] {
                prefix.push('└');
            } else {
                prefix.push('├');
            }
            for _ in 0..gd.saturating_sub(2) {
                prefix.push('─');
            }
            prefix.push(' ');
        } else {
            for _ in 0..gd {
                prefix.push(' ');
            }
        }

        prefix
    }

    fn row_prefix(
        node: &VisibleNode,
        _highlighted: bool,
        show_guides: bool,
        guide_depth: usize,
        hide_twisty: bool,
    ) -> String {
        format!(
            "{}{}",
            Self::guide_prefix(node, show_guides, guide_depth),
            Self::twisty(node, hide_twisty)
        )
    }

    pub(super) fn twisty_hit_max_x(
        node: &VisibleNode,
        show_guides: bool,
        guide_depth: usize,
        hide_twisty: bool,
    ) -> usize {
        let guide = Self::guide_prefix(node, show_guides, guide_depth);
        let prefix = format!("{}{}", guide, Self::twisty(node, hide_twisty));
        let mut max_x = rich_rs::cell_len(&prefix);
        if hide_twisty && node.expandable {
            // With the twisty hidden (DirectoryTree), the folder emoji that
            // leads the label is the toggle affordance (Python attaches
            // TOGGLE_STYLE to that prefix). Extend the hit-zone over the
            // label's leading icon (up to and including its trailing space).
            let icon_cells = node
                .label
                .find(' ')
                .map(|byte_idx| rich_rs::cell_len(&node.label[..=byte_idx]))
                .unwrap_or_else(|| rich_rs::cell_len(&node.label));
            max_x = max_x.saturating_add(icon_cells);
        }
        max_x.saturating_sub(1)
    }
}

impl crate::widgets::Render for Tree {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let nodes = self.visible_nodes();
        let mut out = Segments::new();

        // Resolve component styles once per render, through the canonical API
        // with per-name MERGE semantics for stacked node class lists (Python
        // stylize order: DirectoryTree stacks `directory-tree--file` +
        // `--extension` + `--hidden` in application order). During a tree
        // render the Tree's live meta is already the top of the selector
        // stack; off-tree the seed fallback applies.
        let parent_resolved = crate::css::current_self_style().unwrap_or_else(|| {
            let parent_meta = crate::css::selector_meta_generic(self);
            crate::css::resolve_style(self, &parent_meta)
        });
        let resolve_component =
            |classes: &[&str]| crate::css::resolve_component_style_merged(self, classes);
        let default_bg = crate::style::parse_color_like("$background")
            .unwrap_or(crate::style::Color::rgb(0, 0, 0));
        let component_bg_base = crate::css::current_composited_background()
            .or(parent_resolved.bg)
            .unwrap_or(default_bg);
        let base_style = parent_resolved
            .to_rich_over(component_bg_base)
            .unwrap_or_default();
        let guide_style = resolve_component(&["tree--guides"])
            .to_rich_over(component_bg_base)
            .unwrap_or(base_style);
        let guide_hover_style = resolve_component(&["tree--guides-hover"])
            .to_rich_over(component_bg_base)
            .unwrap_or(guide_style);
        let guide_selected_style = resolve_component(&["tree--guides-selected"])
            .to_rich_over(component_bg_base)
            .unwrap_or(guide_style);
        let label_style = resolve_component(&["tree--label"])
            .to_rich_over(component_bg_base)
            .unwrap_or(base_style);
        let cursor_style = resolve_component(&["tree--cursor"])
            .to_rich_over(component_bg_base)
            .unwrap_or(base_style);
        let highlight_style = resolve_component(&["tree--highlight"])
            .to_rich_over(component_bg_base)
            .unwrap_or(base_style);
        let highlight_line_style = resolve_component(&["tree--highlight-line"])
            .to_rich_over(component_bg_base)
            .unwrap_or(base_style);

        let selected = self.selected_line_in(&nodes);
        let selected_path: Option<&[usize]> = if self.node_state().focused {
            nodes.get(selected).map(|node| node.path.as_slice())
        } else {
            None
        };
        let hovered_path: Option<&[usize]> = self
            .hovered_index
            .and_then(|index| nodes.get(index))
            .map(|node| node.path.as_slice());

        for row in 0..height {
            let index = self.offset + row;
            if let Some(node) = nodes.get(index) {
                let highlighted = index == selected && !node.disabled;
                let hovered = self.hovered_index == Some(index);
                let hover_in_path = hovered_path.is_some_and(|path| node.path.starts_with(path));
                let row_line_style = if hover_in_path {
                    highlight_line_style
                } else {
                    rich_rs::Style::default()
                };

                // Per-level guide style, mirroring Python `_tree.py::_render_line`.
                // A guide at visual `level` is styled selected/hover only when an
                // ANCESTOR strictly above that level is the cursor/hover node; the
                // selected style propagates to a node's DESCENDANT guides, not to
                // the node's own connector. So the cursor row's own `├──`/`│`
                // guides keep the base (muted `$surface-lighten-3`) colour, while
                // Python's `$block-cursor-background` only reaches deeper guides.
                let sel_len = selected_path
                    .filter(|sp| node.path.starts_with(sp))
                    .map_or(usize::MAX, <[usize]>::len);
                let hov_len = hovered_path
                    .filter(|hp| node.path.starts_with(hp))
                    .map_or(usize::MAX, <[usize]>::len);
                let guide_style_at = |level: usize| -> rich_rs::Style {
                    let base = if sel_len <= level {
                        guide_selected_style
                    } else if hov_len <= level {
                        guide_hover_style
                    } else {
                        guide_style
                    };
                    base + row_line_style
                };

                // Build label style: base label + component classes + highlight + cursor.
                let mut row_label_style = label_style + row_line_style;
                // Apply node-specific component classes (e.g. directory-tree--file).
                if !node.component_classes.is_empty() {
                    let cc_refs: Vec<&str> =
                        node.component_classes.iter().map(String::as_str).collect();
                    if let Some(cc_style) =
                        resolve_component(&cc_refs).to_rich_over(component_bg_base)
                    {
                        row_label_style = row_label_style + cc_style;
                    }
                }
                if hovered {
                    row_label_style = row_label_style + highlight_style;
                }
                if highlighted {
                    row_label_style = row_label_style + cursor_style;
                }

                // Build segments for this row.
                let mut row_segments: Vec<Segment> = Vec::new();

                // 1. Guide prefix segments (per-depth styled).
                if node.depth > 0 {
                    let gd = self.guide_depth.clamp(2, 10);
                    // Ancestor continuation lines.
                    for level in 1..node.depth {
                        let guide_text = if self.show_guides && !node.is_last_at_depth[level] {
                            let mut s = String::with_capacity(gd);
                            s.push('│');
                            for _ in 0..gd - 1 {
                                s.push(' ');
                            }
                            s
                        } else {
                            " ".repeat(gd)
                        };
                        row_segments.push(Segment::styled(guide_text, guide_style_at(level)));
                    }
                    // Branch connector for this node.
                    let connector = if self.show_guides {
                        let ch = if node.is_last_at_depth[node.depth] {
                            '└'
                        } else {
                            '├'
                        };
                        let mut s = String::with_capacity(gd);
                        s.push(ch);
                        for _ in 0..gd.saturating_sub(2) {
                            s.push('─');
                        }
                        s.push(' ');
                        s
                    } else {
                        " ".repeat(gd)
                    };
                    row_segments.push(Segment::styled(connector, guide_style_at(node.depth)));
                }

                // Cursor label/twisty cells are fully composed above; tag them so
                // the widget-level `background-tint` pass does not re-tint the
                // opaque `$block-cursor-background` fill.
                let label_start = row_segments.len();

                // 2. Twisty (expand/collapse indicator).
                let twisty = Self::twisty(node, self.hide_twisty);
                if !twisty.is_empty() {
                    row_segments.push(Segment::styled(twisty.to_string(), row_label_style));
                }

                // 3. Label text (with Rich markup support).
                //
                // Mirrors Python's TreeNode which stores `rich.text.Text` objects
                // with per-character styling. Parse Rich markup (e.g. `[b]name[/b]`)
                // so json_tree can render bold keys like Python does.
                let label_segs = Self::render_label_markup(&node.label, row_label_style, console);
                row_segments.extend(label_segs);

                // When this row carries the focused cursor, its label/twisty cells
                // paint the opaque `$block-cursor-background`; tag them `no_style`
                // so the `Tree:focus` `background-tint` is not composited on top.
                if highlighted && self.node_state().focused {
                    for seg in &mut row_segments[label_start..] {
                        tag_segment_no_style(seg);
                    }
                }

                // Pad/crop to width.
                // For hover-line rows, fill the entire row width with hover background.
                // Otherwise keep trailing cells transparent so parent surface composes naturally.
                let line = if hover_in_path {
                    Segment::adjust_line_length(&row_segments, width, Some(row_line_style), true)
                } else {
                    adjust_line_length_no_bg(&row_segments, width)
                };
                out.extend(line);
            } else {
                // Empty row beyond visible nodes.
                let line =
                    adjust_line_length_no_bg(&[Segment::styled(String::new(), base_style)], width);
                out.extend(line);
            }
            if row + 1 < height {
                out.push(Segment::line());
            }
        }

        out
    }
}
