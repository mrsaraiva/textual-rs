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
        let text = format!("tick: {}", self.tick);
        Label::new(text).render(console, options)
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
        let text = format!("mounted: {}", self.mounted);
        Label::new(text).render(console, options)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    let mut app = App::new()?;
    app.enable_debug_layout(true);
    let size = app.driver().size();
    let row = Row::new()
        .with_child(Label::new("row: left"))
        .with_child(Label::new("row: right"));
    let dock = Dock::new()
        .height(5)
        .push_top(Some(1), Label::new("dock: top"))
        .push_bottom(Some(1), Label::new("dock: bottom"))
        .push_left(6, Label::new("dock L"))
        .push_right(6, Label::new("dock R"))
        .push_fill(Label::new("dock center"));
    let mut grid = Grid::new(2, 2);
    grid.set(0, 0, Label::new("g(0,0)"));
    grid.set(0, 1, Label::new("g(0,1)"));
    grid.set(1, 0, Label::new("g(1,0)"));
    grid.set(1, 1, Label::new("g(1,1)"));
    let scroller = ScrollView::new(grid).height(4);
    let mut root = AppRoot::new()
        .with_child(Label::new("textual-rs demo (widget tree + layout)"))
        .with_child(Label::new(format!("size: {}x{}", size.width, size.height)))
        .with_child(row)
        .with_child(dock)
        .with_child(scroller)
        .with_child(Input::new().with_placeholder("type here..."))
        .with_child(Checkbox::new("accept terms"))
        .with_child(TickLabel::new())
        .with_child(MountedLabel::new())
        .with_child(Frame::new(Button::new("Toggle me with Enter/Space")).padding(1))
        .with_child(Label::new("press q to quit"));
    app.run_widget_tree(&mut root).await
}
