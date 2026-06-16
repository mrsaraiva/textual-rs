use rich_rs::Segments;
use textual::compose;
use textual::prelude::*;

const TEXT: &str = "I must not fear.\nFear is the mind-killer.\nFear is the little-death that brings total obliteration.\nI will face my fear.\nI will permit it to pass over me and through me.\nAnd when it has gone past, I will turn the inner eye to see its path.\nWhere the fear has gone there will be nothing. Only I will remain.";

struct QuitDialogRoot;

impl Widget for QuitDialogRoot {
    fn style_type(&self) -> &'static str {
        "QuitScreen"
    }

    fn compose(&self) -> ComposeResult {
        compose![
            Grid::new(2, 2)
                .id("dialog")
                .with_child(Label::new("Are you sure you want to quit?").with_id("question"))
                .with_child(Button::error("Quit").with_action("app.quit"))
                .with_child(Button::primary("Cancel").with_action("app.pop_screen"))
        ]
    }

    fn render(&self, _console: &rich_rs::Console, _options: &rich_rs::ConsoleOptions) -> Segments {
        Segments::new()
    }
}

struct QuitScreen;

impl Screen for QuitScreen {
    fn name(&self) -> &str {
        "QuitScreen"
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(QuitDialogRoot)
    }

    fn css(&self) -> Option<&str> {
        Some(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/shared/modal01.tcss"
        ))
    }

    fn is_modal(&self) -> bool {
        false
    }
}

struct ModalApp;

impl TextualApp for ModalApp {
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("q", "app.push_screen('quit')", "Quit")]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Header::new())
            .with_child(Label::new(TEXT.repeat(8)))
            .with_child(Footer::new())
    }

    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.add_mode("quit", || Box::new(QuitScreen));
        Ok(())
    }
}

fn main() -> Result<()> {
    run_sync(ModalApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modal01_screen_is_non_modal() {
        let screen = QuitScreen;
        assert!(!screen.is_modal());
    }

    #[test]
    fn modal01_registers_quit_mode_and_pushes_screen() {
        let mut definition = ModalApp;
        let mut app = App::new().expect("app should initialize");
        definition
            .configure(&mut app)
            .expect("modal01 configure should succeed");

        assert_eq!(app.screen_count(), 0);
        assert!(app.action_push_screen("quit"));
        assert_eq!(app.screen_count(), 1);
    }
}
