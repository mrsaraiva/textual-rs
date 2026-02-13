use textual::compose;
use textual::prelude::*;

struct TickLabel {
    tick: u64,
}

impl TickLabel {
    fn new() -> Self {
        Self { tick: 0 }
    }
}

impl Widget for TickLabel {
    fn on_tick(&mut self, tick: u64) {
        self.tick = tick;
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        Label::new(format!("tick: {}", self.tick)).render(console, options)
    }
}

struct MountedLabel {
    mounted: bool,
}

impl MountedLabel {
    fn new() -> Self {
        Self { mounted: false }
    }
}

impl Widget for MountedLabel {
    fn on_mount(&mut self) {
        self.mounted = true;
    }

    fn on_unmount(&mut self) {
        self.mounted = false;
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        Label::new(format!("mounted: {}", self.mounted)).render(console, options)
    }
}

struct SizeLabel;

impl SizeLabel {
    fn new() -> Self {
        Self
    }
}

impl Widget for SizeLabel {
    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        Label::new(format!("size: {}x{}", options.size.0, options.size.1)).render(console, options)
    }
}

struct HelloApp;

impl TextualApp for HelloApp {
    fn compose(&mut self) -> AppRoot {
        let mut grid = Grid::new(2, 2);
        grid.set(0, 0, Label::new("g(0,0)"));
        grid.set(0, 1, Label::new("g(0,1)"));
        grid.set(1, 0, Label::new("g(1,0)"));
        grid.set(1, 1, Label::new("g(1,1)"));

        let controls = Constrained::new(
            Panel::new(Container::new().with_compose(compose![
                    Constrained::new(ListView::new(vec![
                        "item one".to_string(),
                        "item two".to_string(),
                        "item three".to_string(),
                    ]))
                    .min_height(3)
                    .max_height(3),
                    Spacer::new(1),
                    Frame::new(Button::new("Toggle me with Enter/Space")).padding(1),
                ]))
            .title("Controls")
            .padding(1),
        );

        AppRoot::new().with_child(
            ScrollView::new(AppRoot::new().with_compose(compose![
                    Label::new("textual-rs demo (widget tree + layout)"),
                    SizeLabel::new(),
                    Row::new().with_compose(compose![
                        Label::new("row: left"),
                        Label::new("row: right"),
                    ]),
                    Dock::new()
                        .height(5)
                        .push_top(Some(1), Label::new("dock: top"))
                        .push_bottom(Some(1), Label::new("dock: bottom"))
                        .push_left(6, Label::new("dock L"))
                        .push_right(6, Label::new("dock R"))
                        .push_fill(Label::new("dock center")),
                    ScrollView::new(grid).height(4),
                    Node::new(controls).class("panel").class("controls"),
                    DataTable::new(
                        vec!["Name".into(), "Value".into()],
                        vec![
                            vec!["Alpha".into(), "1".into()],
                            vec!["Beta".into(), "2".into()],
                            vec!["Gamma".into(), "3".into()],
                        ],
                    ),
                    Tree::new(vec![
                        TreeNode::new("Root")
                            .with_child(TreeNode::new("Child A"))
                            .with_child(
                                TreeNode::new("Child B")
                                    .expanded(false)
                                    .with_child(TreeNode::new("Leaf")),
                            ),
                        TreeNode::new("Other"),
                    ]),
                    Tabs::new()
                        .with_tab("One", Label::new("first tab"))
                        .with_tab("Two", Label::new("second tab")),
                    Markdown::new("# Demo\n\n- Alpha\n- Beta\n\n`inline`"),
                    Overlay::new(
                        Label::new("overlay base"),
                        Frame::new(Label::new("overlay modal")).padding(1),
                    ),
                    Input::new().with_placeholder("type here..."),
                    Checkbox::new("accept terms"),
                    TickLabel::new(),
                    MountedLabel::new(),
                    Spacer::new(1),
                    Label::new("press ctrl+q to quit"),
                ]))
            .scroll_step(2)
            .scroll_step_x(4),
        )
    }

    fn css_path(&self) -> Option<&'static str> {
        Some("demo.css")
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(HelloApp)
}
