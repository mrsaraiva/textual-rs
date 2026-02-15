use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::compose::ComposeResult;
use crate::css;
use crate::event::{Event, EventCtx};
use crate::message::*;

use super::{
    Widget, WidgetStyles,
    helpers::{
        adjust_line_length_no_bg, clamp_with_constraints, constraints_from_style, empty_classes,
        fixed_height_from_constraints, margin_from_style, merge_constraints, pad_lines_to_width,
    },
};
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

// ── CollapsibleTitle ────────────────────────────────────────────────────

/// Child widget that renders the title bar of a `Collapsible`.
///
/// Displays the collapsed/expanded symbol followed by the title text.
/// Intended to be owned by `Collapsible` and delegated to for title rendering.
pub struct CollapsibleTitle {
    title: String,
    collapsed_symbol: String,
    expanded_symbol: String,
    collapsed: bool,
    focused: bool,
    hovered: bool,
    pressed: bool,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl CollapsibleTitle {
    pub fn new(
        title: impl Into<String>,
        collapsed_symbol: impl Into<String>,
        expanded_symbol: impl Into<String>,
        collapsed: bool,
    ) -> Self {
        Self {
            title: title.into(),
            collapsed_symbol: collapsed_symbol.into(),
            expanded_symbol: expanded_symbol.into(),
            collapsed,
            focused: false,
            hovered: false,
            pressed: false,
            classes: vec!["collapsible--title".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    fn current_symbol(&self) -> &str {
        if self.collapsed {
            &self.collapsed_symbol
        } else {
            &self.expanded_symbol
        }
    }

    pub fn set_collapsed(&mut self, collapsed: bool) {
        self.collapsed = collapsed;
    }

    pub fn set_pressed(&mut self, pressed: bool) {
        self.pressed = pressed;
    }

    pub fn is_pressed(&self) -> bool {
        self.pressed
    }

    /// Render the title line using a pre-resolved style.
    ///
    /// This is called by `Collapsible::render()` which resolves the component
    /// style on `&Collapsible` (not `&CollapsibleTitle`) so that CSS selectors
    /// like `Collapsible > .collapsible--title` match correctly.
    pub(crate) fn render_title_line(&self, width: usize, style: rich_rs::Style) -> Vec<Segment> {
        let title_text = format!("{} {}", self.current_symbol(), self.title);
        let title_text = rich_rs::set_cell_size(&title_text, width);
        let title_line = vec![Segment::styled(title_text, style)];
        adjust_line_length_no_bg(&title_line, width)
    }
}

impl Widget for CollapsibleTitle {
    fn compose(&self) -> ComposeResult {
        Vec::new()
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    fn is_active(&self) -> bool {
        self.pressed && self.hovered
    }

    fn style_type(&self) -> &'static str {
        "CollapsibleTitle"
    }

    fn content_width(&self) -> Option<usize> {
        let symbol_width = rich_rs::cell_len(self.current_symbol());
        let title_width = rich_rs::cell_len(&self.title);
        let content_width = symbol_width
            .saturating_add(1)
            .saturating_add(title_width)
            .max(1);
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let (_, _, border_left, border_right) =
            super::helpers::border_spacing_from_style(&resolved);
        let chrome_lr =
            usize::from(padding.left.saturating_add(padding.right)) + border_left + border_right;
        Some(content_width.saturating_add(chrome_lr).max(1))
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }

    /// Standalone render — resolves style on `self` which won't match
    /// `Collapsible > .collapsible--title` selectors. Prefer the parent's
    /// `render_title_line()` call path when rendering inside a Collapsible.
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let style = rich_rs::Style::new();
        let title_line = self.render_title_line(width, style);
        let mut out = Segments::new();
        out.extend(title_line);
        out
    }

    fn style_classes(&self) -> &[String] {
        if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for CollapsibleTitle {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

// ── Collapsible ─────────────────────────────────────────────────────────

pub struct Collapsible {
    title_widget: CollapsibleTitle,
    collapsed: bool,
    focused: bool,
    hovered: bool,
    children: Vec<Box<dyn Widget>>,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    collapsed_classes: Vec<String>,
    focused_collapsed_classes: Vec<String>,
    styles: WidgetStyles,
}

impl Collapsible {
    pub fn new(title: impl Into<String>) -> Self {
        let title_str = title.into();
        Self {
            title_widget: CollapsibleTitle::new(title_str, "\u{25b6}", "\u{25bc}", true),
            collapsed: true,
            focused: false,
            hovered: false,
            children: Vec::new(),
            classes: vec!["collapsible".to_string()],
            focused_classes: vec!["collapsible".to_string(), "focused".to_string()],
            collapsed_classes: vec!["collapsible".to_string(), "-collapsed".to_string()],
            focused_collapsed_classes: vec![
                "collapsible".to_string(),
                "focused".to_string(),
                "-collapsed".to_string(),
            ],
            styles: WidgetStyles::default(),
        }
    }

    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self.title_widget.set_collapsed(collapsed);
        self
    }

    pub fn collapsed_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.title_widget.collapsed_symbol = symbol.into();
        self
    }

    pub fn expanded_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.title_widget.expanded_symbol = symbol.into();
        self
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    pub fn add_child(&mut self, child: impl Widget + 'static) {
        self.children.push(Box::new(child));
    }

    /// Read-only access to the collapsible's children.
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    /// Mutable access to the collapsible's children.
    pub fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    /// Read-only access to the title widget.
    pub fn title_widget(&self) -> &CollapsibleTitle {
        &self.title_widget
    }

    // ── Reactive getters ─────────────────────────────────────────────────

    pub fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    // ── Reactive setters ─────────────────────────────────────────────────

    /// Reactive setter for `collapsed`. Records the change in the provided
    /// [`ReactiveCtx`] and triggers layout invalidation.
    pub fn set_collapsed(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.collapsed != value {
            let old = self.collapsed;
            self.collapsed = value;
            self.title_widget.set_collapsed(value);
            ctx.record_change(
                "collapsed",
                ReactiveFlags::reactive_layout(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    // ── Watchers ─────────────────────────────────────────────────────────

    fn watch_collapsed(&mut self, _old: &bool, _new: &bool, _ctx: &mut ReactiveCtx) {
        // Layout invalidation is handled by ReactiveFlags::reactive_layout().
    }

    pub fn toggle(&mut self) {
        self.collapsed = !self.collapsed;
        self.title_widget.set_collapsed(self.collapsed);
    }

    fn toggle_with_ctx(&mut self, ctx: &mut EventCtx) {
        self.collapsed = !self.collapsed;
        self.title_widget.set_collapsed(self.collapsed);
        ctx.post_message(Message::CollapsibleToggled(CollapsibleToggled {
            collapsed: self.collapsed,
        }));
        ctx.request_repaint();
        ctx.set_handled();
    }
}

impl Widget for Collapsible {
    fn compose(&self) -> ComposeResult {
        Vec::new()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        std::mem::take(&mut self.children)
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        self.title_widget.set_focus(focused);
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
        self.title_widget.set_hovered(hovered);
    }

    fn is_active(&self) -> bool {
        self.title_widget.is_pressed() && self.hovered
    }

    fn style_type(&self) -> &'static str {
        "Collapsible"
    }

    fn content_width(&self) -> Option<usize> {
        let content_width = self.title_widget.content_width().unwrap_or(1);
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let (_, _, border_left, border_right) =
            super::helpers::border_spacing_from_style(&resolved);
        let chrome_lr =
            usize::from(padding.left.saturating_add(padding.right)) + border_left + border_right;
        Some(content_width.saturating_add(chrome_lr).max(1))
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
        if !self.collapsed {
            for child in &mut self.children {
                child.on_tick(tick);
            }
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        for child in &mut self.children {
            child.on_resize(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.collapsed {
            for child in &mut self.children {
                child.on_event_capture(event, ctx);
                if ctx.handled() {
                    break;
                }
            }
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        // Delegate to children first when expanded
        if !self.collapsed {
            for child in &mut self.children {
                child.on_event(event, ctx);
                if ctx.handled() {
                    return;
                }
            }
        }

        match event {
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                // Click on the title bar area (y == 0) toggles
                if mouse.y == 0 {
                    self.title_widget.set_pressed(true);
                    ctx.request_repaint();
                    ctx.set_handled();
                }
            }
            Event::MouseUp(mouse) => {
                if self.title_widget.is_pressed() {
                    self.title_widget.set_pressed(false);
                    ctx.request_repaint();
                    if mouse.target.is_some_and(|t| t == self.node_id()) && mouse.y == 0 {
                        self.toggle_with_ctx(ctx);
                        return;
                    }
                }
            }
            Event::AppFocus(false) => {
                if self.title_widget.is_pressed() {
                    self.title_widget.set_pressed(false);
                    ctx.request_repaint();
                }
            }
            Event::Key(key) if self.focused => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.toggle_with_ctx(ctx);
                    return;
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn on_message(&mut self, message: &crate::message::MessageEvent, ctx: &mut EventCtx) {
        if !self.collapsed {
            for child in &mut self.children {
                child.on_message(message, ctx);
            }
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        // Resolve component style on &Collapsible so `Collapsible > .collapsible--title` matches.
        let title_style = crate::css::resolve_component_style(self, &["collapsible--title"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);
        let title_line = self.title_widget.render_title_line(width, title_style);

        let mut lines = vec![title_line];

        // When expanded, render children below the title
        if !self.collapsed && height > 1 {
            let child_height_limit = height.saturating_sub(1);
            let mut cursor_y = 0usize;

            for child in &self.children {
                if cursor_y >= child_height_limit {
                    break;
                }
                let meta = css::selector_meta_generic(child.as_ref());
                let resolved = css::resolve_style(child.as_ref(), &meta);
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
                let remaining = child_height_limit.saturating_sub(cursor_y);
                let render_height = clamp_with_constraints(
                    remaining
                        .saturating_sub(margin.top as usize + margin.bottom as usize)
                        .max(1),
                    constraints.min_height,
                    constraints.max_height,
                    remaining
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
                child_lines = Segment::set_shape(
                    &child_lines,
                    render_width,
                    Some(target_height),
                    None,
                    false,
                );
                child_lines = pad_lines_to_width(child_lines, render_width);
                child_lines = super::helpers::apply_margin(child_lines, width, margin);

                let child_h = child_lines.len();
                for line in child_lines {
                    if lines.len() >= height {
                        break;
                    }
                    lines.push(line);
                }
                cursor_y += child_h;
            }
        }

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
        if self.collapsed {
            return Some(1);
        }
        // Title line + children heights
        let mut total = 1usize;
        for child in &self.children {
            let meta = css::selector_meta_generic(child.as_ref());
            let resolved = css::resolve_style(child.as_ref(), &meta);
            let margin = margin_from_style(&resolved);
            match child.layout_height() {
                Some(height) => {
                    total = total
                        .saturating_add(height)
                        .saturating_add(margin.top as usize + margin.bottom as usize);
                }
                None => return None,
            }
        }
        Some(total.max(1))
    }

    fn style_classes(&self) -> &[String] {
        match (self.focused, self.collapsed) {
            (true, true) => &self.focused_collapsed_classes,
            (true, false) => &self.focused_classes,
            (false, true) => &self.collapsed_classes,
            (false, false) => {
                if self.classes.is_empty() {
                    empty_classes()
                } else {
                    &self.classes
                }
            }
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Collapsible {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

impl ReactiveWidget for Collapsible {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
        for change in changes {
            match change.field_name {
                "collapsed" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<bool>(),
                        change.new_value.downcast_ref::<bool>(),
                    ) {
                        self.watch_collapsed(old, new, ctx);
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rich_rs::{Console, ConsoleOptions};

    fn make_console_options(width: usize, height: usize) -> ConsoleOptions {
        let mut opts = ConsoleOptions::default();
        opts.size = (width, height);
        opts.max_width = width;
        opts.max_height = height;
        opts
    }

    // ── CollapsibleTitle tests ──────────────────────────────────────────

    #[test]
    fn collapsible_title_renders_collapsed_symbol() {
        let title = CollapsibleTitle::new("Section", "\u{25b6}", "\u{25bc}", true);
        let console = Console::new();
        let options = make_console_options(20, 1);
        let segments = Widget::render(&title, &console, &options);
        let text: String = segments.iter().map(|s| &*s.text).collect();
        assert!(text.contains("\u{25b6}"));
        assert!(text.contains("Section"));
    }

    #[test]
    fn collapsible_title_renders_expanded_symbol() {
        let title = CollapsibleTitle::new("Section", "\u{25b6}", "\u{25bc}", false);
        let console = Console::new();
        let options = make_console_options(20, 1);
        let segments = Widget::render(&title, &console, &options);
        let text: String = segments.iter().map(|s| &*s.text).collect();
        assert!(text.contains("\u{25bc}"));
        assert!(text.contains("Section"));
    }

    #[test]
    fn collapsible_title_style_type() {
        let title = CollapsibleTitle::new("Test", ">", "v", true);
        assert_eq!(title.style_type(), "CollapsibleTitle");
    }

    #[test]
    fn collapsible_title_focusable() {
        let title = CollapsibleTitle::new("Test", ">", "v", true);
        assert!(title.focusable());
    }

    #[test]
    fn collapsible_title_style_classes() {
        let title = CollapsibleTitle::new("Test", ">", "v", true);
        assert_eq!(title.style_classes(), &["collapsible--title".to_string()]);
    }

    #[test]
    fn collapsible_title_content_width() {
        let title = CollapsibleTitle::new("Hi", ">", "v", true);
        // ">" (1) + " " (1) + "Hi" (2) = 4
        assert_eq!(title.content_width(), Some(4));
    }

    #[test]
    fn collapsible_title_layout_height() {
        let title = CollapsibleTitle::new("Test", ">", "v", true);
        assert_eq!(title.layout_height(), Some(1));
    }

    #[test]
    fn collapsible_title_compose_returns_empty() {
        let title = CollapsibleTitle::new("Test", ">", "v", true);
        assert!(title.compose().is_empty());
    }

    // ── Collapsible compose / take_composed_children tests ──────────────

    #[test]
    fn collapsible_compose_returns_empty() {
        let c = Collapsible::new("Section");
        assert!(c.compose().is_empty());
    }

    #[test]
    fn collapsible_take_composed_children_drains() {
        use crate::widgets::aliases::Static;
        let mut c = Collapsible::new("Section")
            .with_child(Static::new("child1"))
            .with_child(Static::new("child2"));
        assert_eq!(c.children().len(), 2);
        let taken = c.take_composed_children();
        assert_eq!(taken.len(), 2);
        assert!(c.children().is_empty());
    }

    #[test]
    fn collapsible_take_composed_children_empty() {
        let mut c = Collapsible::new("Empty");
        let taken = c.take_composed_children();
        assert!(taken.is_empty());
        assert!(c.children().is_empty());
    }

    // ── Collapsible title_widget delegation tests ───────────────────────

    #[test]
    fn collapsible_delegates_content_width_to_title_widget() {
        let c = Collapsible::new("Hello");
        let title_w = c.title_widget().content_width();
        assert_eq!(c.content_width(), title_w);
    }

    #[test]
    fn collapsible_toggle_syncs_title_widget() {
        let mut c = Collapsible::new("Section");
        assert!(c.is_collapsed());
        assert!(c.title_widget().collapsed);
        c.toggle();
        assert!(!c.is_collapsed());
        assert!(!c.title_widget().collapsed);
        c.toggle();
        assert!(c.is_collapsed());
        assert!(c.title_widget().collapsed);
    }

    #[test]
    fn collapsible_builder_collapsed_syncs_title() {
        let c = Collapsible::new("Section").collapsed(false);
        assert!(!c.is_collapsed());
        assert!(!c.title_widget().collapsed);
    }

    #[test]
    fn collapsible_builder_symbols() {
        let c = Collapsible::new("Section")
            .collapsed_symbol("+")
            .expanded_symbol("-");
        assert_eq!(c.title_widget().collapsed_symbol, "+");
        assert_eq!(c.title_widget().expanded_symbol, "-");
    }

    #[test]
    fn collapsible_style_type() {
        let c = Collapsible::new("Section");
        assert_eq!(c.style_type(), "Collapsible");
    }

    #[test]
    fn collapsible_focus_syncs_to_title_widget() {
        let mut c = Collapsible::new("Section");
        assert!(!c.title_widget().focused);
        c.set_focus(true);
        assert!(c.has_focus());
        assert!(c.title_widget().focused);
        c.set_focus(false);
        assert!(!c.title_widget().focused);
    }

    #[test]
    fn collapsible_hover_syncs_to_title_widget() {
        let mut c = Collapsible::new("Section");
        assert!(!c.title_widget().hovered);
        c.set_hovered(true);
        assert!(c.is_hovered());
        assert!(c.title_widget().hovered);
        c.set_hovered(false);
        assert!(!c.title_widget().hovered);
    }
}
