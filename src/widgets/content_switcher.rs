use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::compose::ComposeResult;
use crate::css;
use crate::event::{Event, EventCtx};
use crate::widgets::delegate::delegate_widget_method;

use super::{
    Widget, WidgetStyles,
    helpers::{
        apply_margin, clamp_with_constraints, constraints_from_style,
        fixed_height_from_constraints, margin_from_style, merge_constraints, pad_lines_to_width,
    },
};

pub struct ContentSwitcher {
    children: Vec<Box<dyn Widget>>,
    /// CSS ids of children in insertion order.  Populated when children are
    /// added via `with_child`, `add_child`, or `add_content`.  Retained after
    /// `take_composed_children` drains `children` so that
    /// `child_display_for_tree` can still map indices to ids.
    child_ids: Vec<Option<String>>,
    current: Option<String>,
    styles: WidgetStyles,
    /// True once `take_composed_children` has been called (arena tree mode).
    children_extracted: bool,
}

struct IdTaggedChild {
    id: String,
    child: Box<dyn Widget>,
}

impl Widget for IdTaggedChild {
    fn style_type(&self) -> &'static str {
        self.child.style_type()
    }

    fn style_id(&self) -> Option<&str> {
        Some(self.id.as_str())
    }

    // delegate-audit: 72 methods as of 2026-02-26
    delegate_widget_method!(
        child,
        [
            render,
            render_with_debug,
            render_line,
            render_lines,
            compose,
            take_composed_children,
            focusable,
            can_focus,
            can_focus_children,
            set_focus,
            has_focus,
            on_mount,
            on_unmount,
            on_tick,
            on_resize,
            on_layout,
            set_virtual_content_size,
            on_event_capture,
            on_event,
            on_message,
            on_mouse_scroll,
            on_mouse_move,
            on_app_key,
            on_app_action,
            on_app_message,
            on_app_tick,
            on_app_mount,
            scroll_offset,
            scroll_offset_f32,
            scroll_viewport_size,
            scroll_virtual_content_size,
            clips_descendants_to_content,
            child_display_for_tree,
            tree_child_content_inset,
            layout_height,
            content_width,
            layout_constraints,
            preserve_underlay,
            bindings,
            binding_hints,
            execute_action,
            action_namespace,
            action_registry,
            styles,
            styles_mut,
            style_type_aliases,
            style_classes,
            set_style_id,
            border_title,
            border_subtitle,
            is_disabled,
            set_disabled_state,
            is_loading,
            set_loading_state,
            is_hovered,
            set_hovered,
            is_active,
            mouse_interactive,
            tooltip,
            tooltip_anchor,
            help_markup,
            allow_select,
            selection_at,
            selection_word_range_at,
            selection_all_range,
            update_selection,
            clear_selection,
            get_selection,
            selection_updated,
            reactive_widget,
        ]
    );
}

impl Default for ContentSwitcher {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentSwitcher {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            child_ids: Vec::new(),
            current: None,
            styles: WidgetStyles::default(),
            children_extracted: false,
        }
    }

    pub fn initial(mut self, id: impl Into<String>) -> Self {
        self.current = Some(id.into());
        self
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.child_ids.push(child.style_id().map(str::to_string));
        self.children.push(Box::new(child));
        self
    }

    pub fn add_child(&mut self, child: impl Widget + 'static) {
        self.child_ids.push(child.style_id().map(str::to_string));
        self.children.push(Box::new(child));
    }

    /// Python-compat helper: add content with optional id and optional activation.
    pub fn add_content(
        &mut self,
        child: impl Widget + 'static,
        id: Option<&str>,
        set_current: bool,
    ) {
        let original_id = child.style_id().map(str::to_string);
        let child: Box<dyn Widget> = if let Some(id) = id {
            Box::new(IdTaggedChild {
                id: id.to_string(),
                child: Box::new(child),
            })
        } else {
            Box::new(child)
        };
        let fallback_id = id.map(str::to_string).or(original_id);
        self.child_ids.push(fallback_id.clone());
        self.children.push(child);
        if set_current {
            self.current = fallback_id;
        }
    }

    /// Returns the 0-based index of the child whose id matches `self.current`.
    ///
    /// Uses `child_ids` so it works both before and after `take_composed_children`.
    fn current_child_index(&self) -> Option<usize> {
        let current = self.current.as_deref()?;
        self.child_ids
            .iter()
            .position(|id| id.as_deref() == Some(current))
    }

    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    pub fn set_current(&mut self, current: Option<String>) {
        self.current = current;
    }

    fn query_child_index_by_id(&self, id: &str) -> Option<usize> {
        self.children
            .iter()
            .position(|child| child.style_id() == Some(id))
    }

    fn query_visible_child_index(&self) -> Option<usize> {
        let current = self.current.as_deref()?;
        self.query_child_index_by_id(current)
    }

    /// Returns a reference to the currently visible content widget, if any.
    ///
    /// The visible child is determined by matching `current` against each
    /// child's `style_id()`.
    pub fn visible_content(&self) -> Option<&dyn Widget> {
        self.visible_child()
    }

    fn visible_child(&self) -> Option<&dyn Widget> {
        self.query_visible_child_index()
            .and_then(|index| self.children.get(index))
            .map(|child| child.as_ref())
    }

    fn visible_child_mut(&mut self) -> Option<&mut Box<dyn Widget>> {
        let index = self.query_visible_child_index()?;
        self.children.get_mut(index)
    }

    /// Read-only access to all children (not just the visible one).
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    /// Mutable access to all children.
    pub fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }
}

impl Widget for ContentSwitcher {
    fn compose(&self) -> ComposeResult {
        Vec::new()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.children_extracted = true;
        std::mem::take(&mut self.children)
    }

    fn focusable(&self) -> bool {
        false
    }

    fn style_type(&self) -> &'static str {
        "ContentSwitcher"
    }

    /// Control arena-tree child visibility based on `self.current`.
    ///
    /// Called every frame by `sync_widget_controlled_child_display_tree`.
    /// Returns `Some(true)` for the active child, `Some(false)` for all others.
    /// Returns `None` before `take_composed_children` is called (flat mode).
    fn child_display_for_tree(&self, child_index: usize) -> Option<bool> {
        if !self.children_extracted {
            return None;
        }
        Some(self.current_child_index() == Some(child_index))
    }

    fn on_mount(&mut self) {
        for child in &mut self.children {
            child.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        for child in &mut self.children {
            child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if let Some(child) = self.visible_child_mut() {
            child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        if let Some(child) = self.visible_child_mut() {
            child.on_resize(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Some(child) = self.visible_child_mut() {
            child.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Some(child) = self.visible_child_mut() {
            child.on_event(event, ctx);
        }
    }

    fn on_message(&mut self, message: &crate::message::MessageEvent, ctx: &mut EventCtx) {
        if let Some(child) = self.visible_child_mut() {
            child.on_message(message, ctx);
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        let child = match self.visible_child() {
            Some(child) => child,
            None => {
                // No visible child: render empty space
                let mut out = Segments::new();
                out.push(Segment::styled(" ".repeat(width), rich_rs::Style::new()));
                return out;
            }
        };

        let meta = css::selector_meta_generic(child);
        let resolved = css::resolve_style(child, &meta);
        let margin = margin_from_style(&resolved);
        let style_constraints = constraints_from_style(&resolved);
        let constraints = merge_constraints(style_constraints, child.layout_constraints());
        let available_width = width
            .saturating_sub(margin.left as usize + margin.right as usize)
            .max(1);
        let render_width = clamp_with_constraints(
            available_width,
            constraints.min_width,
            constraints.max_width,
            available_width,
        );
        let render_height = clamp_with_constraints(
            height
                .saturating_sub(margin.top as usize + margin.bottom as usize)
                .max(1),
            constraints.min_height,
            constraints.max_height,
            height
                .saturating_sub(margin.top as usize + margin.bottom as usize)
                .max(1),
        );
        let render_height = if let Some(fixed) = child.layout_height() {
            render_height.min(fixed.max(1))
        } else {
            render_height
        };

        let mut child_options = options.clone();
        child_options.size = (render_width, render_height);
        child_options.max_width = render_width;
        child_options.max_height = render_height;

        let segments = child.render_styled(console, &child_options);
        let mut child_lines =
            Segment::split_and_crop_lines(segments, render_width, None, true, false);
        let mut target_height = child.layout_height().unwrap_or(child_lines.len().max(1));
        target_height = clamp_with_constraints(
            target_height,
            constraints.min_height,
            constraints.max_height,
            target_height,
        );
        child_lines =
            Segment::set_shape(&child_lines, render_width, Some(target_height), None, false);
        child_lines = pad_lines_to_width(child_lines, render_width);
        let lines = apply_margin(child_lines, width, margin);

        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        let child = self.visible_child()?;
        let meta = css::selector_meta_generic(child);
        let resolved = css::resolve_style(child, &meta);
        let margin = margin_from_style(&resolved);
        child.layout_height().map(|h| {
            h.saturating_add(margin.top as usize + margin.bottom as usize)
                .max(1)
        })
    }

    fn content_width(&self) -> Option<usize> {
        let child = self.visible_child()?;
        let meta = css::selector_meta_generic(child);
        let resolved = css::resolve_style(child, &meta);
        let margin = margin_from_style(&resolved);
        let content_width = child.content_width().map(|w| {
            w.saturating_add(margin.left as usize + margin.right as usize)
                .max(1)
        })?;
        let self_meta = css::selector_meta_generic(self);
        let self_resolved = css::resolve_style(self, &self_meta);
        let self_padding = self_resolved.effective_padding();
        let (_, _, border_left, border_right) =
            super::helpers::border_spacing_from_style(&self_resolved);
        let chrome_lr = usize::from(self_padding.left.saturating_add(self_padding.right))
            + border_left
            + border_right;
        Some(content_width.saturating_add(chrome_lr).max(1))
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for ContentSwitcher {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Probe;

    impl Widget for Probe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }
    }

    #[test]
    fn add_content_sets_id_and_current_when_requested() {
        let mut switcher = ContentSwitcher::new();
        switcher.add_content(Probe, Some("alpha"), true);
        assert_eq!(switcher.current(), Some("alpha"));
        assert_eq!(switcher.children().len(), 1);
        assert_eq!(switcher.children()[0].style_id(), Some("alpha"));
    }

    #[test]
    fn add_content_without_id_preserves_existing_current() {
        let mut switcher = ContentSwitcher::new().initial("alpha");
        switcher.add_content(Probe, None, false);
        assert_eq!(switcher.current(), Some("alpha"));
        assert_eq!(switcher.children().len(), 1);
    }

    #[test]
    fn child_display_for_tree_inactive_before_extraction() {
        let switcher = ContentSwitcher::new().initial("a");
        // Before take_composed_children, returns None (flat render mode).
        assert_eq!(switcher.child_display_for_tree(0), None);
    }

    #[test]
    fn child_display_for_tree_shows_only_current_after_extraction() {
        let mut switcher = ContentSwitcher::new()
            .initial("b")
            .with_child(Probe)
            .with_child(Probe);
        // Manually set child_ids since Probe has no style_id.
        switcher.child_ids[0] = Some("a".to_string());
        switcher.child_ids[1] = Some("b".to_string());

        let _ = switcher.take_composed_children(); // enter arena tree mode
        assert_eq!(switcher.child_display_for_tree(0), Some(false), "a hidden");
        assert_eq!(switcher.child_display_for_tree(1), Some(true), "b visible");
        assert_eq!(
            switcher.child_display_for_tree(2),
            Some(false),
            "oob hidden"
        );
    }

    #[test]
    fn child_display_for_tree_all_hidden_when_no_current() {
        let mut switcher = ContentSwitcher::new().with_child(Probe);
        switcher.child_ids[0] = Some("x".to_string());
        let _ = switcher.take_composed_children();
        // No current set → all hidden.
        assert_eq!(switcher.child_display_for_tree(0), Some(false));
    }

    #[test]
    fn set_current_changes_which_child_is_shown() {
        let mut switcher = ContentSwitcher::new()
            .initial("a")
            .with_child(Probe)
            .with_child(Probe);
        switcher.child_ids[0] = Some("a".to_string());
        switcher.child_ids[1] = Some("b".to_string());
        let _ = switcher.take_composed_children();

        assert_eq!(switcher.child_display_for_tree(0), Some(true));
        assert_eq!(switcher.child_display_for_tree(1), Some(false));

        switcher.set_current(Some("b".to_string()));
        assert_eq!(switcher.child_display_for_tree(0), Some(false));
        assert_eq!(switcher.child_display_for_tree(1), Some(true));
    }
}
