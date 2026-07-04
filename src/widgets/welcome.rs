use rich_rs::{Console, ConsoleOptions, Segments};
use textual_macros::widget;

use crate::compose::ComposeResult;
use crate::event::Event;
use crate::message::*;

use super::{Button, ButtonVariant, Focus, Interactive, Layout, Markdown, NodeSeed, Render};
use crate::widgets::containers::Container;

const WELCOME_MD: &str = r#"# Welcome!

Textual is a TUI, or *Text User Interface*, framework for Python inspired by modern web development. **We hope you enjoy using Textual!**

## Dune quote

> "I must not fear.
Fear is the mind-killer.
Fear is the little-death that brings total obliteration.
I will face my fear.
I will permit it to pass over me and through me.
And when it has gone past, I will turn the inner eye to see its path.
Where the fear has gone there will be nothing. Only I will remain.""#;

/// Textual welcome widget.
///
/// Composes:
/// - A `Container` (id `"md"`) containing a `Markdown` widget (id `"text"`).
/// - A `Button` (id `"close"`, variant `Success`) docked to the bottom.
///
/// Mirrors Python `textual.widgets.Welcome`:
/// ```python
/// def compose(self) -> ComposeResult:
///     yield Container(Static(Markdown(WELCOME_MD), id="text"), id="md")
///     yield Button("OK", id="close", variant="success")
/// ```
///
/// Because the Button lives in the arena tree, callers can reach it via
/// `app.query_one_typed::<Button>()` and update its label.
#[derive(Clone)]
#[widget(Focus, Interactive, Layout, style_type = "Welcome")]
pub struct Welcome {
    /// Initial label for the close button.  Used when composing the Button
    /// into the arena tree.  After mounting, change the label via
    /// `app.with_query_one_mut_as::<Button, _>("#close", |btn| ...)`.
    close_label: String,
    seed: NodeSeed,
}

impl std::fmt::Debug for Welcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Welcome")
            .field("close_label", &self.close_label)
            .finish()
    }
}

impl Default for Welcome {
    fn default() -> Self {
        Self::new()
    }
}

impl Welcome {
    crate::seed_ident_methods!();

    pub fn new() -> Self {
        let mut seed = NodeSeed::default();
        seed.classes.push("welcome".to_string());
        Self {
            close_label: "OK".to_string(),
            seed,
        }
    }

    pub fn markdown(&self) -> &str {
        WELCOME_MD
    }

    /// Set the initial label for the close button.
    ///
    /// This affects only the label baked into the composed `Button` widget at
    /// mount time.  To change the label after mounting, use the arena query:
    /// ```ignore
    /// app.with_query_one_mut_as::<Button, _>("#close", |btn, ctx| {
    ///     btn.set_label("YES!".to_string(), ctx);
    /// });
    /// ```
    pub fn set_close_label(&mut self, label: impl Into<String>) {
        self.close_label = label.into();
    }
}

impl Render for Welcome {
    fn compose(&mut self) -> ComposeResult {
        let md = Markdown::new(WELCOME_MD);
        let text_node = crate::compose::ChildDecl::new(Box::new(md)).with_id("text");

        let container = Container::new();
        let md_container =
            crate::compose::ChildDecl::new(Box::new(container)).with_id("md").with_children(vec![text_node]);

        let button = Button::new(self.close_label.clone()).variant(ButtonVariant::Success);
        let close_node = crate::compose::ChildDecl::new(Box::new(button)).with_id("close");

        vec![md_container, close_node]
    }

    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        // Children are rendered by the arena tree.  Welcome itself has no
        // direct visual content beyond what its composed children provide.
        Segments::new()
    }
}

impl Focus for Welcome {
    fn focusable(&self) -> bool {
        true
    }
}

impl Interactive for Welcome {
    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        // Welcome itself does not handle key/mouse events directly — the arena
        // tree routes events to its composed children (Button, Markdown).
        // The only special handling needed here is forwarding focus-related
        // state, which the runtime manages via NodeState.
        let _ = (event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut crate::event::WidgetCtx) {
        // Respond to ButtonPressed bubbling up from the child Button (#close).
        // We do not restrict by sender so this works both in the arena tree
        // (sender = Button's arena NodeId) and in unit tests (sender = any id).
        if message.is::<ButtonPressed>() {
            ctx.post_message(ButtonPressed {
                description: "Welcome.close".to_string(),
                button_id: None,
            });
            ctx.post_message(OverlayDismissRequested { overlay: None });
            ctx.set_handled();
        }
    }
}

impl Layout for Welcome {
    fn layout_height(&self) -> Option<usize> {
        None
    }

    fn content_width(&self) -> Option<usize> {
        Some(rich_rs::cell_len("Welcome!").max(8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::message::MessageEvent;
    use crate::node_id::NodeId;

    #[test]
    fn welcome_compose_yields_two_children() {
        let mut welcome = Welcome::new();
        let children = welcome.compose();
        assert_eq!(
            children.len(),
            2,
            "Welcome must compose exactly two children: Container(md) + Button(close)"
        );
    }

    #[test]
    fn welcome_compose_has_md_container_and_close_button() {
        let mut welcome = Welcome::new();
        let children = welcome.compose();
        // First child: Container with id "md"
        assert_eq!(children[0].id.as_deref(), Some("md"));
        // Second child: Button with id "close"
        assert_eq!(children[1].id.as_deref(), Some("close"));
    }

    #[test]
    fn welcome_set_close_label_reflected_in_compose() {
        let mut welcome = Welcome::new();
        welcome.set_close_label("YES!");
        // The second child should still carry id "close".
        let children = welcome.compose();
        assert_eq!(children[1].id.as_deref(), Some("close"));
        // Confirm the label is stored (tested indirectly via compose).
        assert_eq!(welcome.close_label, "YES!");
    }

    #[test]
    fn welcome_on_message_button_pressed_re_emits() {
        let mut welcome = Welcome::new();

        let mut ctx = EventCtx::default();
        // Send ButtonPressed from *any* sender (simulates arena child button).
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            welcome.on_message(
            &MessageEvent::new(
                NodeId::default(),
                ButtonPressed {
                    description: "Button".to_string(),
                    button_id: None,
                },
            ),
            &mut __w);
        }

        assert!(ctx.handled());
        let emitted = ctx.take_messages();
        assert!(
            emitted.iter().any(|e| e
                .downcast_ref::<ButtonPressed>()
                .is_some_and(|bp| bp.description == "Welcome.close")),
            "should re-emit ButtonPressed with description 'Welcome.close'"
        );
        assert!(
            emitted
                .iter()
                .any(|e| e.downcast_ref::<OverlayDismissRequested>().is_some()),
            "should emit OverlayDismissRequested"
        );
    }

    #[test]
    fn welcome_on_message_non_button_pressed_ignored() {
        let mut welcome = Welcome::new();
        let mut ctx = EventCtx::default();
        // A non-ButtonPressed message should not be handled.
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            welcome.on_message(
            &MessageEvent::new(NodeId::default(), InputChanged { value: "x".to_string(), validation: crate::validation::ValidationResult::success() }),
            &mut __w);
        }
        assert!(!ctx.handled());
    }
}
