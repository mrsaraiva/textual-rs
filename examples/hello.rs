use textual::prelude::*;

struct TickLabel {
    id: WidgetId,
    tick: u64,
}

impl TickLabel {
    fn new() -> Self {
        Self {
            id: WidgetId::new(),
            tick: 0,
        }
    }
}

impl Widget for TickLabel {
    fn id(&self) -> WidgetId {
        self.id
    }

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
    id: WidgetId,
    mounted: bool,
}

impl MountedLabel {
    fn new() -> Self {
        Self {
            id: WidgetId::new(),
            mounted: false,
        }
    }
}

impl Widget for MountedLabel {
    fn id(&self) -> WidgetId {
        self.id
    }

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

struct SizeLabel {
    id: WidgetId,
}

impl SizeLabel {
    fn new() -> Self {
        Self {
            id: WidgetId::new(),
        }
    }
}

impl Widget for SizeLabel {
    fn id(&self) -> WidgetId {
        self.id
    }

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
            Panel::new(
                Container::new()
                    .with_child(
                        Constrained::new(ListView::new(vec![
                            "item one".to_string(),
                            "item two".to_string(),
                            "item three".to_string(),
                        ]))
                        .min_height(3)
                        .max_height(3),
                    )
                    .with_child(Spacer::new(1))
                    .with_child(Frame::new(Button::new("Toggle me with Enter/Space")).padding(1)),
            )
            .title("Controls")
            .padding(1),
        );

        AppRoot::new().with_child(
            ScrollView::new(
                AppRoot::new()
                    .with_child(Label::new("textual-rs demo (widget tree + layout)"))
                    .with_child(SizeLabel::new())
                    .with_child(
                        Row::new()
                            .with_child(Label::new("row: left"))
                            .with_child(Label::new("row: right")),
                    )
                    .with_child(
                        Dock::new()
                            .height(5)
                            .push_top(Some(1), Label::new("dock: top"))
                            .push_bottom(Some(1), Label::new("dock: bottom"))
                            .push_left(6, Label::new("dock L"))
                            .push_right(6, Label::new("dock R"))
                            .push_fill(Label::new("dock center")),
                    )
                    .with_child(ScrollView::new(grid).height(4))
                    .with_child(Node::new(controls).class("panel").class("controls"))
                    .with_child(DataTable::new(
                        vec!["Name".into(), "Value".into()],
                        vec![
                            vec!["Alpha".into(), "1".into()],
                            vec!["Beta".into(), "2".into()],
                            vec!["Gamma".into(), "3".into()],
                        ],
                    ))
                    .with_child(Tree::new(vec![
                        TreeNode::new("Root")
                            .with_child(TreeNode::new("Child A"))
                            .with_child(
                                TreeNode::new("Child B")
                                    .expanded(false)
                                    .with_child(TreeNode::new("Leaf")),
                            ),
                        TreeNode::new("Other"),
                    ]))
                    .with_child(
                        Tabs::new()
                            .with_tab("One", Label::new("first tab"))
                            .with_tab("Two", Label::new("second tab")),
                    )
                    .with_child(Markdown::new("# Demo\n\n- Alpha\n- Beta\n\n`inline`"))
                    .with_child(Overlay::new(
                        Label::new("overlay base"),
                        Frame::new(Label::new("overlay modal")).padding(1),
                    ))
                    .with_child(Input::new().with_placeholder("type here..."))
                    .with_child(Checkbox::new("accept terms"))
                    .with_child(TickLabel::new())
                    .with_child(MountedLabel::new())
                    .with_child(Spacer::new(1))
                    .with_child(Label::new("press ctrl+q to quit")),
            )
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
