macro_rules! delegate_widget_to {
    ($wrapper:ty, $field:ident) => {
        impl Widget for $wrapper {
            fn compose(&self) -> crate::compose::ComposeResult {
                self.$field.compose()
            }

            fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
                self.$field.take_composed_children()
            }

            fn focusable(&self) -> bool {
                self.$field.focusable()
            }

            fn set_focus(&mut self, focused: bool) {
                self.$field.set_focus(focused);
            }

            fn has_focus(&self) -> bool {
                self.$field.has_focus()
            }

            fn render(
                &self,
                console: &rich_rs::Console,
                options: &rich_rs::ConsoleOptions,
            ) -> rich_rs::Segments {
                self.$field.render(console, options)
            }

            fn render_with_debug(
                &self,
                console: &rich_rs::Console,
                options: &rich_rs::ConsoleOptions,
                debug: &crate::debug::DebugLayout,
            ) -> rich_rs::Segments {
                self.$field.render_with_debug(console, options, debug)
            }

            fn on_mount(&mut self) {
                self.$field.on_mount();
            }

            fn on_unmount(&mut self) {
                self.$field.on_unmount();
            }

            fn on_tick(&mut self, tick: u64) {
                self.$field.on_tick(tick);
            }

            fn on_resize(&mut self, width: u16, height: u16) {
                self.$field.on_resize(width, height);
            }

            fn on_layout(&mut self, width: u16, height: u16) {
                self.$field.on_layout(width, height);
            }

            fn on_event_capture(
                &mut self,
                event: &crate::event::Event,
                ctx: &mut crate::event::EventCtx,
            ) {
                self.$field.on_event_capture(event, ctx);
            }

            fn on_event(&mut self, event: &crate::event::Event, ctx: &mut crate::event::EventCtx) {
                self.$field.on_event(event, ctx);
            }

            fn on_mouse_scroll(
                &mut self,
                delta_x: i32,
                delta_y: i32,
                ctx: &mut crate::event::EventCtx,
            ) {
                self.$field.on_mouse_scroll(delta_x, delta_y, ctx);
            }

            fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
                self.$field.on_mouse_move(x, y)
            }

            fn layout_height(&self) -> Option<usize> {
                self.$field.layout_height()
            }

            fn content_width(&self) -> Option<usize> {
                self.$field.content_width()
            }

            fn scroll_offset(&self) -> (usize, usize) {
                self.$field.scroll_offset()
            }

            fn clips_descendants_to_content(&self) -> bool {
                self.$field.clips_descendants_to_content()
            }

            fn bindings(&self) -> Vec<crate::widgets::BindingDecl> {
                self.$field.bindings()
            }

            fn styles(&self) -> Option<&crate::widgets::WidgetStyles> {
                self.$field.styles()
            }

            fn styles_mut(&mut self) -> Option<&mut crate::widgets::WidgetStyles> {
                self.$field.styles_mut()
            }
        }
    };
}

pub(crate) use delegate_widget_to;

pub(crate) fn align_line_horizontal(
    line: &[rich_rs::Segment],
    width: usize,
    child_width: usize,
    offset: usize,
) -> Vec<rich_rs::Segment> {
    let width = width.max(1);
    let child_width = child_width.max(1).min(width);
    let offset = offset.min(width.saturating_sub(child_width));
    let mut out = Vec::new();
    if offset > 0 {
        out.push(rich_rs::Segment::new(" ".repeat(offset)));
    }
    out.extend(crate::widgets::helpers::adjust_line_length_no_bg(
        line,
        child_width,
    ));
    let tail = width.saturating_sub(offset + child_width);
    if tail > 0 {
        out.push(rich_rs::Segment::new(" ".repeat(tail)));
    }
    out
}

pub(crate) fn effective_rendered_height(lines: &[Vec<rich_rs::Segment>]) -> usize {
    let last_non_blank = lines.iter().rposition(|line| {
        line.iter()
            .filter(|segment| !segment.is_control())
            .any(|segment| segment.text.chars().any(|ch| ch != ' '))
    });
    last_non_blank.map(|idx| idx + 1).unwrap_or(1)
}
