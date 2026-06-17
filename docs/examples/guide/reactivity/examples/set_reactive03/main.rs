/// Port of Python Textual `docs/examples/guide/reactivity/set_reactive03.py`.
///
/// Demonstrates a reactive list that grows when names are submitted via Input.
///
/// Python uses `reactive(list, recompose=True)` on the App and `mutate_reactive`
/// to trigger a full `compose()` rebuild when names are appended.  The Rust
/// equivalent achieves the same observable result by mounting a new `Label` widget
/// dynamically each time `InputSubmitted` fires — no recompose needed.
///
/// Behavior mirrors the Python output: the Input sits at the top, and each
/// submitted name spawns a new "Hello, <name>" label below it in submission order.
use textual::prelude::*;

#[derive(Default)]
struct MultiGreet;

impl TextualApp for MultiGreet {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Input::new().with_placeholder("Give me a name"))
    }

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Some(m) = message.downcast_ref::<InputSubmitted>() {
            let name = m.value.clone();
            if !name.is_empty() {
                let label = Label::new(format!("Hello, {name}"));
                let _ = app.mount(label);
                // Clear the input after submission (mirrors Python's implicit clear on Enter).
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
        let mut app = MultiGreet;
        let _root = app.compose();
    }
}
