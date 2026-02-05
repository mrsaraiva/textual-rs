use crate::debug::{DebugLayout, debug_input, debug_render};
use crate::driver::{PointerShape, Size, TerminalDriver};
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

const SYNC_START: &str = "\x1b[?2026h";
const SYNC_END: &str = "\x1b[?2026l";

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
    last_render_at: Instant,
    resized_since_last_render: bool,
    last_resize_at: Option<Instant>,
    resize_burst: u64,
    sync_output: bool,
    pointer_shape: PointerShape,
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
        let sync_output = std::env::var("TEXTUAL_SYNC_OUTPUT")
            .ok()
            .map(|s| s != "0" && s.to_lowercase() != "false")
            .unwrap_or(true);
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
            last_render_at: Instant::now(),
            resized_since_last_render: false,
            last_resize_at: None,
            resize_burst: 0,
            sync_output,
            pointer_shape: PointerShape::Default,
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
        debug_render(&format!("[app] sync_output={}", self.sync_output));
        debug_render(&format!(
            "[app] pointer_shapes_enabled={}",
            self.driver.pointer_shapes_enabled()
        ));
        // Ensure we start in a known state.
        self.set_pointer_shape(PointerShape::Default)?;
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
        let now = Instant::now();
        let dt_ms = now.duration_since(self.last_render_at).as_millis();
        self.last_render_at = now;
        let diff = next.diff_to_segments(&self.frame);
        let mut controls = 0usize;
        let mut text_segments = 0usize;
        let mut text_bytes = 0usize;
        for seg in diff.iter() {
            if seg.control.is_some() {
                controls += 1;
            } else {
                if !seg.text.is_empty() {
                    text_segments += 1;
                    text_bytes += seg.text.len();
                }
            }
        }
        debug_render(&format!(
            "[render] dt={}ms resized={} size={}x{} prev={}x{} diff.segments={} (control={} text_segments={} text_bytes={})",
            dt_ms,
            self.resized_since_last_render,
            next.width,
            next.height,
            self.frame.width,
            self.frame.height,
            diff.len(),
            controls,
            text_segments,
            text_bytes
        ));
        self.resized_since_last_render = false;
        self.print_segments(&diff)?;
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
        let now = Instant::now();
        let dt_ms = now.duration_since(self.last_render_at).as_millis();
        self.last_render_at = now;
        let diff = next.diff_to_segments(&self.frame);
        let mut controls = 0usize;
        let mut text_segments = 0usize;
        let mut text_bytes = 0usize;
        for seg in diff.iter() {
            if seg.control.is_some() {
                controls += 1;
            } else {
                if !seg.text.is_empty() {
                    text_segments += 1;
                    text_bytes += seg.text.len();
                }
            }
        }
        debug_render(&format!(
            "[render_widget] dt={}ms resized={} size={}x{} prev={}x{} diff.segments={} (control={} text_segments={} text_bytes={})",
            dt_ms,
            self.resized_since_last_render,
            next.width,
            next.height,
            self.frame.width,
            self.frame.height,
            diff.len(),
            controls,
            text_segments,
            text_bytes
        ));
        self.resized_since_last_render = false;
        self.print_segments(&diff)?;
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
                        if matches!(key.code, KeyCode::Enter | KeyCode::Char(' ')) {
                            debug_input(&format!("[input] key {:?}", key.code));
                        }
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
                        match mouse.kind {
                            MouseEventKind::Moved => {
                                if self.update_hover_from_frame(mouse.column, mouse.row, root) {
                                    self.render_widget(root)?;
                                    last_render = Instant::now();
                                }
                            }
                            MouseEventKind::Down(_) => {
                                debug_input(&format!(
                                    "[input] mouse down x={} y={} hovered={:?}",
                                    mouse.column,
                                    mouse.row,
                                    self.hovered.map(|id| id.as_u64())
                                ));
                                if let Some(target) =
                                    self.hovered.or_else(|| self.widget_at(mouse.column, mouse.row))
                                {
                                    debug_input(&format!(
                                        "[input] mouse target id={}",
                                        target.as_u64()
                                    ));
                                    dispatch_event(root, Event::MouseDown(target));
                                }
                            }
                            _ => {}
                        }
                    }
                    CrosstermEvent::Resize(_, _) => {
                        let size = self.driver.size();
                        debug_render(&format!("[event] Resize({}x{})", size.width, size.height));
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

    fn print_segments(&mut self, diff: &rich_rs::Segments) -> Result<()> {
        console_write_with_optional_sync(&mut self.console, self.sync_output, |console| {
            console.print_segments(diff)
        })?;
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

        if hovered != self.hovered {
            self.hovered = hovered;
            crate::widget::set_hover_by_id(root, self.hovered);
            let shape = pointer_shape_for_hover(root, self.hovered);
            let _ = self.set_pointer_shape(shape);
            return true;
        }

        false
    }

    fn set_pointer_shape(&mut self, shape: PointerShape) -> Result<()> {
        if shape == self.pointer_shape {
            return Ok(());
        }
        self.pointer_shape = shape;
        self.driver.set_pointer_shape(shape)?;
        Ok(())
    }

    fn widget_at(&self, x: u16, y: u16) -> Option<WidgetId> {
        let x = x as usize;
        let y = y as usize;
        if x >= self.frame.width || y >= self.frame.height {
            return None;
        }
        let cell = self.frame.get(x, y);
        cell.meta
            .as_ref()
            .and_then(|m| m.meta.as_ref())
            .and_then(|map| map.get("textual:widget_id"))
            .and_then(|value| match value {
                MetaValue::Int(n) if *n >= 0 => Some(WidgetId::from_u64(*n as u64)),
                _ => None,
            })
    }

    fn refresh_size(&mut self) -> Result<()> {
        let size = self.driver.refresh_size()?;
        apply_size(&mut self.options, size);
        if self.frame.width != size.width as usize || self.frame.height != size.height as usize {
            let now = Instant::now();
            let dt_ms = self
                .last_resize_at
                .map(|t| now.duration_since(t).as_millis())
                .unwrap_or(0);
            self.last_resize_at = Some(now);
            self.resize_burst = self.resize_burst.saturating_add(1);
            debug_render(&format!(
                "[app] resize: burst={} dt={}ms frame {}x{} -> {}x{} (reset framebuffer to blanks; no explicit clear)",
                self.resize_burst,
                dt_ms,
                self.frame.width,
                self.frame.height,
                size.width,
                size.height
            ));
            self.frame = FrameBuffer::new(size.width as usize, size.height as usize, None);
            self.resized_since_last_render = true;
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

fn pointer_shape_for_hover(root: &mut dyn Widget, hovered: Option<WidgetId>) -> PointerShape {
    let Some(id) = hovered else {
        return PointerShape::Default;
    };

    // Traverse the widget tree to locate the hovered widget.
    let mut found: Option<(bool, bool, &'static str)> = None; // (focusable, disabled, type)
    fn visit(
        w: &mut dyn Widget,
        id: WidgetId,
        out: &mut Option<(bool, bool, &'static str)>,
    ) {
        if out.is_some() {
            return;
        }
        if w.id() == id {
            *out = Some((w.focusable(), w.is_disabled(), w.style_type()));
            return;
        }
        w.visit_children_mut(&mut |child| visit(child, id, out));
    }

    visit(root, id, &mut found);

    let Some((focusable, disabled, ty)) = found else {
        return PointerShape::Default;
    };

    if !focusable {
        return PointerShape::Default;
    }

    if ty == "Input" {
        return PointerShape::Text;
    }

    if disabled {
        PointerShape::NotAllowed
    } else {
        PointerShape::Pointer
    }
}

fn console_write_with_optional_sync<W: std::io::Write>(
    console: &mut rich_rs::Console<W>,
    sync_enabled: bool,
    write_payload: impl FnOnce(&mut rich_rs::Console<W>) -> std::io::Result<()>,
) -> std::io::Result<()> {
    if sync_enabled {
        console.write_str(SYNC_START)?;
    }

    write_payload(console)?;

    if sync_enabled {
        console.write_str(SYNC_END)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_output_wraps_payload_when_enabled() {
        let mut console = rich_rs::Console::capture();
        console_write_with_optional_sync(&mut console, true, |console| {
            console.write_str("PAYLOAD")
        })
        .unwrap();
        let out = console.get_captured_bytes();
        assert!(out.starts_with(SYNC_START.as_bytes()));
        assert!(out.ends_with(SYNC_END.as_bytes()));
        assert!(out.windows(b"PAYLOAD".len()).any(|w| w == b"PAYLOAD"));
    }

    #[test]
    fn sync_output_does_not_wrap_payload_when_disabled() {
        let mut console = rich_rs::Console::capture();
        console_write_with_optional_sync(&mut console, false, |console| {
            console.write_str("PAYLOAD")
        })
        .unwrap();
        let out = console.get_captured_bytes();
        assert_eq!(out, b"PAYLOAD");
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
