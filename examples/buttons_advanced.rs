use std::sync::{Arc, Mutex};

use rich_rs::{Segment, Segments};
use textual::demo_snapshot::{SnapshotArgs, snapshot_widget};
use textual::prelude::*;
use textual::style::{Color, parse_color_like};

struct ButtonsDemo {
    id: WidgetId,
    status: Arc<Mutex<String>>,
    child: Box<dyn Widget>,
}

impl ButtonsDemo {
    fn new(status: Arc<Mutex<String>>, child: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            status,
            child: Box::new(child),
        }
    }
}

impl Widget for ButtonsDemo {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> Segments {
        self.child.render_styled(console, options)
    }

    fn on_mount(&mut self) {
        self.child.on_mount();
    }

    fn on_unmount(&mut self) {
        self.child.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.child.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.child.on_resize(width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.child.on_layout(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Message::ButtonPressed { description } = &message.message {
            *self.status.lock().unwrap_or_else(|e| e.into_inner()) = description.clone();
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(self.child.as_mut());
    }
}

struct StatusLine {
    id: WidgetId,
    text: Arc<Mutex<String>>,
}

impl StatusLine {
    fn new(text: Arc<Mutex<String>>) -> Self {
        Self {
            id: WidgetId::new(),
            text,
        }
    }
}

impl Widget for StatusLine {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, _console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let text = self.text.lock().unwrap_or_else(|e| e.into_inner());
        let line = rich_rs::set_cell_size(&format!("Events: {text}"), width);
        let mut out = Segments::new();
        out.push(Segment::new(line));
        out
    }
}

fn build_buttons_widget() -> AppRoot {
    let status = Arc::new(Mutex::new(String::from("")));
    let status_clone = status.clone();
    let status_for_demo = status.clone();

    let buttons = Horizontal::new()
        .with_child(
            VerticalScroll::new()
                .with_child(Node::new(Static::new("Standard Buttons")).class("header"))
                .with_child(Button::new("Default"))
                .with_child(Button::primary("Primary!"))
                .with_child(Button::success("Success!"))
                .with_child(Button::warning("Warning!"))
                .with_child(Button::error("Error!")),
        )
        .with_child(
            VerticalScroll::new()
                .with_child(Node::new(Static::new("Disabled Buttons")).class("header"))
                .with_child(Button::new("Default").disabled(true))
                .with_child(Button::primary("Primary!").disabled(true))
                .with_child(Button::success("Success!").disabled(true))
                .with_child(Button::warning("Warning!").disabled(true))
                .with_child(Button::error("Error!").disabled(true)),
        )
        .with_child(
            VerticalScroll::new()
                .with_child(Node::new(Static::new("Flat Buttons")).class("header"))
                .with_child(Button::new("Default").flat(true))
                .with_child(Button::primary("Primary!").flat(true))
                .with_child(Button::success("Success!").flat(true))
                .with_child(Button::warning("Warning!").flat(true))
                .with_child(Button::error("Error!").flat(true)),
        )
        .with_child(
            VerticalScroll::new()
                .with_child(Node::new(Static::new("Disabled Flat Buttons")).class("header"))
                .with_child(Button::new("Default").disabled(true).flat(true))
                .with_child(Button::primary("Primary!").disabled(true).flat(true))
                .with_child(Button::success("Success!").disabled(true).flat(true))
                .with_child(Button::warning("Warning!").disabled(true).flat(true))
                .with_child(Button::error("Error!").disabled(true).flat(true)),
        );

    let status_bg = parse_color_like("$panel").or_else(|| parse_color_like("$surface"));
    let status = Styled::new(
        StatusLine::new(status_clone),
        Style::new()
            .line_pad(1)
            .bg(status_bg.unwrap_or(Color::parse("#303a43").unwrap()))
            .border_top(Color::parse("#44cc44").unwrap())
            .border_right(Color::parse("#44cc44").unwrap())
            .border_bottom(Color::parse("#44cc44").unwrap())
            .border_left(Color::parse("#44cc44").unwrap()),
    );
    let scroll = ScrollView::new(buttons).scroll_step(2);
    let layout = Dock::new().push_fill(scroll).push_bottom(Some(3), status);
    AppRoot::new().with_child(ButtonsDemo::new(status_for_demo, layout))
}

#[tokio::main]
async fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }

    if let Some(args) = SnapshotArgs::parse() {
        let widget = build_buttons_widget();
        return snapshot_widget(
            &widget,
            &args,
            Some(std::path::Path::new("examples/button.tcss")),
        );
    }

    let mut app = App::new()?;
    if std::path::Path::new("examples/button.tcss").exists() {
        app.watch_stylesheet(
            "examples/button.tcss",
            std::time::Duration::from_millis(500),
        )?;
    }

    let mut scroll_root = build_buttons_widget();
    app.run_widget_tree(&mut scroll_root).await
}
