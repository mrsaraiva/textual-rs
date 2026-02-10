use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/text_area_extended.py`.
struct TextAreaExtendedApp;

struct AutoParenEditor {
    id: WidgetId,
    child: TextArea,
}

impl AutoParenEditor {
    fn new() -> Self {
        Self {
            id: WidgetId::new(),
            child: TextArea::code_editor("").with_language("python"),
        }
    }
}

impl Widget for AutoParenEditor {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        self.child.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.child.set_focus(focused);
    }

    fn has_focus(&self) -> bool {
        self.child.has_focus()
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::Key(key) = event {
            if key.code == crossterm::event::KeyCode::Char('(') && self.child.has_focus() {
                self.child.insert("()");
                self.child.move_cursor_relative(-1, 0);
                ctx.request_repaint();
                ctx.set_handled();
                return;
            }
        }
        self.child.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.child.on_layout(width, height);
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.child.render_styled(console, options)
    }
}

impl TextualApp for TextAreaExtendedApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(AutoParenEditor::new())
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(TextAreaExtendedApp)
}
