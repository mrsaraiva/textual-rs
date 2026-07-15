//! Generates the SVG screenshots embedded in `README.md` (under `imgs/`).
//!
//! Each screen is mounted and rendered through the real headless runtime
//! (`run_test_sized`), then the rendered frame is exported with
//! `App::save_frame_svg`, the same pipeline a live terminal sees.
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
                Label::new("[b]Rich terminal UIs[/b]: widgets, CSS, reactivity").id("tagline"),
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

const EDITOR_CODE: &str = r#"from dataclasses import dataclass, field


@dataclass
class Task:
    """A single item on the board."""

    title: str
    tags: list[str] = field(default_factory=list)
    done: bool = False

    def toggle(self) -> None:
        self.done = not self.done


def summary(tasks: list[Task]) -> str:
    remaining = sum(1 for task in tasks if not task.done)
    return f"{remaining} of {len(tasks)} tasks remaining"
"#;

/// A syntax-highlighted code editor (tree-sitter based highlighting).
fn editor_screen() -> AppRoot {
    AppRoot::new().with_child(
        TextArea::code_editor(EDITOR_CODE)
            .with_language("python")
            .with_cursor_blink(false),
    )
}

const GALLERY_MARKDOWN: &str = r#"## Markdown, rendered in the terminal

The `Markdown` widget renders headings, *emphasis*, **strong text**,
`inline code`, lists, tables, and fenced code blocks.

- Reactive state with watchers
- CSS styling with theme tokens
- A real layout engine

```python
def greet(name: str) -> str:
    return f"Hello, {name}!"
```
"#;

/// The Markdown widget rendering a small document.
fn markdown_screen() -> AppRoot {
    AppRoot::new().with_child(Markdown::new(GALLERY_MARKDOWN))
}

/// A data-heavy DataTable: header row plus a keyboard-driven cell cursor.
fn table_screen() -> AppRoot {
    let headers: Vec<String> = ["Mission", "Vehicle", "Crew", "Launched", "Outcome"]
        .into_iter()
        .map(String::from)
        .collect();
    let rows: Vec<Vec<String>> = [
        ["Apollo 8", "Saturn V", "3", "1968-12-21", "Success"],
        ["Apollo 11", "Saturn V", "3", "1969-07-16", "Success"],
        ["Apollo 13", "Saturn V", "3", "1970-04-11", "Aborted"],
        ["Skylab 2", "Saturn IB", "3", "1973-05-25", "Success"],
        ["STS-1", "Space Shuttle", "2", "1981-04-12", "Success"],
        ["STS-61", "Space Shuttle", "7", "1993-12-02", "Success"],
        ["Expedition 1", "Soyuz TM-31", "3", "2000-10-31", "Success"],
        ["Demo-2", "Falcon 9", "2", "2020-05-30", "Success"],
        ["Inspiration4", "Falcon 9", "4", "2021-09-15", "Success"],
        ["Artemis I", "SLS", "0", "2022-11-16", "Success"],
    ]
    .into_iter()
    .map(|row| row.into_iter().map(String::from).collect())
    .collect();
    AppRoot::new().with_child(DataTable::new(headers, rows))
}

/// A Tree showing hierarchical data with expanded and collapsed branches.
fn tree_screen() -> AppRoot {
    AppRoot::new().with_child(Tree::new(vec![TreeNode::new("textual-rs")
        .expanded(true)
        .allow_expand(true)
        .with_child(
            TreeNode::new("src")
                .expanded(true)
                .allow_expand(true)
                .with_child(
                    TreeNode::new("runtime")
                        .expanded(true)
                        .allow_expand(true)
                        .with_child(TreeNode::new("event_loop.rs"))
                        .with_child(TreeNode::new("render.rs"))
                        .with_child(TreeNode::new("routing.rs")),
                )
                .with_child(
                    TreeNode::new("widgets")
                        .expanded(true)
                        .allow_expand(true)
                        .with_child(TreeNode::new("button.rs"))
                        .with_child(TreeNode::new("data_table.rs"))
                        .with_child(TreeNode::new("text_area.rs")),
                )
                .with_child(TreeNode::new("css").allow_expand(true)),
        )
        .with_child(TreeNode::new("examples").allow_expand(true))
        .with_child(TreeNode::new("Cargo.toml"))]))
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

    // Gallery: a syntax-highlighted code editor (focused, cursor parked in the code).
    let editor = ShotApp {
        theme: "textual-dark",
        css: None,
        build: editor_screen,
        interact: Some(|pilot| {
            pilot.press(&["tab"])?; // focus the editor
            pilot.press(&["down", "down", "down", "down", "end"])
        }),
    };
    snap(editor, "code_editor.svg", 90, 20, "TextArea")?;

    // Gallery: rendered Markdown.
    let markdown = ShotApp {
        theme: "textual-dark",
        css: None,
        build: markdown_screen,
        interact: None,
    };
    snap(markdown, "markdown.svg", 80, 17, "Markdown")?;

    // Gallery: a data-heavy table, focused, with the cell cursor moved into the data.
    let table = ShotApp {
        theme: "textual-dark",
        css: None,
        build: table_screen,
        interact: Some(|pilot| {
            pilot.press(&["tab"])?; // focus the table
            pilot.press(&["down", "down", "right"])
        }),
    };
    snap(table, "data_table.svg", 90, 12, "DataTable")?;

    // Gallery: a Tree with the cursor on a nested node.
    let tree = ShotApp {
        theme: "textual-dark",
        css: None,
        build: tree_screen,
        interact: Some(|pilot| {
            pilot.press(&["tab"])?; // focus the tree
            pilot.press(&["down", "down", "down"])
        }),
    };
    snap(tree, "tree.svg", 60, 14, "Tree")?;
    Ok(())
}
