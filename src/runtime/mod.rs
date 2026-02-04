use crate::debug::DebugLayout;
use crate::driver::{Size, TerminalDriver};
use crate::event::{Action, ActionMap, Event, EventCtx, KeyBind};
use crate::render::FrameBuffer;
use crate::style::Theme;
use crate::widget::{StyleSheet, Widget, WidgetId, set_style_context};
use crate::{Error, Result};
use crossterm::event::MouseEventKind;
use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEventKind, KeyModifiers};
use rich_rs::{Console, ConsoleOptions, MetaValue, Renderable};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub struct App {
    driver: TerminalDriver,
    console: Console,
    options: ConsoleOptions,
    frame: FrameBuffer,
    debug_layout: DebugLayout,
    action_map: ActionMap,
    theme: Theme,
    default_stylesheet: StyleSheet,
    stylesheet: StyleSheet,
    stylesheet_watch: Option<StylesheetWatcher>,
    running: bool,
    hovered: Option<WidgetId>,
}

struct StylesheetWatcher {
    path: PathBuf,
    last_modified: Option<std::time::SystemTime>,
    interval: Duration,
    last_checked: Instant,
}

impl App {
    pub fn new() -> Result<Self> {
        let driver = TerminalDriver::new()?;
        let console = Console::new();
        let mut options = console.options().clone();
        let size = driver.size();
        apply_size(&mut options, size);
        let frame = FrameBuffer::new(size.width as usize, size.height as usize, None);
        Ok(Self {
            driver,
            console,
            options,
            frame,
            debug_layout: DebugLayout::default(),
            action_map: default_action_map(),
            theme: Theme::default(),
            default_stylesheet: crate::widget::default_widget_stylesheet(),
            stylesheet: StyleSheet::default(),
            stylesheet_watch: None,
            running: true,
            hovered: None,
        })
    }

    pub fn driver(&self) -> &TerminalDriver {
        &self.driver
    }

    pub fn set_debug_layout(&mut self, debug: DebugLayout) {
        self.debug_layout = debug;
    }

    pub fn enable_debug_layout(&mut self, enabled: bool) {
        self.debug_layout.enabled = enabled;
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    pub fn set_stylesheet(&mut self, stylesheet: StyleSheet) {
        self.stylesheet = stylesheet;
    }

    pub fn stylesheet_mut(&mut self) -> &mut StyleSheet {
        &mut self.stylesheet
    }

    pub fn load_stylesheet(&mut self, css: &str) {
        self.stylesheet = StyleSheet::parse(css);
    }

    pub fn load_stylesheet_file(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        let path = path.into();
        let css = fs::read_to_string(&path)?;
        self.stylesheet = StyleSheet::parse(&css);
        Ok(())
    }

    pub fn watch_stylesheet(&mut self, path: impl Into<PathBuf>, interval: Duration) -> Result<()> {
        let path = path.into();
        let css = fs::read_to_string(&path)?;
        self.stylesheet = StyleSheet::parse(&css);
        let last_modified = fs::metadata(&path).and_then(|m| m.modified()).ok();
        self.stylesheet_watch = Some(StylesheetWatcher {
            path,
            last_modified,
            interval: interval.max(Duration::from_millis(50)),
            last_checked: Instant::now(),
        });
        Ok(())
    }

    pub fn bind_key(&mut self, key: KeyBind, action: Action) {
        self.action_map.bind(key, action);
    }

    pub fn start(&mut self) -> Result<()> {
        self.driver.start()?;
        self.refresh_size()?;
        Ok(())
    }

    pub fn finish(&mut self) -> Result<()> {
        self.driver.stop()
    }

    pub fn render(&mut self, renderable: &dyn Renderable) -> Result<()> {
        self.refresh_size()?;
        let base_style = self.theme.base.to_rich();
        let next =
            FrameBuffer::from_renderable(&self.console, &self.options, renderable, base_style);
        let diff = next.diff_to_segments(&self.frame);
        self.console.print_segments(&diff)?;
        self.frame = next;
        Ok(())
    }

    pub fn render_widget(&mut self, widget: &dyn Widget) -> Result<()> {
        self.refresh_size()?;
        let mut sheet = self.default_stylesheet.clone();
        sheet.extend(&self.stylesheet);
        let _guard = set_style_context(sheet);
        let segments = if self.debug_layout.enabled {
            widget.render_styled_with_debug(&self.console, &self.options, &self.debug_layout)
        } else {
            widget.render_styled(&self.console, &self.options)
        };
        let (width, height) = self.options.size;
        let lines = rich_rs::Segment::split_and_crop_lines(segments, width, None, true, false);
        let base_style = self.theme.base.to_rich();
        let next = FrameBuffer::from_lines(&lines, width, height, base_style);
        let diff = next.diff_to_segments(&self.frame);
        self.console.print_segments(&diff)?;
        self.frame = next;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

    pub async fn run(&mut self) -> Result<()> {
        if !self.running {
            return Err(Error::RuntimeStopped);
        }
        // Placeholder event loop; real driver + frame diff will live here.
        self.start()?;
        Ok(())
    }

    pub async fn run_with<F, R>(&mut self, mut render: F) -> Result<()>
    where
        F: FnMut(&mut App, u64) -> R,
        R: Renderable,
    {
        if !self.running {
            return Err(Error::RuntimeStopped);
        }

        self.start()?;

        let mut tick: u64 = 0;
        let tick_rate = Duration::from_millis(100);
        let mut last_render = Instant::now();

        loop {
            let timeout = tick_rate.saturating_sub(last_render.elapsed());
            if event::poll(timeout)? {
                match event::read()? {
                    CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                        if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                            break;
                        }
                    }
                    CrosstermEvent::Resize(_, _) => {
                        self.refresh_size()?;
                    }
                    _ => {}
                }
            }

            if last_render.elapsed() >= tick_rate {
                self.poll_stylesheet();
                let renderable = render(self, tick);
                self.render(&renderable)?;
                tick += 1;
                last_render = Instant::now();
            }
        }

        self.finish()?;
        Ok(())
    }

    pub async fn run_widget_tree(&mut self, root: &mut dyn Widget) -> Result<()> {
        if !self.running {
            return Err(Error::RuntimeStopped);
        }

        self.start()?;
        root.on_mount();

        let mut tick: u64 = 0;
        let tick_rate = Duration::from_millis(100);
        let mut last_render = Instant::now();

        loop {
            let timeout = tick_rate.saturating_sub(last_render.elapsed());
            if event::poll(timeout)? {
                match event::read()? {
                    CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                        if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                            break;
                        }
                        let bind = KeyBind::from_event(&key);
                        if let Some(action) = self.action_map.lookup(&bind) {
                            dispatch_event(root, Event::Action(action));
                        } else {
                            dispatch_event(root, Event::Key(key));
                        }
                    }
                    CrosstermEvent::Mouse(mouse) => {
                        if matches!(mouse.kind, MouseEventKind::Moved) {
                            if self.update_hover_from_frame(mouse.column, mouse.row, root) {
                                self.render_widget(root)?;
                                last_render = Instant::now();
                            }
                        }
                    }
                    CrosstermEvent::Resize(_, _) => {
                        self.refresh_size()?;
                        let size = self.driver.size();
                        root.on_resize(size.width, size.height);
                        dispatch_event(root, Event::Resize(size.width, size.height));
                    }
                    _ => {}
                }
            }

            if last_render.elapsed() >= tick_rate {
                self.poll_stylesheet();
                root.on_tick(tick);
                dispatch_event(root, Event::Tick(tick));
                self.render_widget(root)?;
                tick += 1;
                last_render = Instant::now();
            }
        }

        root.on_unmount();
        self.finish()?;
        Ok(())
    }

    fn update_hover_from_frame(&mut self, x: u16, y: u16, root: &mut dyn Widget) -> bool {
        let x = x as usize;
        let y = y as usize;
        if x >= self.frame.width || y >= self.frame.height {
            return false;
        }

        let cell = self.frame.get(x, y);
        let hovered = cell
            .meta
            .as_ref()
            .and_then(|m| m.meta.as_ref())
            .and_then(|map| map.get("textual:widget_id"))
            .and_then(|value| match value {
                MetaValue::Int(n) if *n >= 0 => Some(WidgetId::from_u64(*n as u64)),
                _ => None,
            });

        let hovered = hovered.and_then(|id| {
            let enabled = crate::widget::hover_target_is_enabled(root, id);
            match enabled {
                Some(true) => Some(id),
                _ => None,
            }
        });

        if hovered != self.hovered {
            self.hovered = hovered;
            crate::widget::set_hover_by_id(root, self.hovered);
            return true;
        }

        false
    }

    fn refresh_size(&mut self) -> Result<()> {
        let size = self.driver.refresh_size()?;
        apply_size(&mut self.options, size);
        if self.frame.width != size.width as usize || self.frame.height != size.height as usize {
            self.frame = FrameBuffer::new(size.width as usize, size.height as usize, None);
        }
        Ok(())
    }

    fn poll_stylesheet(&mut self) {
        let Some(watch) = &mut self.stylesheet_watch else {
            return;
        };
        if watch.last_checked.elapsed() < watch.interval {
            return;
        }
        watch.last_checked = Instant::now();
        let Ok(meta) = fs::metadata(&watch.path) else {
            return;
        };
        let Ok(modified) = meta.modified() else {
            return;
        };
        let changed = watch
            .last_modified
            .map(|prev| modified > prev)
            .unwrap_or(true);
        if !changed {
            return;
        }
        if let Ok(css) = fs::read_to_string(&watch.path) {
            self.stylesheet = StyleSheet::parse(&css);
            watch.last_modified = Some(modified);
        }
    }
}

fn apply_size(options: &mut ConsoleOptions, size: Size) {
    let width = size.width as usize;
    let height = size.height as usize;
    options.size = (width, height);
    options.max_width = width;
    options.max_height = height;
}

fn default_action_map() -> ActionMap {
    let mut map = ActionMap::new();
    map.bind(
        KeyBind::new(KeyCode::Tab, KeyModifiers::empty()),
        Action::FocusNext,
    );
    map.bind(
        KeyBind::new(KeyCode::BackTab, KeyModifiers::SHIFT),
        Action::FocusPrev,
    );
    map.bind(
        KeyBind::new(KeyCode::Up, KeyModifiers::empty()),
        Action::ScrollUp,
    );
    map.bind(
        KeyBind::new(KeyCode::Down, KeyModifiers::empty()),
        Action::ScrollDown,
    );
    map.bind(
        KeyBind::new(KeyCode::PageUp, KeyModifiers::empty()),
        Action::ScrollPageUp,
    );
    map.bind(
        KeyBind::new(KeyCode::PageDown, KeyModifiers::empty()),
        Action::ScrollPageDown,
    );
    map.bind(
        KeyBind::new(KeyCode::Char('k'), KeyModifiers::empty()),
        Action::ScrollUp,
    );
    map.bind(
        KeyBind::new(KeyCode::Char('j'), KeyModifiers::empty()),
        Action::ScrollDown,
    );
    map.bind(
        KeyBind::new(KeyCode::Left, KeyModifiers::empty()),
        Action::ScrollLeft,
    );
    map.bind(
        KeyBind::new(KeyCode::Right, KeyModifiers::empty()),
        Action::ScrollRight,
    );
    map.bind(
        KeyBind::new(KeyCode::Char('h'), KeyModifiers::empty()),
        Action::ScrollLeft,
    );
    map.bind(
        KeyBind::new(KeyCode::Char('l'), KeyModifiers::empty()),
        Action::ScrollRight,
    );
    map.bind(
        KeyBind::new(KeyCode::Char(' '), KeyModifiers::empty()),
        Action::Toggle,
    );
    map.bind(
        KeyBind::new(KeyCode::Enter, KeyModifiers::empty()),
        Action::Toggle,
    );
    map
}

fn dispatch_event(root: &mut dyn Widget, event: Event) {
    let mut ctx = EventCtx::default();
    root.on_event_capture(&event, &mut ctx);
    if !ctx.handled() {
        root.on_event(&event, &mut ctx);
    }
}
