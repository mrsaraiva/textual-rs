use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::compose::ComposeResult;
use crate::css;
use crate::debug::DebugLayout;
use crate::event::{Event, EventCtx};

use crate::widgets::{
    Widget, WidgetStyles,
    helpers::{
        apply_debug_box, apply_margin, clamp_with_constraints, constraints_from_style,
        fixed_height_from_constraints, margin_from_style, merge_constraints, pad_lines_to_width,
    },
};

pub struct Container {
    children: Vec<Box<dyn Widget>>,
    styles: WidgetStyles,
}

impl Container {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Add multiple children from a `compose![]` result.
    pub fn with_compose(mut self, children: ComposeResult) -> Self {
        for decl in children {
            match decl.builder {
                crate::compose::WidgetBuilder::Ready(widget) => self.children.push(widget),
            }
        }
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.children.push(Box::new(child));
    }

    /// Read-only access to the container's children.
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    /// Mutable access to the container's children.
    pub fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    /// Drain all children, returning them as owned widgets.
    ///
    /// Intended for runtime mount: the runtime can call this once during
    /// tree construction to move children into the `WidgetTree` arena.
    /// After draining, `self.children` is empty.
    pub(crate) fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        std::mem::take(&mut self.children)
    }
}

impl Widget for Container {
    /// Declare children for tree-based mounting.
    ///
    /// TODO(P1-15): Container stores children via `with_child()`/`push()` as
    /// owned `Box<dyn Widget>`. Because `compose()` is `&self`, we cannot move
    /// them into `ChildDecl` entries. Once the runtime supports extracting
    /// children from containers during mount (via `take_composed_children()`),
    /// this will return proper declarations. Until then, render/event methods
    /// continue iterating `self.children` directly.
    fn compose(&self) -> ComposeResult {
        Vec::new()
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);
        let bounds = rich_rs::Region::from_size(width as u32, height_limit as u32);

        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut cursor_y: i32 = 0;

        for child in &self.children {
            let meta = css::selector_meta_generic(child.as_ref());
            let resolved = css::resolve_style(child.as_ref(), &meta);
            let margin = margin_from_style(&resolved);
            let style_constraints = constraints_from_style(&resolved);
            let constraints = merge_constraints(style_constraints, child.layout_constraints());
            let available_width = width
                .saturating_sub(margin.left as usize + margin.right as usize)
                .max(1);
            let mut render_width = clamp_with_constraints(
                available_width,
                constraints.min_width,
                constraints.max_width,
                available_width,
            );
            if matches!(resolved.width, Some(crate::style::Scalar::Auto)) {
                let pad = resolved
                    .padding
                    .map(|s| s.left as usize)
                    .unwrap_or(0)
                    .saturating_mul(2);
                let (_, _, border_left, border_right) =
                    crate::widgets::helpers::border_spacing_from_style(&resolved);
                let intrinsic = child
                    .content_width()
                    .unwrap_or(render_width)
                    .saturating_add(pad + border_left + border_right)
                    .max(1);
                render_width = clamp_with_constraints(
                    intrinsic,
                    constraints.min_width,
                    constraints.max_width,
                    available_width,
                );
            }
            let render_height = clamp_with_constraints(
                height_limit
                    .saturating_sub(margin.top as usize + margin.bottom as usize)
                    .max(1),
                constraints.min_height,
                constraints.max_height,
                height_limit
                    .saturating_sub(margin.top as usize + margin.bottom as usize)
                    .max(1),
            );
            let render_height = if let Some(fixed_total) = child.layout_height() {
                render_height.min(fixed_total.max(1))
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
            child_lines = apply_margin(child_lines, width, margin);
            let child_height = child_lines.len();
            let child_region = rich_rs::Region::new(0, cursor_y, width as u32, child_height as u32);
            if let Some(visible) = child_region.intersection(&bounds) {
                let start = (visible.y - child_region.y).max(0) as usize;
                let end = (start + visible.height as usize).min(child_lines.len());
                for line in child_lines.into_iter().skip(start).take(end - start) {
                    if lines.len() >= height_limit {
                        break;
                    }
                    lines.push(line);
                }
            }
            cursor_y += child_height as i32;
            if cursor_y as usize >= height_limit {
                break;
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

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);
        let bounds = rich_rs::Region::from_size(width as u32, height_limit as u32);

        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut cursor_y: i32 = 0;

        for (idx, child) in self.children.iter().enumerate() {
            let constraints = child.layout_constraints();
            let render_width =
                clamp_with_constraints(width, constraints.min_width, constraints.max_width, width);
            let render_height = clamp_with_constraints(
                height_limit,
                constraints.min_height,
                constraints.max_height,
                height_limit,
            );
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
            child_lines = pad_lines_to_width(child_lines, width);
            let child_height = child_lines.len().max(1);
            let debug_height = (child_height + 2).max(3);
            let child_region = rich_rs::Region::new(0, cursor_y, width as u32, debug_height as u32);
            if let Some(visible) = child_region.intersection(&bounds) {
                let start = (visible.y - child_region.y).max(0) as usize;
                let end = (start + visible.height as usize).min(debug_height);
                let label = if debug.show_sizes {
                    Some(format!("{width}x{debug_height}"))
                } else {
                    None
                };
                let wrapped = apply_debug_box(
                    child_lines,
                    width,
                    debug_height,
                    label.as_deref(),
                    debug.style_for(idx),
                );
                for line in wrapped.into_iter().skip(start).take(end - start) {
                    if lines.len() >= height_limit {
                        break;
                    }
                    lines.push(line);
                }
            }
            cursor_y += debug_height as i32;
            if cursor_y as usize >= height_limit {
                break;
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
        for child in &mut self.children {
            child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        for child in &mut self.children {
            child.on_resize(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        for child in &mut self.children {
            child.on_event_capture(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        for child in &mut self.children {
            child.on_event(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        let mut total = 0usize;
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

    fn content_width(&self) -> Option<usize> {
        let mut widest = 0usize;
        let mut any = false;
        for child in &self.children {
            let meta = css::selector_meta_generic(child.as_ref());
            let resolved = css::resolve_style(child.as_ref(), &meta);
            let margin = margin_from_style(&resolved);
            if let Some(width) = child.content_width() {
                widest =
                    widest.max(width.saturating_add(margin.left as usize + margin.right as usize));
                any = true;
            }
        }
        if any { Some(widest.max(1)) } else { None }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Container {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
