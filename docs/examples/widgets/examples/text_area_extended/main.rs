use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/text_area_extended.py`.
struct TextAreaExtendedApp;

struct AutoParenEditor {
    child: TextArea,
}

impl AutoParenEditor {
    fn new() -> Self {
        Self {
            child: TextArea::code_editor("").with_language("python"),
        }
    }
}

impl Widget for AutoParenEditor {
    fn focusable(&self) -> bool {
        self.child.focusable()
    }

    fn on_node_state_changed(&mut self, old: NodeState, new: NodeState) {
        self.child.on_node_state_changed(old, new);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        if let Event::Key(key) = event
            && key.code == crossterm::event::KeyCode::Char('(')
            && self.node_state().focused
        {
            self.child.insert("()");
            self.child.move_cursor_relative(-1, 0);
            ctx.request_repaint();
            ctx.set_handled();
            return;
        }
        self.child.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.child.on_mouse_move(x, y)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// LIVENESS: focus the AutoParenEditor and type `(`; the custom key hook
    /// auto-inserts the matching `)` and repositions the cursor — turning an
    /// empty editor into `()`, so the rendered frame must change. A dead key
    /// hook (event not intercepted) leaves the frame identical.
    #[test]
    fn liveness_auto_paren_insert() {
        TextAreaExtendedApp
            .run_test(|pilot| {
                pilot.press(&["tab"])?; // focus the editor
                let before = pilot.app().frame_fingerprint();
                pilot.press(&["("])?;
                let after = pilot.app().frame_fingerprint();
                assert_ne!(
                    before, after,
                    "typing `(` must auto-insert `()` (frame changes)"
                );
                Ok(())
            })
            .expect("run_test");
    }
}
