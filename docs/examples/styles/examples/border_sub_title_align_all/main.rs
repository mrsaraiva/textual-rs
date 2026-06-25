/// Port of Python Textual `docs/examples/styles/border_sub_title_align_all.py`.
///
/// Demonstrates border-title-align and border-subtitle-align on a 3x3 grid of
/// labeled containers. Each label has a border_title and border_subtitle set,
/// using Rich markup (`[b red]`, `[reverse]`, `[u][r]…[/]`) which is styled
/// through the Content markup pipeline, and long titles are ellipsis-truncated
/// to the border width — matching Python `_border.render_border_label`.
use textual::prelude::*;

const CSS: &str = r##"
Grid {
    grid-size: 3 3;
    align: center middle;
}

Container {
    width: 100%;
    height: 100%;
    align: center middle;
}

#lbl1 {
    border: vkey $secondary;
}

#lbl2 {
    border: round $secondary;
    border-title-align: right;
    border-subtitle-align: right;
}

#lbl3 {
    border: wide $secondary;
    border-title-align: center;
    border-subtitle-align: center;
}

#lbl4 {
    border: ascii $success;
    border-title-align: center;
    border-subtitle-align: left;
}

#lbl5 {
    /* No border = no (sub)title. */
    border: none $success;
    border-title-align: center;
    border-subtitle-align: center;
}

#lbl6 {
    border-top: solid $success;
    border-bottom: solid $success;
}

#lbl7 {
    border-top: solid $error;
    border-bottom: solid $error;
    padding: 1 2;
    border-subtitle-align: left;
}

#lbl8 {
    border-top: solid $error;
    border-bottom: solid $error;
    border-title-align: center;
    border-subtitle-align: center;
}

#lbl9 {
    border-top: solid $error;
    border-bottom: solid $error;
    border-title-align: right;
}
"##;

struct BorderSubTitleAlignAll;

impl TextualApp for BorderSubTitleAlignAll {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Grid::new(3, 3)
                .with_child(
                    Container::new().with_child(
                        Label::new("This is the story of")
                            .id("lbl1")
                            .with_border_title("[b]Border [i]title[/i][/]")
                            .with_border_subtitle("[u][r]Border[/r] subtitle[/]"),
                    ),
                )
                .with_child(
                    Container::new().with_child(
                        Label::new("a Python")
                            .id("lbl2")
                            .with_border_title("[b red]Left, but it's loooooooooooong")
                            .with_border_subtitle("[reverse]Center, but it's loooooooooooong"),
                    ),
                )
                .with_child(
                    Container::new().with_child(
                        Label::new("developer that")
                            .id("lbl3")
                            .with_border_title("[b i on purple]Left[/]")
                            .with_border_subtitle("[r u white on black]@@@[/]"),
                    ),
                )
                .with_child(
                    Container::new().with_child(
                        Label::new("had to fill up")
                            .id("lbl4")
                            .with_border_subtitle("[link='https://textual.textualize.io']Left[/]"),
                    ),
                )
                .with_child(
                    Container::new().with_child(
                        Label::new("nine labels")
                            .id("lbl5")
                            .with_border_title("Title")
                            .with_border_subtitle("Subtitle"),
                    ),
                )
                .with_child(
                    Container::new().with_child(
                        Label::new("and ended up redoing it")
                            .id("lbl6")
                            .with_border_title("Title")
                            .with_border_subtitle("Subtitle"),
                    ),
                )
                .with_child(
                    Container::new().with_child(
                        Label::new("because the first try")
                            .id("lbl7")
                            .with_border_title("Title, but really loooooooooong!")
                            .with_border_subtitle("Subtitle, but really loooooooooong!"),
                    ),
                )
                .with_child(
                    Container::new().with_child(
                        Label::new("had some labels")
                            .id("lbl8")
                            .with_border_title("Title, but really loooooooooong!")
                            .with_border_subtitle("Subtitle, but really loooooooooong!"),
                    ),
                )
                .with_child(
                    Container::new().with_child(
                        Label::new("that were too long.")
                            .id("lbl9")
                            .with_border_title("Title, but really loooooooooong!")
                            .with_border_subtitle("Subtitle, but really loooooooooong!"),
                    ),
                ),
        )
    }
}

fn main() -> Result<()> {
    run_sync(BorderSubTitleAlignAll)
}
