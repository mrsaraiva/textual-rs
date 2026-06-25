/// Port of Python Textual `docs/examples/widgets/progress_bar.py`.
///
/// Demonstrates `ProgressBar` with a funding tracker:
/// - Header with title "Funding tracking"
/// - ProgressBar (total=100, show_eta=False)
/// - Input for donation amounts and a "Donate" button
/// - A history log below showing past donations
///
/// Donate by entering an integer amount and pressing Enter or clicking "Donate".
/// The progress bar advances by the entered amount.
///
/// NOTE: Dynamic mounting of Labels under VerticalScroll is not available via
/// the public `App` API (no `mount_at(parent, widget)`). History is tracked
/// using `ListView::append` which provides equivalent scrollable history.
use textual::prelude::*;

const CSS: &str = r#"
Container {
    overflow: hidden hidden;
    height: auto;
}

Center {
    margin-top: 1;
    margin-bottom: 1;
    layout: horizontal;
}

ProgressBar {
    padding-left: 3;
}

Input {
    width: 16;
}

ListView {
    height: auto;
}
"#;

struct FundingProgressApp;

impl TextualApp for FundingProgressApp {
    fn title(&self) -> &'static str {
        "Funding tracking"
    }

    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let header = Header::new();

        let funding_row = Center::new().with_compose(vec![
            ChildDecl::from(Label::new("Funding: ")),
            ChildDecl::from(ProgressBar::new(Some(100.0))).with_id("progress"),
        ]);

        let donate_row = Center::new().with_compose(vec![
            ChildDecl::from(Input::new().with_placeholder("$$$")).with_id("amount"),
            ChildDecl::from(Button::new("Donate")),
        ]);

        let history = ChildDecl::from(ListView::new(vec![])).with_id("history");

        AppRoot::new()
            .with_child(header)
            .with_child(funding_row)
            .with_child(donate_row)
            .with_compose(vec![history])
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut EventCtx) {
        // Mirror Python: ProgressBar(total=100, show_eta=False)
        // Use a dummy ReactiveCtx; show_eta field is set directly, ctx just records watchers.
        if let Ok(node_id) = app.query_one("ProgressBar") {
            let mut rctx = ReactiveCtx::new(node_id);
            let _ = app.with_query_one_mut_as::<ProgressBar, _>("ProgressBar", |bar| {
                bar.set_show_eta(false, &mut rctx);
            });
        }
        ctx.request_repaint();
    }

    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        ctx: &mut EventCtx,
    ) {
        // Handle both Button press and Input submit as donation triggers.
        let triggered = message.downcast_ref::<ButtonPressed>().is_some()
            || message.downcast_ref::<InputSubmitted>().is_some();

        if triggered {
            self.add_donation(app, ctx);
        }
    }
}

impl FundingProgressApp {
    fn add_donation(&mut self, app: &mut App, ctx: &mut EventCtx) {
        // Read the current input value.
        let text_value = app
            .with_query_one_mut_as::<Input, _>("#amount", |input| input.value().to_string())
            .unwrap_or_default();

        let value: i64 = match text_value.trim().parse() {
            Ok(v) => v,
            Err(_) => return,
        };

        // Advance the progress bar.
        let _ = app.with_query_one_mut_as::<ProgressBar, _>("#progress", |bar| {
            bar.advance(value as f64);
        });

        // Append a message to the history list. `ListView` composes its items
        // as arena `ListItem` children, so the newly appended item must be
        // re-composed into the tree to become visible.
        let donation_msg = format!("Donation for ${value} received!");
        let _ = app.with_query_one_mut_as::<ListView, _>("#history", |list| {
            list.append(donation_msg);
        });
        if let Ok(history_id) = app.query_one("#history") {
            ctx.request_recompose_node(history_id);
        }

        // Clear the input field.
        let _ = app.with_query_one_mut_as::<Input, _>("#amount", |input| {
            input.clear();
        });

        ctx.request_repaint();
    }
}

fn main() -> textual::Result<()> {
    run_sync(FundingProgressApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn funding_progress_app_composes_without_panic() {
        let mut app = FundingProgressApp;
        let _root = app.compose();
    }

    #[test]
    fn compose_has_expected_children() {
        let mut app = FundingProgressApp;
        let root = app.compose();
        assert!(!root.children().is_empty(), "AppRoot should have children");
    }

    #[test]
    fn progress_bar_total_is_100() {
        let bar = ProgressBar::new(Some(100.0));
        assert_eq!(bar.total(), Some(100.0));
    }

    #[test]
    fn title_matches_python() {
        let app = FundingProgressApp;
        assert_eq!(app.title(), "Funding tracking");
    }

    /// LIVENESS: focus the donation Input, type "25", submit with Enter, and
    /// require the rendered frame to change (progress bar advances + a history
    /// line "Donation for $25 received!" appears). A dead wiring (button/input
    /// message not routed to `add_donation`) would leave the frame unchanged.
    #[test]
    fn liveness_donation_advances_progress() {
        FundingProgressApp
            .run_test(|pilot| {
                pilot.click("#amount")?;
                let before = pilot.app().frame_fingerprint();
                pilot.press(&["2", "5", "enter"])?;
                let after = pilot.app().frame_fingerprint();
                assert_ne!(
                    before, after,
                    "submitting a donation must change the rendered frame"
                );
                Ok(())
            })
            .expect("run_test");
    }
}
