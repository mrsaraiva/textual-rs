use textual::prelude::*;

const LETO: &str = r#"
# Duke Leto I Atreides

Head of House Atreides.
"#;

const JESSICA: &str = r#"
# Lady Jessica

Bene Gesserit and concubine of Leto, and mother of Paul and Alia.
"#;

const PAUL: &str = r#"
# Paul Atreides

Son of Leto and Jessica.
"#;

struct TabbedContentApp;

impl TextualApp for TabbedContentApp {
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("l", "show_tab('leto')", "Leto"),
            BindingDecl::new("j", "show_tab('jessica')", "Jessica"),
            BindingDecl::new("p", "show_tab('paul')", "Paul"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        let nested = TabbedContent::new()
            .with_pane(TabPane::new("Paul", Label::new("First child")))
            .with_pane(TabPane::new("Alia", Label::new("Second child")));

        let tabs = TabbedContent::new()
            .initial("jessica")
            .with_pane(TabPane::new("Leto", Markdown::new(LETO)).id("leto"))
            .with_pane(
                TabPane::new(
                    "Jessica",
                    Container::new()
                        .with_child(Markdown::new(JESSICA))
                        .with_child(nested),
                )
                .id("jessica"),
            )
            .with_pane(TabPane::new("Paul", Markdown::new(PAUL)).id("paul"));

        AppRoot::new().with_child(Footer::new()).with_child(tabs)
    }
}

fn main() -> Result<()> {
    run_sync(TabbedContentApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabbed_content_app_composes_without_panic() {
        let mut app = TabbedContentApp;
        let _root = app.compose();
    }

    /// LIVENESS: the content starts on the "jessica" tab (`initial("jessica")`).
    /// Pressing `p` runs `show_tab('paul')`, switching the active pane to Paul's
    /// markdown — the displayed body changes, so the frame must change. A dead
    /// `show_tab` binding leaves the Jessica pane showing / frame identical.
    #[test]
    fn liveness_switch_tab() {
        TabbedContentApp
            .run_test(|pilot| {
                let before = pilot.app().frame_fingerprint();
                pilot.press(&["p"])?; // show_tab('paul')
                let after = pilot.app().frame_fingerprint();
                assert_ne!(
                    before, after,
                    "switching to the Paul tab must change the rendered frame"
                );
                Ok(())
            })
            .expect("run_test");
    }
}
