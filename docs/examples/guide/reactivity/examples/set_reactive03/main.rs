/// Port of Python Textual `docs/examples/guide/reactivity/set_reactive03.py`.
///
/// Demonstrates a reactive list that grows when names are submitted via Input,
/// using APP-LEVEL recompose + `mutate_reactive`.
///
/// Python:
///   names: reactive[list[str]] = reactive(list, recompose=True)   # (1)
///   def compose(self):
///       yield Input(placeholder="Give me a name")
///       for name in self.names: yield Label(f"Hello, {name}")
///   def on_input_submitted(self, event):
///       self.names.append(event.value)
///       self.mutate_reactive(MultiGreet.names)   # (2)
///
/// Rust port (faithful): the app derives `Reactive` with
/// `#[reactive(recompose)] names: Vec<String>`. On submit, the name is pushed in
/// place and `mutate_names(ctx)` is called — the generated `mutate_<field>`
/// (Python `mutate_reactive`) records a recompose change unconditionally. The app
/// reactive bridge then re-invokes `compose()` (which now iterates the updated
/// `names`) via `App::recompose_app` — exactly Python's `recompose=True` +
/// `mutate_reactive`.
use textual::prelude::*;

#[derive(Reactive, Default)]
struct MultiGreet {
    #[reactive(recompose)]
    names: Vec<String>,
}

impl TextualApp for MultiGreet {
    fn reactive_widget_mut(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }

    fn compose(&mut self) -> AppRoot {
        // Python: yield Input(...); for name in self.names: yield Label(...).
        let mut root = AppRoot::new().with_child(Input::new().with_placeholder("Give me a name"));
        for name in self.names() {
            root = root.with_child(Label::new(format!("Hello, {name}")));
        }
        root
    }

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Some(m) = message.downcast_ref::<InputSubmitted>() {
            let name = m.value.clone();
            if !name.is_empty() {
                // Python: self.names.append(value); self.mutate_reactive(MultiGreet.names).
                self.names.push(name);
                self.mutate_names(app.reactive_ctx());
                // Clear the input (Python clears on Enter).
                let _ = app.with_query_one_mut_as::<Input, _>("Input", |input| {
                    input.clear();
                });
                ctx.request_repaint();
                ctx.set_handled();
            }
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(MultiGreet::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_greet_composes_without_panic() {
        let mut app = MultiGreet::default();
        let _root = app.compose();
    }

    #[test]
    fn compose_yields_a_label_per_name() {
        let mut app = MultiGreet {
            names: vec!["Ada".to_string(), "Linus".to_string()],
        };
        let root = app.compose();
        // Input + one Label per name = 3 top-level children.
        assert_eq!(root.children().len(), 3);
    }

    #[test]
    fn mutate_names_requests_recompose() {
        let mut app = MultiGreet::default();
        app.names.push("Ada".to_string());
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        app.mutate_names(&mut ctx);
        assert!(ctx.has_changes());
        assert!(ctx.needs_recompose(), "mutate of a recompose reactive must request recompose");
    }
}
