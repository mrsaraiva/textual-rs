//! Generates the SVG screenshots embedded in `README.md` (under `imgs/`).
//!
//! Each screen is mounted and rendered through the real headless runtime
//! (`run_test_sized`), then the rendered frame is exported with
//! `App::save_frame_svg` — the same pipeline a live terminal sees.
//!
//! Run from the repository root:
//!
//! ```sh
//! cargo run --example readme_screens
//! ```
use textual::compose;
use textual::prelude::*;

const HERO_CSS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/readme_screens/hero.tcss"
);

/// The composed hero screen: widget variety in one small dashboard.
fn hero_screen() -> AppRoot {
    let mut progress = ProgressBar::new(Some(100.0));
    progress.advance(65.0);

    let table = DataTable::new(
        vec!["Widget".into(), "Kind".into(), "Since".into()],
        vec![
            vec!["Button".into(), "Interactive".into(), "0.1".into()],
            vec!["DataTable".into(), "Interactive".into(), "0.4".into()],
            vec!["Sparkline".into(), "Display".into(), "0.9".into()],
        ],
    );

    AppRoot::new().with_child(
        Container::new()
            .id("hero")
            .with_border_title("textual-rs")
            .with_compose(compose![
                Label::new("[b]Rich terminal UIs[/b] — widgets, CSS, reactivity").id("tagline"),
                Horizontal::new().id("actions").with_compose(compose![
                    Button::primary("Primary"),
                    Button::success("Success"),
                    Button::warning("Warning"),
                    Button::error("Error"),
                ]),
                Horizontal::new().id("controls").with_compose(compose![
                    Checkbox::new("Autosave"),
                    Switch::new(true),
                    progress,
                ]),
                table,
            ]),
    )
}

/// The exact widget tree from the README "A complete app" example.
fn question_screen() -> AppRoot {
    AppRoot::new()
        .with_child(Label::new("Do you love Textual?"))
        .with_child(Button::primary("Yes").id("yes"))
        .with_child(Button::error("No").id("no"))
}

/// A snapshot definition: theme + widget tree + optional stylesheet.
struct ShotApp {
    theme: &'static str,
    css: Option<&'static str>,
    build: fn() -> AppRoot,
    /// Optional real interaction (clicks/keys) applied before the screenshot.
    interact: Option<fn(&mut Pilot) -> Result<()>>,
}

impl TextualApp for ShotApp {
    fn compose(&mut self) -> AppRoot {
        (self.build)()
    }

    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.set_theme_by_name(self.theme);
        if let Some(css) = self.css {
            app.load_stylesheet_file(css)?;
        }
        Ok(())
    }
}

fn snap(shot: ShotApp, file: &str, width: u16, height: u16, title: &str) -> Result<()> {
    // Activate the theme globally before the App parses any stylesheet, so
    // every token resolves against the target theme from the start.
    textual::theme::set_active_theme(shot.theme);
    let path = format!("imgs/{file}");
    let title = title.to_string();
    let interact = shot.interact;
    run_test_sized(shot, width, height, move |pilot| {
        if let Some(interact) = interact {
            interact(pilot)?;
        }
        pilot.app().save_frame_svg(&path, &title)?;
        println!("wrote {path}");
        Ok(())
    })
}

fn main() -> Result<()> {
    std::fs::create_dir_all("imgs")?;

    let hero = |theme| ShotApp {
        theme,
        css: Some(HERO_CSS),
        build: hero_screen,
        // Genuinely check the checkbox through the runtime (real clicks),
        // then park focus + pointer on the primary button.
        interact: Some(|pilot| {
            pilot.click("Checkbox")?;
            pilot.click("Button")
        }),
    };

    // Hero (default theme).
    snap(hero("textual-dark"), "hero.svg", 90, 24, "textual-rs")?;

    // The "complete app" example, exactly as shown in the README.
    let question = ShotApp {
        theme: "textual-dark",
        css: None,
        build: question_screen,
        interact: None,
    };
    snap(question, "question.svg", 70, 10, "QuestionApp")?;

    // Themes montage: the same hero screen under other built-in themes.
    for theme in ["nord", "gruvbox", "dracula", "solarized-light"] {
        let file = format!("theme_{}.svg", theme.replace('-', "_"));
        snap(hero(theme), &file, 90, 24, theme)?;
    }
    Ok(())
}
