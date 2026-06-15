/// Port of Python Textual `docs/examples/widgets/footer.py`.
///
/// Demonstrates the `Footer` widget:
/// - Four bindings declared (one hidden)
/// - Footer renders key hints for visible bindings
/// - `q` quits, `?` shows help, `delete` deletes, `j` is hidden
use textual::prelude::*;

struct FooterApp;

impl TextualApp for FooterApp {
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("q", "quit", "Quit the app"),
            BindingDecl::new("question_mark", "help", "Show help screen"),
            BindingDecl::new("delete", "delete", "Delete the thing"),
            BindingDecl::new("j", "down", "Scroll down").hidden(),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Footer::new())
    }
}

fn main() -> textual::Result<()> {
    run_sync(FooterApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_app_composes_without_panic() {
        let mut app = FooterApp;
        let _root = app.compose();
    }

    #[test]
    fn bindings_declare_four_entries() {
        let app = FooterApp;
        let bindings = app.bindings();
        assert_eq!(bindings.len(), 4);
    }

    #[test]
    fn j_binding_is_hidden() {
        let app = FooterApp;
        let bindings = app.bindings();
        let j = bindings.iter().find(|b| b.key == "j").expect("j binding");
        assert!(!j.show, "j binding should be hidden");
    }

    #[test]
    fn visible_bindings_count_is_three() {
        let app = FooterApp;
        let bindings = app.bindings();
        let visible: Vec<_> = bindings.iter().filter(|b| b.show).collect();
        assert_eq!(visible.len(), 3);
    }
}
