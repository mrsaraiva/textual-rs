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

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("l", "show_tab('leto')", "Leto"),
            BindingDecl::new("j", "show_tab('jessica')", "Jessica"),
            BindingDecl::new("p", "show_tab('paul')", "Paul"),
        ]
    }
}

fn main() -> Result<()> {
    run_sync(TabbedContentApp)
}
