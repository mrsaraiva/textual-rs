use std::sync::atomic::{AtomicU64, Ordering};

use rich_rs::{Console, ConsoleOptions, MetaValue, Segments, StyleMeta};

use crate::debug::DebugLayout;
use crate::event::{Event, EventCtx};
use crate::style::{Color, Style};

mod aliases;
mod containers;
mod controls;
mod defaults;
mod helpers;
mod layout;
mod style_selectors;
mod text;

pub use aliases::{Horizontal, Static, VerticalScroll};
pub use containers::{
    AppRoot, Constrained, Container, Frame, Node, Overlay, Panel, ScrollView, Styled,
};
pub use controls::{
    Button, Checkbox, DataTable, Input, ListView, Spacer, Tab, Tabs, Tree, TreeNode,
};
pub use defaults::default_widget_stylesheet;
pub use helpers::WidgetRenderable;
pub(crate) use helpers::set_hover_by_id;
pub use layout::{Dock, DockItem, DockKind, Grid, Row, RowAlign};
pub use style_selectors::{
    StyleContextGuard, StyleRule, StyleSelector, StyleSheet, set_style_context,
};
pub use text::{Label, Markdown};

const META_WIDGET_ID: &str = "textual:widget_id";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct WidgetId(u64);

impl WidgetId {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }

    pub fn from_u64(value: u64) -> Self {
        Self(value)
    }
}

pub trait Widget: Send + Sync {
    fn id(&self) -> WidgetId;
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments;
    fn render_styled_dyn_obj(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: Option<&DebugLayout>,
    ) -> Segments {
        let meta = style_selectors::selector_meta_generic(self);
        let resolved = style_selectors::resolve_style(self, &meta);
        let parent_style = style_selectors::current_parent_style();
        let line_pad = resolved.line_pad.unwrap_or(0);
        let full_width = options.size.0.max(1);
        let full_height = options.size.1.max(1);
        let (border_top, border_bottom, border_left, border_right) =
            helpers::border_spacing_from_style(&resolved);

        let content_width = full_width
            .saturating_sub(border_left + border_right)
            .saturating_sub(line_pad.saturating_mul(2))
            .max(1);
        let content_height = full_height
            .saturating_sub(border_top + border_bottom)
            .max(1);

        // Textual's `line-pad` is horizontal padding applied to each line. To model this, render the
        // widget into a smaller content width and then wrap each line with `line_pad` spaces.
        let mut content_options = options.clone();
        content_options.size = (content_width, content_height);
        content_options.max_width = content_width;
        content_options.max_height = content_height;

        let segments = style_selectors::with_style_stack(meta, resolved, || match debug {
            Some(debug) => self.render_with_debug(console, &content_options, debug),
            None => self.render(console, &content_options),
        });
        let segments = tag_widget_meta(self.id(), segments);

        let inner_width = content_width
            .saturating_add(line_pad.saturating_mul(2))
            .max(1);
        let segments = if line_pad > 0 {
            let padded = helpers::apply_line_pad(segments, content_width, inner_width, line_pad);
            tag_widget_meta(self.id(), padded)
        } else {
            segments
        };

        let styled = style_selectors::apply_style_to_segments(self.id(), segments, resolved);
        let segments = helpers::apply_border_edges(
            styled,
            inner_width,
            resolved,
            parent_style,
            full_width,
            full_height,
        );
        tag_widget_meta(self.id(), segments)
    }
    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        _debug: &DebugLayout,
    ) -> Segments {
        self.render(console, options)
    }
    fn on_mount(&mut self) {}
    fn on_unmount(&mut self) {}
    fn on_tick(&mut self, _tick: u64) {}
    fn on_resize(&mut self, _width: u16, _height: u16) {}
    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn visit_children_mut(&mut self, _f: &mut dyn FnMut(&mut dyn Widget)) {}
    fn focusable(&self) -> bool {
        false
    }
    fn set_focus(&mut self, _focused: bool) {}
    /// Whether the widget is disabled (used for `:disabled` selector matching).
    fn is_disabled(&self) -> bool {
        false
    }
    /// Whether the widget currently has focus (used for `:focus` selector matching).
    fn has_focus(&self) -> bool {
        false
    }
    /// Whether the widget is hovered (mouse support not yet implemented).
    fn is_hovered(&self) -> bool {
        false
    }
    fn set_hovered(&mut self, _hovered: bool) {}
    /// Whether the widget is active (e.g. pressed/dragging).
    fn is_active(&self) -> bool {
        false
    }
    /// Optional intrinsic content width hint (in cells), used by layout when `width: auto`.
    ///
    /// This should return the width of the widget's *content* (excluding margins and borders).
    fn content_width(&self) -> Option<usize> {
        None
    }
    fn layout_height(&self) -> Option<usize> {
        helpers::fixed_height_from_constraints(self.layout_constraints())
    }
    fn layout_constraints(&self) -> LayoutConstraints {
        self.styles()
            .map(|styles| styles.layout)
            .unwrap_or_default()
    }
    fn style(&self) -> Option<Style> {
        self.styles().map(|styles| styles.style)
    }
    fn styles(&self) -> Option<&WidgetStyles> {
        None
    }
    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        None
    }
    fn style_type(&self) -> &'static str {
        std::any::type_name::<Self>()
            .rsplit("::")
            .next()
            .unwrap_or("Widget")
    }
    fn style_id(&self) -> Option<&str> {
        None
    }
    fn style_classes(&self) -> &[String] {
        helpers::empty_classes()
    }
    fn render_styled(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.render_styled_dyn_obj(console, options, None)
    }
    fn render_styled_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        self.render_styled_dyn_obj(console, options, Some(debug))
    }
    fn set_width(&mut self, value: usize) {
        if let Some(styles) = self.styles_mut() {
            styles.set_width(value);
        }
    }

    fn set_height(&mut self, value: usize) {
        if let Some(styles) = self.styles_mut() {
            styles.set_height(value);
        }
    }

    fn set_min_width(&mut self, value: usize) {
        if let Some(styles) = self.styles_mut() {
            styles.set_min_width(value);
        }
    }

    fn set_max_width(&mut self, value: usize) {
        if let Some(styles) = self.styles_mut() {
            styles.set_max_width(value);
        }
    }

    fn set_min_height(&mut self, value: usize) {
        if let Some(styles) = self.styles_mut() {
            styles.set_min_height(value);
        }
    }

    fn set_max_height(&mut self, value: usize) {
        if let Some(styles) = self.styles_mut() {
            styles.set_max_height(value);
        }
    }
}

fn tag_widget_meta(widget_id: WidgetId, segments: Segments) -> Segments {
    let mut out = Segments::new();
    for mut seg in segments {
        if seg.control.is_some() {
            out.push(seg);
            continue;
        }

        let has_widget_id = seg
            .meta
            .as_ref()
            .and_then(|m| m.meta.as_ref())
            .map(|map| map.contains_key(META_WIDGET_ID))
            .unwrap_or(false);
        if has_widget_id {
            out.push(seg);
            continue;
        }

        let mut map = seg
            .meta
            .as_ref()
            .and_then(|m| m.meta.as_ref())
            .map(|m| (**m).clone())
            .unwrap_or_default();
        map.insert(
            META_WIDGET_ID.to_string(),
            MetaValue::Int(widget_id.as_u64() as i64),
        );

        let mut meta = seg.meta.unwrap_or_else(StyleMeta::new);
        meta.meta = Some(std::sync::Arc::new(map));
        seg.meta = Some(meta);
        out.push(seg);
    }
    out
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutConstraints {
    pub min_width: Option<usize>,
    pub max_width: Option<usize>,
    pub min_height: Option<usize>,
    pub max_height: Option<usize>,
}

impl LayoutConstraints {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn min_width(mut self, value: usize) -> Self {
        self.min_width = Some(value.max(1));
        self
    }

    pub fn max_width(mut self, value: usize) -> Self {
        self.max_width = Some(value.max(1));
        self
    }

    pub fn min_height(mut self, value: usize) -> Self {
        self.min_height = Some(value.max(1));
        self
    }

    pub fn max_height(mut self, value: usize) -> Self {
        self.max_height = Some(value.max(1));
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct WidgetStyles {
    pub style: Style,
    pub layout: LayoutConstraints,
}

impl WidgetStyles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fg(mut self, color: Color) -> Self {
        self.style = self.style.fg(color);
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.style = self.style.bg(color);
        self
    }

    pub fn bold(mut self, value: bool) -> Self {
        self.style = self.style.bold(value);
        self
    }

    pub fn dim(mut self, value: bool) -> Self {
        self.style = self.style.dim(value);
        self
    }

    pub fn italic(mut self, value: bool) -> Self {
        self.style = self.style.italic(value);
        self
    }

    pub fn underline(mut self, value: bool) -> Self {
        self.style = self.style.underline(value);
        self
    }

    pub fn border(mut self, value: bool) -> Self {
        self.style = self.style.border(value);
        self
    }

    pub fn set_fg(&mut self, color: Color) {
        self.style = self.style.fg(color);
    }

    pub fn set_bg(&mut self, color: Color) {
        self.style = self.style.bg(color);
    }

    pub fn set_bold(&mut self, value: bool) {
        self.style = self.style.bold(value);
    }

    pub fn set_dim(&mut self, value: bool) {
        self.style = self.style.dim(value);
    }

    pub fn set_italic(&mut self, value: bool) {
        self.style = self.style.italic(value);
    }

    pub fn set_underline(&mut self, value: bool) {
        self.style = self.style.underline(value);
    }

    pub fn set_border(&mut self, value: bool) {
        self.style = self.style.border(value);
    }

    pub fn width(mut self, value: usize) -> Self {
        let value = value.max(1);
        self.layout.min_width = Some(value);
        self.layout.max_width = Some(value);
        self
    }

    pub fn height(mut self, value: usize) -> Self {
        let value = value.max(1);
        self.layout.min_height = Some(value);
        self.layout.max_height = Some(value);
        self
    }

    pub fn min_width(mut self, value: usize) -> Self {
        self.layout.min_width = Some(value.max(1));
        self
    }

    pub fn max_width(mut self, value: usize) -> Self {
        self.layout.max_width = Some(value.max(1));
        self
    }

    pub fn min_height(mut self, value: usize) -> Self {
        self.layout.min_height = Some(value.max(1));
        self
    }

    pub fn max_height(mut self, value: usize) -> Self {
        self.layout.max_height = Some(value.max(1));
        self
    }

    pub fn set_width(&mut self, value: usize) {
        let value = value.max(1);
        self.layout.min_width = Some(value);
        self.layout.max_width = Some(value);
    }

    pub fn set_height(&mut self, value: usize) {
        let value = value.max(1);
        self.layout.min_height = Some(value);
        self.layout.max_height = Some(value);
    }

    pub fn set_min_width(&mut self, value: usize) {
        self.layout.min_width = Some(value.max(1));
    }

    pub fn set_max_width(&mut self, value: usize) {
        self.layout.max_width = Some(value.max(1));
    }

    pub fn set_min_height(&mut self, value: usize) {
        self.layout.min_height = Some(value.max(1));
    }

    pub fn set_max_height(&mut self, value: usize) {
        self.layout.max_height = Some(value.max(1));
    }
}
