//! Regression: focus transfers when the FOCUSED widget becomes hidden.
//!
//! Python parity (`Widget._on_hide` -> `Screen._reset_focus`, `screen.py`):
//! when the focused widget stops being displayed (e.g. a class op flips its
//! CSS `display` to `none`), focus moves to the first shown focusable sibling;
//! with no candidate, focus is cleared. Tutorial `stopwatch04` depends on
//! this: clicking `#start` adds `.started` to the Stopwatch, hiding `#start`
//! and revealing `#stop` — Python focuses `#stop` so it renders with the
//! `Button:focus` background tint.
//!
//! Written against GENERIC synthetic widgets (not `Button`/`Stopwatch`) so
//! widget refactors cannot mask a regression here.

#![cfg(test)]

use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::compose::{ChildDecl, ComposeResult};
use crate::event::{Event, WidgetCtx};
use crate::widgets::{AppRoot, NodeSeed, Widget};
use crate::{App, TextualApp};

/// A focusable 1-line leaf with a CSS id.
struct HideLeaf {
    seed: NodeSeed,
}

impl HideLeaf {
    fn new(id: &str) -> Self {
        Self {
            seed: NodeSeed {
                css_id: Some(id.to_string()),
                ..NodeSeed::default()
            },
        }
    }
}

impl Widget for HideLeaf {
    fn focusable(&self) -> bool {
        true
    }
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }
    fn style_type(&self) -> &'static str {
        "HideLeaf"
    }
    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }
    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }
}

impl Renderable for HideLeaf {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

/// Parent container: on `s`, adds `-started` to itself (bubbled from the
/// focused child), which per the stylesheet hides `#start` and reveals
/// `#stop` — the stopwatch pattern.
struct HideStack {
    children: Vec<&'static str>,
    seed: NodeSeed,
}

impl HideStack {
    fn new(children: Vec<&'static str>) -> Self {
        Self {
            children,
            seed: NodeSeed::default(),
        }
    }
}

impl Widget for HideStack {
    fn compose(&mut self) -> ComposeResult {
        self.children
            .iter()
            .map(|id| ChildDecl::new(Box::new(HideLeaf::new(id))))
            .collect()
    }
    fn on_event(&mut self, event: &Event, ctx: &mut WidgetCtx) {
        if let Event::Key(key) = event
            && key.code == KeyCode::Char('s')
        {
            // Deferred class op: applied by the post-dispatch command flush.
            ctx.add_class("-started");
            ctx.set_handled();
        }
    }
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }
    fn style_type(&self) -> &'static str {
        "HideStack"
    }
    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }
    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
    fn layout_height(&self) -> Option<usize> {
        Some(4)
    }
}

impl Renderable for HideStack {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

struct HideApp {
    css: &'static str,
    children: Vec<&'static str>,
}

impl TextualApp for HideApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(HideStack::new(self.children.clone()))
    }
    fn configure(&mut self, app: &mut App) -> crate::Result<()> {
        app.load_stylesheet(self.css);
        Ok(())
    }
}

fn focused_node(app: &App) -> Option<crate::node_id::NodeId> {
    app.active_widget_tree()
        .and_then(super::routing::focused_node_id_tree)
}

/// Stopwatch pattern: hiding focused `#start` reveals `#stop` — focus must
/// transfer to `#stop` (the first SHOWN focusable sibling; `#reset` flips to
/// `visibility: hidden` at the same time and must be skipped).
#[test]
fn hiding_focused_widget_moves_focus_to_shown_sibling() {
    let app = HideApp {
        css: "#stop { display: none; } \
              HideStack.-started #start { display: none; } \
              HideStack.-started #stop { display: block; } \
              HideStack.-started #reset { visibility: hidden; }",
        children: vec!["start", "stop", "reset"],
    };
    crate::run_test(app, |pilot| {
        pilot.app_mut().action_focus("start").unwrap();
        pilot.pause()?;
        let start = pilot.app().query_one("#start").unwrap();
        assert_eq!(
            focused_node(pilot.app()),
            Some(start),
            "precondition: #start is focused before it hides"
        );

        pilot.press_key("s")?;

        let stop = pilot.app().query_one("#stop").unwrap();
        let tree = pilot.app().active_widget_tree().unwrap();
        assert!(
            !tree.is_displayed(start),
            "#start must be display:none after -started"
        );
        assert!(
            tree.is_displayed(stop),
            "#stop must be displayed after -started"
        );
        assert_eq!(
            focused_node(pilot.app()),
            Some(stop),
            "focus must transfer to the revealed #stop when focused #start hides \
             (Python Screen._reset_focus sibling branch)"
        );
        Ok(())
    })
    .unwrap();
}

/// With no shown focusable sibling, hiding the focused widget clears focus
/// (Python `Screen._reset_focus` -> `set_focus(None)`).
#[test]
fn hiding_focused_widget_with_no_candidate_clears_focus() {
    let app = HideApp {
        css: "HideStack.-started #start { display: none; }",
        children: vec!["start"],
    };
    crate::run_test(app, |pilot| {
        pilot.app_mut().action_focus("start").unwrap();
        pilot.pause()?;
        let start = pilot.app().query_one("#start").unwrap();
        assert_eq!(focused_node(pilot.app()), Some(start));

        pilot.press_key("s")?;

        let tree = pilot.app().active_widget_tree().unwrap();
        assert!(!tree.is_displayed(start));
        assert_eq!(
            focused_node(pilot.app()),
            None,
            "focus must clear when the hidden widget has no shown focusable sibling"
        );
        Ok(())
    })
    .unwrap();
}
