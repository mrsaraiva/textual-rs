//! Profile-setup form — a COLD-USER app pressure-testing the textual-rs 1.0
//! INPUT surface: `Input` + the validation framework, `Select`, `Switch`,
//! `RadioSet`/`RadioButton`, `Checkbox`, and a submit `Button`. Authored against
//! only `std` + `textual::prelude::*` (+ `rich-rs`, forced by `Widget::render`).
//!
//! What it exercises that Kanban/Pomodoro did not:
//! * live `Input` validation (`with_validators` → the `-valid`/`-invalid` border
//!   feedback a user sees while typing),
//! * the post-RA2.5 arena widgets `Select` (dropdown on the overlay layer) and
//!   `RadioSet` (composed-children arena widget),
//! * gathering heterogeneous control state on submit via the query surface.
//!
//! Design note (friction sidestep): only `Input` and `Select` ship a native
//! `.id(...)` builder; `Checkbox`/`Switch`/`RadioSet` would need
//! `ChildDecl::with_id` (whose mount-time harvest is unreliable — see the
//! Pomodoro/Kanban friction reports). Because the form has exactly ONE of each
//! of those, we query them by TYPE selector (`"Checkbox"`, `"Switch"`,
//! `"RadioSet"`) and only the two `Input`s (which collide by type) carry ids.
//! The single residual runtime-leak a cold user still types is `ReactiveCtx` —
//! only to READ nothing; all reads here are through `with_query_one_mut_as`.

use std::sync::Arc;

use textual::prelude::*;

const THEMES: [&str; 3] = ["Dark", "Light", "System"];

/// Username rule: 3+ characters, no whitespace. Used both as the live `Input`
/// validator AND as the submit-time check (one function, two call sites).
fn username_ok(value: &str) -> bool {
    let v = value.trim();
    v.chars().count() >= 3 && !v.chars().any(char::is_whitespace)
}

fn parse_age(value: &str) -> Option<i64> {
    value.trim().parse::<i64>().ok().filter(|n| (18..=120).contains(n))
}

const CSS: &str = r#"
Screen { align: center middle; }

#card {
    width: 60;
    height: auto;
    border: round $primary;
    padding: 1 2;
    background: $surface;
}

#title { text-style: bold; color: $accent; width: 100%; text-align: center; }
.field-label { margin: 1 0 0 0; color: $text-muted; }

Input { margin: 0; }
Input.-valid { border: tall $success 60%; }
Input.-valid:focus { border: tall $success; }
Input.-invalid { border: tall $error 60%; }
Input.-invalid:focus { border: tall $error; }

RadioSet { width: 100%; height: auto; }
Select { width: 100%; }

#submit-row { height: auto; margin: 1 0 0 0; align: center middle; }

#status { margin: 1 0 0 0; width: 100%; text-align: center; }
#status.ok { color: $success; text-style: bold; }
#status.err { color: $error; text-style: bold; }
"#;

struct FormApp {
    /// Count of successful submissions (test signal).
    submitted: u32,
    /// Last status line pushed to the `#status` label (test signal).
    last_status: String,
}

impl FormApp {
    fn new() -> Self {
        Self { submitted: 0, last_status: String::new() }
    }

    /// Gather every control's state, validate, and return either the success
    /// summary or the first error. Reads are by native id (`Input`) or by type
    /// selector (single-instance controls).
    fn evaluate(&self, app: &mut App) -> std::result::Result<String, String> {
        let username = app
            .with_query_one_mut_as::<Input, _>("#username", |i| i.value().to_string())
            .unwrap_or_default();
        let age_raw = app
            .with_query_one_mut_as::<Input, _>("#age", |i| i.value().to_string())
            .unwrap_or_default();
        let plan = app
            .with_query_one_mut_as::<Select<String>, _>("Select", |s| s.value().cloned())
            .ok()
            .flatten()
            .unwrap_or_else(|| "free".to_string());
        let notify = app
            .with_query_one_mut_as::<Switch, _>("Switch", |s| s.value())
            .unwrap_or(false);
        let theme_idx = app
            .with_query_one_mut_as::<RadioSet, _>("RadioSet", |rs| rs.pressed_index())
            .ok()
            .flatten()
            .unwrap_or(0);
        let terms = app
            .with_query_one_mut_as::<Checkbox, _>("Checkbox", |c| c.checked())
            .unwrap_or(false);

        if !username_ok(&username) {
            return Err("Username must be 3+ characters with no spaces.".to_string());
        }
        let Some(age) = parse_age(&age_raw) else {
            return Err("Age must be a whole number between 18 and 120.".to_string());
        };
        if !terms {
            return Err("You must accept the terms to continue.".to_string());
        }
        let theme = THEMES.get(theme_idx).copied().unwrap_or("Dark");
        Ok(format!(
            "Welcome, {username} ({age}) · {plan} plan · {theme} theme · notifications {}",
            if notify { "on" } else { "off" }
        ))
    }

    fn push_status(&mut self, app: &mut App, text: &str, ok: bool) {
        self.last_status = text.to_string();
        let t = text.to_string();
        let _ = app.with_query_one_mut_as::<Label, _>("#status", |l| l.set_text(t));
        // Recolor the sole status line green (ok) or red (err) via the query
        // surface's chainable class builder.
        let _ = app
            .query_mut("#status")
            .map(|q| q.set_class(ok, &["ok"]).set_class(!ok, &["err"]));
    }
}

impl TextualApp for FormApp {
    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let username = Input::new()
            .with_placeholder("3+ chars, no spaces")
            .id("username")
            .with_validators(vec![Arc::new(Function::new(
                username_ok,
                "Username must be 3+ chars, no spaces.",
            )) as ValidatorRef]);

        let age = Input::new()
            .with_placeholder("18 to 120")
            .id("age")
            .with_validators(vec![
                Arc::new(Number::new().minimum(18.0).maximum(120.0)) as ValidatorRef,
            ]);

        let plan = Select::new(
            vec![
                ("Free".to_string(), "free".to_string()),
                ("Pro".to_string(), "pro".to_string()),
                ("Team".to_string(), "team".to_string()),
            ],
            "Choose a plan",
        )
        .with_allow_blank(false);

        let theme = RadioSet::new()
            .with_button(RadioButton::new("Dark").with_value(true))
            .with_button(RadioButton::new("Light"))
            .with_button(RadioButton::new("System"));

        let card = Container::new().with_compose(vec![
            ChildDecl::new(Box::new(Label::new("Create your profile"))).with_id("title"),
            ChildDecl::new(Box::new(Label::new("Username"))).with_classes(&["field-label"]),
            ChildDecl::from(username),
            ChildDecl::new(Box::new(Label::new("Age"))).with_classes(&["field-label"]),
            ChildDecl::from(age),
            ChildDecl::new(Box::new(Label::new("Plan"))).with_classes(&["field-label"]),
            ChildDecl::from(plan),
            ChildDecl::new(Box::new(Label::new("Theme"))).with_classes(&["field-label"]),
            ChildDecl::from(theme),
            ChildDecl::from(
                HorizontalGroup::new()
                    .with_child(Checkbox::new("I accept the terms"))
                    .with_child(Switch::new(true)),
            ),
            ChildDecl::from(
                HorizontalGroup::new().with_child(Button::success("Submit").id("submit")),
            )
            .with_id("submit-row"),
            ChildDecl::new(Box::new(Label::new("Fill the form and submit."))).with_id("status"),
        ]);

        AppRoot::new().with_compose(vec![ChildDecl::new(Box::new(card)).with_id("card")])
    }

    /// Submit is handled through the message bus (`ButtonPressed` bubbles to the
    /// app), which is the hook that hands us `&mut App` for the query surface.
    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut WidgetCtx) {
        let Some(ev) = message.downcast_ref::<ButtonPressed>() else {
            return;
        };
        // Match the button by its id, not its `description` (which is a widget
        // repr, not the label — a genuine cold-user footgun).
        if ev.button_id.as_deref() != Some("submit") {
            return;
        }
        match self.evaluate(app) {
            Ok(summary) => {
                self.submitted += 1;
                self.push_status(app, &summary, true);
            }
            Err(err) => self.push_status(app, &err, false),
        }
        ctx.request_repaint();
    }
}

fn main() -> Result<()> {
    run_sync(FormApp::new())
}

// ===========================================================================
// Tests — deterministic, via Pilot
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn submitted(pilot: &mut Pilot) -> u32 {
        pilot
            .app_mut()
            .with_app_struct::<FormApp, _>(
                |a, _app, _ctx| a.submitted,
                &mut textual::event::EventCtx::default(),
            )
            .unwrap_or(0)
    }

    fn status(pilot: &mut Pilot) -> String {
        pilot
            .app_mut()
            .with_app_struct::<FormApp, _>(
                |a, _app, _ctx| a.last_status.clone(),
                &mut textual::event::EventCtx::default(),
            )
            .unwrap_or_default()
    }

    fn type_into(pilot: &mut Pilot, id: &str, text: &str) -> textual::Result<()> {
        let _ = pilot.app_mut().query_mut(id).map(|q| q.focus());
        let keys: Vec<&str> = text.split("").filter(|s| !s.is_empty()).collect();
        pilot.press(&keys)?;
        Ok(())
    }

    #[test]
    fn app_composes_without_panic() {
        let mut app = FormApp::new();
        let _root = app.compose();
    }

    #[test]
    fn username_and_age_rules() {
        assert!(!username_ok("ab"));
        assert!(!username_ok("a b"));
        assert!(username_ok("alice"));
        assert_eq!(parse_age("30"), Some(30));
        assert_eq!(parse_age("17"), None);
        assert_eq!(parse_age("abc"), None);
    }

    /// Submitting an empty form fails validation (username rule) and does NOT
    /// count as a submission; the status line carries the error.
    #[test]
    fn empty_submit_is_rejected() {
        run_test(FormApp::new(), |pilot| {
            pilot.resize(80, 44)?;
            pilot.click("#submit")?;
            assert_eq!(submitted(pilot), 0, "an invalid form must not submit");
            assert!(
                status(pilot).contains("Username"),
                "status must report the username failure, got: {}",
                status(pilot)
            );
            Ok(())
        })
        .unwrap();
    }

    /// Missing the terms checkbox blocks a submit even with valid name+age.
    #[test]
    fn unchecked_terms_blocks_submit() {
        run_test(FormApp::new(), |pilot| {
            pilot.resize(80, 44)?;
            type_into(pilot, "#username", "alice")?;
            type_into(pilot, "#age", "30")?;
            pilot.click("#submit")?;
            assert_eq!(submitted(pilot), 0);
            assert!(
                status(pilot).contains("terms"),
                "status must report the terms failure, got: {}",
                status(pilot)
            );
            Ok(())
        })
        .unwrap();
    }

    /// A fully valid form (name + age + terms) submits and produces a welcome
    /// summary carrying the entered values.
    #[test]
    fn valid_form_submits_with_summary() {
        run_test(FormApp::new(), |pilot| {
            pilot.resize(80, 44)?;
            type_into(pilot, "#username", "alice")?;
            type_into(pilot, "#age", "30")?;
            // Check the sole Checkbox (space toggles the focused widget).
            let _ = pilot.app_mut().query_mut("Checkbox").map(|q| q.focus());
            pilot.press(&["space"])?;
            pilot.click("#submit")?;
            assert_eq!(submitted(pilot), 1, "a valid form must submit exactly once");
            let s = status(pilot);
            assert!(s.contains("alice") && s.contains("30"), "summary must echo inputs, got: {s}");
            Ok(())
        })
        .unwrap();
    }

    /// The live `Input` validators drive the `-valid`/`-invalid` border classes
    /// as the user types — so typing a valid then invalid age changes the frame.
    #[test]
    fn input_validation_feedback_is_live() {
        run_test(FormApp::new(), |pilot| {
            pilot.resize(80, 44)?;
            let before = pilot.app().frame_fingerprint();
            type_into(pilot, "#age", "30")?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(before, after, "typing into a validated Input must re-render");
            Ok(())
        })
        .unwrap();
    }
}
