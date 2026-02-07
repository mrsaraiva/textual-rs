use crossterm::event::{KeyCode, KeyModifiers};
use rich_rs::{Console, ConsoleOptions, Segment, Segments};
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

struct TabbedDemo {
    id: WidgetId,
    tabs: TabbedContent,
    footer: Footer,
}

impl TabbedDemo {
    fn new() -> Self {
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

        Self {
            id: WidgetId::new(),
            tabs,
            footer: Footer::new(),
        }
    }
}

impl Widget for TabbedDemo {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let footer_height = 1usize.min(height);
        let body_height = height.saturating_sub(footer_height).max(1);

        let mut body_options = options.clone();
        body_options.size = (width, body_height);
        body_options.max_width = width;
        body_options.max_height = body_height;
        let body_segments = self.tabs.render_styled(console, &body_options);
        let mut body_lines = Segment::split_and_crop_lines(body_segments, width, None, true, false);
        body_lines = Segment::set_shape(&body_lines, width, Some(body_height), None, false);

        let mut footer_options = options.clone();
        footer_options.size = (width, footer_height);
        footer_options.max_width = width;
        footer_options.max_height = footer_height;
        let footer_segments = self.footer.render_styled(console, &footer_options);
        let mut footer_lines =
            Segment::split_and_crop_lines(footer_segments, width, None, true, false);
        footer_lines = Segment::set_shape(&footer_lines, width, Some(footer_height), None, false);

        let mut lines = body_lines;
        lines.extend(footer_lines);

        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_mount(&mut self) {
        self.tabs.on_mount();
        self.footer.on_mount();
    }

    fn on_unmount(&mut self) {
        self.tabs.on_unmount();
        self.footer.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.tabs.on_tick(tick);
        self.footer.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.tabs.on_resize(width, height);
        self.footer.on_resize(width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        let footer_height = 1u16.min(height);
        let body_height = height.saturating_sub(footer_height).max(1);
        self.tabs.on_layout(width, body_height);
        self.footer.on_layout(width, footer_height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.tabs.on_event_capture(event, ctx);
        if !ctx.handled() {
            self.footer.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Char('l') => {
                    if self.tabs.set_active_id("leto") {
                        ctx.request_repaint();
                        ctx.set_handled();
                        return;
                    }
                }
                KeyCode::Char('j') => {
                    if self.tabs.set_active_id("jessica") {
                        ctx.request_repaint();
                        ctx.set_handled();
                        return;
                    }
                }
                KeyCode::Char('p') => {
                    if self.tabs.set_active_id("paul") {
                        ctx.request_repaint();
                        ctx.set_handled();
                        return;
                    }
                }
                _ => {}
            }
        }
        self.tabs.on_event(event, ctx);
        if !ctx.handled() {
            self.footer.on_event(event, ctx);
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        self.tabs.on_message(message, ctx);
        if !ctx.handled() {
            self.footer.on_message(message, ctx);
        }
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(&mut self.tabs);
        f(&mut self.footer);
    }

    fn focusable(&self) -> bool {
        self.tabs.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.tabs.set_focus(focused);
    }

    fn has_focus(&self) -> bool {
        self.tabs.has_focus()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        None
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        None
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut app = App::new()?;
    app.set_command_palette_hint(true);
    app.add_binding_hint(KeyBind::new(KeyCode::Char('l'), KeyModifiers::NONE), "Leto");
    app.add_binding_hint(
        KeyBind::new(KeyCode::Char('j'), KeyModifiers::NONE),
        "Jessica",
    );
    app.add_binding_hint(KeyBind::new(KeyCode::Char('p'), KeyModifiers::NONE), "Paul");
    let mut demo = AppRoot::new().with_child(CommandPalette::new(TabbedDemo::new()));
    app.run_widget_tree(&mut demo).await
}
