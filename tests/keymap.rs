//! Keymap subsystem end-to-end tests (SPEC_keymap phase K2).
//!
//! Ports of `test_keymap.py` (replace / unknown id / inherited same id /
//! different id / pre-mount) and the `test_binding.py` normalization case,
//! plus Rust-specific regression guards for the two keymap transforms
//! (dispatch flatten vs shape-preserving hint substitution) and the 1.0.2
//! binding fixes (priority order, char-key normalization, check_action
//! gating) under a non-empty keymap.
//!
//! Python's `Counter` fixture sets the keymap in `on_mount`; the Rust ports
//! use the declarative `TextualApp::keymap()` hook (read once at startup),
//! which is the same pre-dispatch timing.

use std::sync::{Arc, Mutex};

use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use textual::action::ParsedAction;
use textual::compose;
use textual::prelude::*;

fn km(pairs: &[(&str, &str)]) -> Keymap {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

// ---------------------------------------------------------------------------
// Counter fixture (test_keymap.py::Counter)
// ---------------------------------------------------------------------------

struct CounterApp {
    count: Arc<Mutex<i32>>,
    keymap: Keymap,
}

impl CounterApp {
    fn new(count: Arc<Mutex<i32>>, keymap: Keymap) -> Self {
        Self { count, keymap }
    }
}

impl TextualApp for CounterApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Label::new("foo"))
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("i,up", "increment", "").with_id("app.increment"),
            BindingDecl::new("d,down", "decrement", "").with_id("app.decrement"),
        ]
    }

    fn keymap(&self) -> Keymap {
        self.keymap.clone()
    }

    fn on_app_action_str(&mut self, _app: &mut App, action: &str, ctx: &mut WidgetCtx) {
        match action {
            "increment" => {
                *self.count.lock().unwrap_or_else(|e| e.into_inner()) += 1;
                ctx.set_handled();
            }
            "decrement" => {
                *self.count.lock().unwrap_or_else(|e| e.into_inner()) -= 1;
                ctx.set_handled();
            }
            _ => {}
        }
    }
}

// Port of test_keymap.py::test_keymap_default_binding_replaces_old_binding.
#[test]
fn keymap_default_binding_replaces_old_binding() {
    let count: Arc<Mutex<i32>> = Arc::new(Mutex::new(0));
    let observed = Arc::clone(&count);
    run_test(
        CounterApp::new(count, km(&[("app.increment", "right,k")])),
        |pilot| {
            // The original bindings are removed - action not called.
            pilot.press(&["i", "up"])?;
            assert_eq!(*observed.lock().unwrap_or_else(|e| e.into_inner()), 0);

            // The new bindings are active and call the action.
            pilot.press(&["right", "k"])?;
            assert_eq!(*observed.lock().unwrap_or_else(|e| e.into_inner()), 2);
            Ok(())
        },
    )
    .expect("run_test");
}

// Port of test_keymap.py::test_keymap_with_unknown_id_is_noop.
#[test]
fn keymap_with_unknown_id_is_noop() {
    let count: Arc<Mutex<i32>> = Arc::new(Mutex::new(0));
    let observed = Arc::clone(&count);
    run_test(
        CounterApp::new(count, km(&[("this.is.an.unknown.id", "d")])),
        |pilot| {
            pilot.press(&["d"])?;
            assert_eq!(*observed.lock().unwrap_or_else(|e| e.into_inner()), -1);
            Ok(())
        },
    )
    .expect("run_test");
}

// A keymap that matches no declared id must leave resolution byte-identical
// to the no-keymap walk (guards the fast/slow path split): defaults still
// fire exactly as before.
#[test]
fn keymap_present_but_irrelevant_keeps_default_resolution() {
    let count: Arc<Mutex<i32>> = Arc::new(Mutex::new(0));
    let observed = Arc::clone(&count);
    run_test(
        CounterApp::new(count, km(&[("unrelated.id", "z")])),
        |pilot| {
            pilot.press(&["i", "up", "d"])?;
            assert_eq!(
                *observed.lock().unwrap_or_else(|e| e.into_inner()),
                1,
                "defaults must resolve unchanged through the slow path (+1 +1 -1)"
            );
            Ok(())
        },
    )
    .expect("run_test");
}

// A keymap value declared as a punctuation character ("?") must match the
// pressed key (which carries the long name "question_mark"): set_keymap
// normalizes values at store time.
#[test]
fn punctuation_keymap_value_matches_pressed_key() {
    let count: Arc<Mutex<i32>> = Arc::new(Mutex::new(0));
    let observed = Arc::clone(&count);
    run_test(
        CounterApp::new(count, km(&[("app.increment", "?")])),
        |pilot| {
            pilot.press(&["?"])?;
            assert_eq!(*observed.lock().unwrap_or_else(|e| e.into_inner()), 1);
            // The default keys were replaced.
            pilot.press(&["i"])?;
            assert_eq!(*observed.lock().unwrap_or_else(|e| e.into_inner()), 1);
            Ok(())
        },
    )
    .expect("run_test");
}

// ---------------------------------------------------------------------------
// Widget-level fixtures (test_keymap.py Parent/Child)
// ---------------------------------------------------------------------------

/// A focusable widget declaring an id-carrying "x" -> increment binding
/// (the Rust analog of test_keymap.py's `Parent(Widget, can_focus=True)`).
struct IncrementWidget {
    binding_id: &'static str,
    counter: Arc<Mutex<i32>>,
}

impl Widget for IncrementWidget {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let mut out = Segments::new();
        out.push(Segment::new(" ".repeat(options.size.0.max(1))));
        out
    }

    fn focusable(&self) -> bool {
        true
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("x", "increment", "").with_id(self.binding_id)]
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut WidgetCtx) -> bool {
        if action.name == "increment" {
            *self.counter.lock().unwrap_or_else(|e| e.into_inner()) += 1;
            ctx.set_handled();
            return true;
        }
        false
    }
}

struct TwoWidgetApp {
    parent_id: &'static str,
    child_id: &'static str,
    parent_counter: Arc<Mutex<i32>>,
    child_counter: Arc<Mutex<i32>>,
    keymap: Keymap,
}

impl TextualApp for TwoWidgetApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_compose(compose![
            IncrementWidget {
                binding_id: self.parent_id,
                counter: Arc::clone(&self.parent_counter),
            },
            IncrementWidget {
                binding_id: self.child_id,
                counter: Arc::clone(&self.child_counter),
            },
        ])
    }

    fn keymap(&self) -> Keymap {
        self.keymap.clone()
    }
}

// Port of test_keymap.py::test_keymap_inherited_bindings_same_id: two widgets
// declaring a binding with the SAME id are both overridden by the keymap.
// (Rust has no class-hierarchy BINDINGS merge; both nodes simply declare the
// id-carrying binding, which is the resolved shape Python ends up with.)
// Like Python, the first focusable widget (the "parent") holds initial focus.
#[test]
fn keymap_inherited_bindings_same_id() {
    let parent_counter: Arc<Mutex<i32>> = Arc::new(Mutex::new(0));
    let child_counter: Arc<Mutex<i32>> = Arc::new(Mutex::new(0));
    let parent_observed = Arc::clone(&parent_counter);
    let child_observed = Arc::clone(&child_counter);
    let app = TwoWidgetApp {
        parent_id: "increment",
        child_id: "increment",
        parent_counter,
        child_counter,
        keymap: km(&[("increment", "i")]),
    };
    run_test(app, |pilot| {
        let parent = |o: &Arc<Mutex<i32>>| *o.lock().unwrap_or_else(|e| e.into_inner());

        // Default binding is unbound due to keymap.
        pilot.press(&["x"])?;
        assert_eq!(parent(&parent_observed), 0);
        assert_eq!(parent(&child_observed), 0);

        // New binding is active, parent is focused - action called.
        pilot.press(&["i"])?;
        assert_eq!(parent(&parent_observed), 1);
        assert_eq!(parent(&child_observed), 0);

        // Tab to focus the child.
        pilot.press(&["tab"])?;

        // Default binding results in no change.
        pilot.press(&["x"])?;
        assert_eq!(parent(&parent_observed), 1);
        assert_eq!(parent(&child_observed), 0);

        // New binding is active, child is focused - action called.
        pilot.press(&["i"])?;
        assert_eq!(parent(&parent_observed), 1);
        assert_eq!(parent(&child_observed), 1);
        Ok(())
    })
    .expect("run_test");
}

// Port of test_keymap.py::test_keymap_child_with_different_id_overridden:
// overriding one widget's binding does not influence a sibling binding with
// a different id.
#[test]
fn keymap_child_with_different_id_overridden() {
    let parent_counter: Arc<Mutex<i32>> = Arc::new(Mutex::new(0));
    let child_counter: Arc<Mutex<i32>> = Arc::new(Mutex::new(0));
    let parent_observed = Arc::clone(&parent_counter);
    let child_observed = Arc::clone(&child_counter);
    let app = TwoWidgetApp {
        parent_id: "parent.increment",
        child_id: "child.increment",
        parent_counter,
        child_counter,
        keymap: km(&[("parent.increment", "i")]),
    };
    run_test(app, |pilot| {
        let count = |o: &Arc<Mutex<i32>>| *o.lock().unwrap_or_else(|e| e.into_inner());
        // Default binding is unbound due to keymap.
        pilot.press(&["x"])?;
        assert_eq!(count(&parent_observed), 0);
        assert_eq!(count(&child_observed), 0);

        // New binding is active, parent is focused - action called.
        pilot.press(&["i"])?;
        assert_eq!(count(&parent_observed), 1);
        assert_eq!(count(&child_observed), 0);

        // Tab to focus the child.
        pilot.press(&["tab"])?;

        // Default binding is still active on the child.
        pilot.press(&["x"])?;
        assert_eq!(count(&parent_observed), 1);
        assert_eq!(count(&child_observed), 1);

        // The keymap only affects the parent id; pressing its key with the
        // child focused does nothing.
        pilot.press(&["i"])?;
        assert_eq!(count(&parent_observed), 1);
        assert_eq!(count(&child_observed), 1);
        Ok(())
    })
    .expect("run_test");
}

// ---------------------------------------------------------------------------
// Pre-mount + normalization (test_keymap.py:197-219, test_binding.py:132-139)
// ---------------------------------------------------------------------------

// Port of test_keymap.py::test_set_keymap_before_app_mount: the keymap can be
// configured before mount (declaratively via `TextualApp::keymap()`, the Rust
// analog of calling `update_keymap` in `__init__`).
#[test]
fn set_keymap_before_app_mount() {
    struct PreMountApp {
        worked: Arc<Mutex<bool>>,
    }

    impl TextualApp for PreMountApp {
        fn compose(&mut self) -> AppRoot {
            AppRoot::new().with_child(Label::new("pre-mount"))
        }

        fn bindings(&self) -> Vec<BindingDecl> {
            vec![BindingDecl::new("x", "test", "").with_id("test")]
        }

        fn keymap(&self) -> Keymap {
            km(&[("test", "y")])
        }

        fn on_app_action_str(&mut self, _app: &mut App, action: &str, ctx: &mut WidgetCtx) {
            if action == "test" {
                *self.worked.lock().unwrap_or_else(|e| e.into_inner()) = true;
                ctx.set_handled();
            }
        }
    }

    let worked: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
    let observed = Arc::clone(&worked);
    run_test(PreMountApp { worked }, |pilot| {
        pilot.press(&["y"])?;
        Ok(())
    })
    .expect("run_test");
    assert!(*observed.lock().unwrap_or_else(|e| e.into_inner()));
}

// Port of test_binding.py::test_keymap_key: set_keymap/update_keymap
// normalize single-character values to long key names at store time.
#[test]
fn keymap_key_values_are_normalized() {
    struct PlainApp;
    impl TextualApp for PlainApp {
        fn compose(&mut self) -> AppRoot {
            AppRoot::new().with_child(Label::new("plain"))
        }
    }

    run_test(PlainApp, |pilot| {
        pilot.app_mut().set_keymap([("foo", "?,space")]);
        assert_eq!(pilot.app().keymap(), &km(&[("foo", "question_mark,space")]));

        pilot.app_mut().update_keymap([("bar", "$")]);
        assert_eq!(
            pilot.app().keymap(),
            &km(&[("bar", "dollar_sign"), ("foo", "question_mark,space")])
        );
        Ok(())
    })
    .expect("run_test");
}

// ---------------------------------------------------------------------------
// 1.0.2 regression guards under a NON-EMPTY keymap (spec 4.1 / 6.2)
// ---------------------------------------------------------------------------

// A remapped PRIORITY binding is still resolved in the priority phase: the
// app-level priority binding beats the focused widget's normal binding for
// the same key (guards 1.0.2 ordering through the slow path).
#[test]
fn remapped_priority_binding_still_beats_focused_normal_binding() {
    struct RecordingWidget {
        records: Arc<Mutex<Vec<String>>>,
    }

    impl Widget for RecordingWidget {
        fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
            let mut out = Segments::new();
            out.push(Segment::new(" ".repeat(options.size.0.max(1))));
            out
        }

        fn focusable(&self) -> bool {
            true
        }

        fn bindings(&self) -> Vec<BindingDecl> {
            vec![BindingDecl::new("z", "widget_act", "")]
        }

        fn execute_action(&mut self, action: &ParsedAction, ctx: &mut WidgetCtx) -> bool {
            if action.name == "widget_act" {
                self.records
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push("widget_act".to_string());
                ctx.set_handled();
                return true;
            }
            false
        }
    }

    struct PriorityApp {
        records: Arc<Mutex<Vec<String>>>,
    }

    impl TextualApp for PriorityApp {
        fn compose(&mut self) -> AppRoot {
            AppRoot::new().with_child(RecordingWidget {
                records: Arc::clone(&self.records),
            })
        }

        fn bindings(&self) -> Vec<BindingDecl> {
            vec![
                BindingDecl::new("p", "app_prio", "")
                    .with_id("app.prio")
                    .priority(),
            ]
        }

        fn keymap(&self) -> Keymap {
            km(&[("app.prio", "z")])
        }

        fn on_app_action_str(&mut self, _app: &mut App, action: &str, ctx: &mut WidgetCtx) {
            if action == "app_prio" {
                self.records
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push("app_prio".to_string());
                ctx.set_handled();
            }
        }
    }

    let records: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&records);
    run_test(PriorityApp { records }, |pilot| {
        // The widget holds initial focus (first focusable).
        pilot.press(&["z"])?;
        Ok(())
    })
    .expect("run_test");
    assert_eq!(
        *observed.lock().unwrap_or_else(|e| e.into_inner()),
        vec!["app_prio".to_string()],
        "the remapped app priority binding must win the priority phase over \
         the focused widget's normal binding on the same key"
    );
}

// A check_action-gated action stays gated after its binding is remapped
// (the gate is consulted on the post-keymap binding).
#[test]
fn check_action_gate_still_applies_to_remapped_binding() {
    struct GatedApp {
        fired: Arc<Mutex<Vec<String>>>,
    }

    impl TextualApp for GatedApp {
        fn compose(&mut self) -> AppRoot {
            AppRoot::new().with_child(Label::new("gated"))
        }

        fn bindings(&self) -> Vec<BindingDecl> {
            vec![
                BindingDecl::new("g", "gated", "").with_id("app.gated"),
                BindingDecl::new("o", "open", "").with_id("app.open"),
            ]
        }

        fn keymap(&self) -> Keymap {
            km(&[("app.gated", "h"), ("app.open", "j")])
        }

        fn check_action(
            &self,
            action: &str,
            _parameters: &[textual::action::ActionArgument],
        ) -> Option<bool> {
            if action == "gated" {
                return Some(false);
            }
            Some(true)
        }

        fn on_app_action_str(&mut self, _app: &mut App, action: &str, ctx: &mut WidgetCtx) {
            if action == "gated" || action == "open" {
                self.fired
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(action.to_string());
                ctx.set_handled();
            }
        }
    }

    let fired: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&fired);
    run_test(GatedApp { fired }, |pilot| {
        pilot.press(&["h", "j"])?;
        Ok(())
    })
    .expect("run_test");
    assert_eq!(
        *observed.lock().unwrap_or_else(|e| e.into_inner()),
        vec!["open".to_string()],
        "the remapped gated binding must stay suppressed by check_action; \
         the remapped allowed binding must fire"
    );
}

// ---------------------------------------------------------------------------
// Footer / hint-path shape preservation (spec 3.6 / 6.2)
// ---------------------------------------------------------------------------

struct FooterApp {
    keymap: Keymap,
}

impl TextualApp for FooterApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_compose(compose![Label::new("body"), Footer::new()])
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("i,up", "increment", "Increment").with_id("app.increment")]
    }

    fn keymap(&self) -> Keymap {
        self.keymap.clone()
    }
}

// A multi-key decl under an IRRELEVANT keymap must produce exactly ONE
// footer entry displaying the FIRST alternative (pins the no-doubling
// requirement: the hint path substitutes, it does not flatten).
#[test]
fn multi_key_binding_shows_one_footer_entry_under_irrelevant_keymap() {
    run_test(
        FooterApp {
            keymap: km(&[("unrelated.id", "z")]),
        },
        |pilot| {
            pilot.pause()?;
            let text = pilot.app().frame_plain_text();
            assert_eq!(
                text.matches("Increment").count(),
                1,
                "exactly one footer row for the multi-key decl; got:\n{text}"
            );
            assert!(
                !text.contains("up"),
                "the footer must display the FIRST alternative ('i'), not 'up'; got:\n{text}"
            );
            Ok(())
        },
    )
    .expect("run_test");
}

// Footer hints must reflect the remapped keys (choke-point site 3), and a
// binding remapped to a punctuation key must render as its symbol
// (format_key_display("question_mark") -> "?").
#[test]
fn footer_shows_remapped_punctuation_key() {
    run_test(
        FooterApp {
            keymap: km(&[("app.increment", "?,space")]),
        },
        |pilot| {
            pilot.pause()?;
            let text = pilot.app().frame_plain_text();
            assert_eq!(
                text.matches("Increment").count(),
                1,
                "the remapped multi-key value must still be ONE footer row; got:\n{text}"
            );
            assert!(
                text.contains('?'),
                "the remapped punctuation key must render as its symbol; got:\n{text}"
            );
            Ok(())
        },
    )
    .expect("run_test");
}

// ---------------------------------------------------------------------------
// Phase K3: clash delivery + forced-rebroadcast signal semantics
// ---------------------------------------------------------------------------

/// Counter app that records `handle_bindings_clash` payloads and call count
/// (the Rust `Counter` fixture of test_keymap.py with clash capture).
struct ClashApp {
    count: Arc<Mutex<i32>>,
    clashes: Arc<Mutex<Vec<BindingClash>>>,
    calls: Arc<Mutex<usize>>,
}

impl TextualApp for ClashApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Label::new("foo"))
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("i,up", "increment", "").with_id("app.increment"),
            BindingDecl::new("d,down", "decrement", "").with_id("app.decrement"),
        ]
    }

    fn keymap(&self) -> Keymap {
        km(&[("app.increment", "d")])
    }

    fn handle_bindings_clash(&mut self, clashed: &[BindingClash]) {
        *self.calls.lock().unwrap_or_else(|e| e.into_inner()) += 1;
        *self.clashes.lock().unwrap_or_else(|e| e.into_inner()) = clashed.to_vec();
    }

    fn on_app_action_str(&mut self, _app: &mut App, action: &str, ctx: &mut WidgetCtx) {
        match action {
            "increment" => {
                *self.count.lock().unwrap_or_else(|e| e.into_inner()) += 1;
                ctx.set_handled();
            }
            "decrement" => {
                *self.count.lock().unwrap_or_else(|e| e.into_inner()) -= 1;
                ctx.set_handled();
            }
            _ => {}
        }
    }
}

// Port of test_keymap.py::test_keymap_sends_message_when_clash: the pinned
// clash is the SELF-clash of the remapped binding (comma-expansion sibling),
// not the decrement binding that originally owned "d" (that one is displaced
// without clashing because its entry is simply deleted). Verbatim payload:
// key "d", action "increment", id "app.increment", declared on the app node.
#[test]
fn keymap_clash_reports_verbatim_self_clash() {
    let count: Arc<Mutex<i32>> = Arc::new(Mutex::new(0));
    let clashes: Arc<Mutex<Vec<BindingClash>>> = Arc::new(Mutex::new(Vec::new()));
    let calls: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    let (count_o, clashes_o, calls_o) = (
        Arc::clone(&count),
        Arc::clone(&clashes),
        Arc::clone(&calls),
    );
    run_test(
        ClashApp {
            count,
            clashes,
            calls,
        },
        |pilot| {
            pilot.press(&["d"])?;

            let observed = clashes_o.lock().unwrap_or_else(|e| e.into_inner()).clone();
            assert_eq!(observed.len(), 1, "exactly one clash reported: {observed:?}");
            let clash = &observed[0];
            assert_eq!(clash.binding.key, "d");
            assert_eq!(clash.binding.action, "increment");
            assert_eq!(clash.binding.id.as_deref(), Some("app.increment"));
            // Python asserts `clashed_node is app`: the clash was declared on
            // the app node, the root of the active (app) tree.
            assert_eq!(clash.source, BindingSource::Active);
            let app_node = pilot
                .app()
                .query_one("ClashApp")
                .expect("app root node addressable by its type name");
            assert_eq!(clash.node, app_node, "the clashed node must be the app node");
            // The remapped increment binding fired on "d".
            assert_eq!(*count_o.lock().unwrap_or_else(|e| e.into_inner()), 1);
            assert_eq!(*calls_o.lock().unwrap_or_else(|e| e.into_inner()), 1);

            // Cadence: the hook fires per clashing KEYPRESS, never from idle
            // loop passes (the hint pass produces no clash information).
            pilot.pause()?;
            pilot.pause()?;
            pilot.pause()?;
            assert_eq!(
                *calls_o.lock().unwrap_or_else(|e| e.into_inner()),
                1,
                "idle frames must not re-fire the clash hook"
            );
            pilot.press(&["d"])?;
            assert_eq!(
                *calls_o.lock().unwrap_or_else(|e| e.into_inner()),
                2,
                "each clashing keypress fires the hook once"
            );
            Ok(())
        },
    )
    .expect("run_test");
}

// Clash node identity under an ACTIVE SCREEN: a clash on an app-root binding
// while a screen is pushed reports `BindingSource::AppRoot` (the two-tree
// discriminant), not an active-tree node.
#[test]
fn keymap_clash_under_active_screen_reports_app_root_source() {
    struct PlainScreen;
    impl Screen for PlainScreen {
        fn name(&self) -> &str {
            "PlainScreen"
        }
        fn compose(&self) -> Box<dyn Widget> {
            Box::new(Label::new("screen body"))
        }
    }

    let count: Arc<Mutex<i32>> = Arc::new(Mutex::new(0));
    let clashes: Arc<Mutex<Vec<BindingClash>>> = Arc::new(Mutex::new(Vec::new()));
    let calls: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    let clashes_o = Arc::clone(&clashes);
    run_test(
        ClashApp {
            count,
            clashes,
            calls,
        },
        |pilot| {
            pilot.app_mut().push_screen(Box::new(PlainScreen));
            pilot.pause()?;
            pilot.press(&["d"])?;

            let observed = clashes_o.lock().unwrap_or_else(|e| e.into_inner()).clone();
            assert_eq!(observed.len(), 1, "exactly one clash reported: {observed:?}");
            let clash = &observed[0];
            assert_eq!(
                clash.source,
                BindingSource::AppRoot,
                "under an active screen the app-node clash must carry the \
                 AppRoot discriminant, not an active-tree node id"
            );
            assert_eq!(clash.binding.key, "d");
            assert_eq!(clash.binding.id.as_deref(), Some("app.increment"));
            Ok(())
        },
    )
    .expect("run_test");
}

// Port of test_binding.py::test_keymap_update: every `set_keymap` call
// rebroadcasts `Event::BindingsChanged`, even when the keymap is identical
// (Python publishes bindings_updated_signal unconditionally; Rust's
// refresh_bindings() clears the hint caches, forcing the diff to report a
// change). Written in the TextualApp idiom: the adapter's default bindings
// guarantee a non-empty hint set, which the forced rebroadcast requires
// (a bare zero-binding harness app would diff empty-vs-empty and stay
// silent).
#[test]
fn set_keymap_rebroadcasts_bindings_changed_even_when_identical() {
    struct BroadcastProbe {
        hits: Arc<Mutex<usize>>,
    }

    impl Widget for BroadcastProbe {
        fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
            let mut out = Segments::new();
            out.push(Segment::new(" ".repeat(options.size.0.max(1))));
            out
        }

        fn on_event(&mut self, event: &Event, _ctx: &mut WidgetCtx) {
            if matches!(event, Event::BindingsChanged(_)) {
                *self.hits.lock().unwrap_or_else(|e| e.into_inner()) += 1;
            }
        }
    }

    struct KeymapUpdateApp {
        hits: Arc<Mutex<usize>>,
    }

    impl TextualApp for KeymapUpdateApp {
        fn compose(&mut self) -> AppRoot {
            AppRoot::new().with_child(BroadcastProbe {
                hits: Arc::clone(&self.hits),
            })
        }

        fn bindings(&self) -> Vec<BindingDecl> {
            vec![BindingDecl::new("q", "quit", "Quit").with_id("quit")]
        }
    }

    let hits: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    let observed = Arc::clone(&hits);
    run_test(KeymapUpdateApp { hits }, |pilot| {
        let count = || *observed.lock().unwrap_or_else(|e| e.into_inner());
        pilot.pause()?;
        pilot.pause()?;
        let base = count();

        pilot.app_mut().set_keymap([("quit", "f1")]);
        pilot.pause()?;
        assert_eq!(
            count(),
            base + 1,
            "first set_keymap must broadcast BindingsChanged once"
        );

        // An IDENTICAL keymap still rebroadcasts (forced by refresh_bindings).
        pilot.app_mut().set_keymap([("quit", "f1")]);
        pilot.pause()?;
        assert_eq!(
            count(),
            base + 2,
            "an identical set_keymap must rebroadcast (Python publishes \
             bindings_updated_signal unconditionally)"
        );
        Ok(())
    })
    .expect("run_test");
}
