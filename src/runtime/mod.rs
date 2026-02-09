use crate::animation::{animation_level_from_env, Animator};
use crate::css::{default_widget_stylesheet, set_app_active, set_style_context, StyleSheet};
use crate::debug::{debug_input, debug_message, debug_render, DebugLayout};
use crate::driver::{DriverOptions, KeyboardProtocol, PointerShape, Size, TerminalDriver};
use crate::event::{
    Action, ActionMap, AnimationRequest, AnimationValueEvent, BindingHint, Event, EventCtx,
    KeyBind, MouseDownEvent, MouseScrollEvent, MouseUpEvent,
};
use crate::keys::KeyEventData;
use crate::message::MessageEvent;
use crate::render::FrameBuffer;
use crate::style::Theme;
use crate::widgets::{border_spacing_from_style, Toast, ToastSeverity, Widget, WidgetId};
use crate::{Error, Result};
use crossterm::event::MouseEventKind;
use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEventKind, KeyModifiers};
use rich_rs::{Console, ConsoleOptions, ControlType, MetaValue, Renderable, Segment, Segments};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const SYNC_START: &str = "\x1b[?2026h";
const SYNC_END: &str = "\x1b[?2026l";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    x0: u16,
    y0: u16,
    x1: u16,
    y1: u16,
}

#[derive(Debug, Default, Clone)]
struct HitTestMap {
    bounds: HashMap<WidgetId, Rect>,
}

impl HitTestMap {
    fn from_frame(frame: &FrameBuffer) -> Self {
        let mut out = HitTestMap::default();
        for y in 0..frame.height {
            for x in 0..frame.width {
                let cell = frame.get(x, y);
                let Some(meta) = cell.meta.as_ref() else {
                    continue;
                };
                let Some(map) = meta.meta.as_ref() else {
                    continue;
                };
                let Some(MetaValue::Int(id)) = map.get("textual:widget_id") else {
                    continue;
                };
                if *id < 0 {
                    continue;
                }
                let wid = WidgetId::from_u64(*id as u64);
                let xu = x as u16;
                let yu = y as u16;
                out.bounds
                    .entry(wid)
                    .and_modify(|r| {
                        r.x0 = r.x0.min(xu);
                        r.y0 = r.y0.min(yu);
                        r.x1 = r.x1.max(xu);
                        r.y1 = r.y1.max(yu);
                    })
                    .or_insert(Rect {
                        x0: xu,
                        y0: yu,
                        x1: xu,
                        y1: yu,
                    });
            }
        }
        out
    }

    fn rect(&self, id: WidgetId) -> Option<Rect> {
        self.bounds.get(&id).copied()
    }

    fn content_local_coords(
        &self,
        root: &mut dyn Widget,
        target: WidgetId,
        screen_x: u16,
        screen_y: u16,
    ) -> (u16, u16) {
        let Some(rect) = self.rect(target) else {
            return (0, 0);
        };

        let mut insets: Option<(u16, u16)> = None;
        fn visit(w: &mut dyn Widget, id: WidgetId, out: &mut Option<(u16, u16)>) {
            if out.is_some() {
                return;
            }
            if w.id() == id {
                let meta = crate::css::selector_meta_generic(w);
                let resolved = crate::css::resolve_style(w, &meta);
                let line_pad = resolved.line_pad.unwrap_or(0);
                let (top, _bottom, left, _right) = border_spacing_from_style(&resolved);
                let inset_x = left.saturating_add(line_pad) as u16;
                let inset_y = top as u16;
                *out = Some((inset_x, inset_y));
                return;
            }
            w.visit_children_mut(&mut |child| visit(child, id, out));
        }
        visit(root, target, &mut insets);
        let (inset_x, inset_y) = insets.unwrap_or((0, 0));

        let origin_x = rect.x0.saturating_add(inset_x);
        let origin_y = rect.y0.saturating_add(inset_y);
        (
            screen_x.saturating_sub(origin_x),
            screen_y.saturating_sub(origin_y),
        )
    }
}

pub struct App {
    driver: TerminalDriver,
    console: Console,
    options: ConsoleOptions,
    frame: FrameBuffer,
    hit_test: HitTestMap,
    debug_layout: DebugLayout,
    action_map: ActionMap,
    quit_keys: Vec<KeyBind>,
    custom_binding_hints: Vec<BindingHintEntry>,
    command_palette_hint: Option<BindingHintEntry>,
    theme: Theme,
    default_stylesheet: StyleSheet,
    stylesheet: StyleSheet,
    stylesheet_watch: Option<StylesheetWatcher>,
    running: bool,
    hovered: Option<WidgetId>,
    last_render_at: Instant,
    resized_since_last_render: bool,
    clear_on_next_render: bool,
    last_resize_at: Option<Instant>,
    resize_burst: u64,
    sync_output: bool,
    pointer_shape: PointerShape,
    app_active: bool,
    last_binding_hints: Vec<BindingHint>,
    animator: Animator,
    animation_level: crate::event::AnimationLevel,
    notifications: Vec<AppNotification>,
}

#[derive(Debug, Clone)]
struct BindingHintEntry {
    key: KeyBind,
    hint: BindingHint,
}

#[derive(Debug, Clone)]
struct AppNotification {
    title: String,
    message: String,
    severity: ToastSeverity,
    expires_at: Instant,
}

impl AppNotification {
    fn new(
        title: impl Into<String>,
        message: impl Into<String>,
        severity: ToastSeverity,
        timeout: Duration,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            severity,
            expires_at: Instant::now() + timeout,
        }
    }
}

const DEFAULT_NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(5);

struct StylesheetWatcher {
    path: PathBuf,
    last_modified: Option<std::time::SystemTime>,
    interval: Duration,
    last_checked: Instant,
}

impl App {
    pub fn new() -> Result<Self> {
        let mut options = DriverOptions::default();
        // Preserve textual-rs behavior: mouse capture enabled by default.
        options.enable_mouse = true;
        // Enable xterm focus change reporting so widgets can react to window focus.
        options.enable_focus_change = true;
        // Default to Auto so supported terminals can enable richer key reporting,
        // while still allowing TEXTUAL_KEYBOARD_PROTOCOL overrides.
        options.keyboard_protocol = KeyboardProtocol::Auto;
        let driver = TerminalDriver::new(options)?;
        let console = Console::new();
        let mut options = console.options().clone();
        let size = driver.size();
        apply_size(&mut options, size);
        let frame = FrameBuffer::new(size.width as usize, size.height as usize, None);
        let sync_output = std::env::var("TEXTUAL_SYNC_OUTPUT")
            .ok()
            .map(|s| s != "0" && s.to_lowercase() != "false")
            .unwrap_or(true);
        let mut app = Self {
            driver,
            console,
            options,
            frame,
            hit_test: HitTestMap::default(),
            debug_layout: DebugLayout::default(),
            action_map: default_action_map(),
            quit_keys: vec![KeyBind::new(KeyCode::Char('q'), KeyModifiers::CONTROL)],
            custom_binding_hints: Vec::new(),
            command_palette_hint: Some(BindingHintEntry {
                key: KeyBind::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
                hint: BindingHint::new("ctrl+p", "palette")
                    .with_key_display("^p")
                    .with_group("command_palette")
                    .with_priority(true)
                    .with_system(true),
            }),
            theme: Theme::default(),
            default_stylesheet: default_widget_stylesheet(),
            stylesheet: StyleSheet::default(),
            stylesheet_watch: None,
            running: true,
            hovered: None,
            last_render_at: Instant::now(),
            resized_since_last_render: false,
            clear_on_next_render: false,
            last_resize_at: None,
            resize_burst: 0,
            sync_output,
            pointer_shape: PointerShape::Default,
            app_active: true,
            last_binding_hints: Vec::new(),
            animator: Animator::new(60),
            animation_level: animation_level_from_env(),
            notifications: Vec::new(),
        };
        app.last_binding_hints = app.binding_hints();
        Ok(app)
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

    pub fn binding_hints(&self) -> Vec<BindingHint> {
        let mut out = Vec::new();
        for quit in &self.quit_keys {
            out.push(
                BindingHint::new(quit.display_key(), "Quit application")
                    .hidden(true)
                    .with_priority(true)
                    .with_system(true),
            );
        }
        for (bind, action) in self.action_map.entries() {
            out.push(
                BindingHint::new(bind.display_key(), action.description())
                    .hidden(true)
                    .with_system(true),
            );
        }
        out.extend(
            self.custom_binding_hints
                .iter()
                .map(|entry| entry.hint.clone()),
        );
        if let Some(entry) = &self.command_palette_hint {
            out.push(entry.hint.clone());
        }

        let mut unique = HashSet::new();
        out.retain(|entry| {
            unique.insert((
                entry.key.clone(),
                entry.description.clone(),
                entry.show,
                entry.key_display.clone(),
                entry.group.clone(),
                entry.priority,
                entry.system,
            ))
        });
        let mut prioritized = Vec::new();
        let mut regular = Vec::new();
        for entry in out {
            if entry.priority {
                prioritized.push(entry);
            } else {
                regular.push(entry);
            }
        }
        prioritized.extend(regular);
        prioritized
    }

    pub fn set_command_palette_hint(&mut self, enabled: bool) {
        if enabled {
            if self.command_palette_hint.is_none() {
                self.command_palette_hint = Some(BindingHintEntry {
                    key: KeyBind::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
                    hint: BindingHint::new("ctrl+p", "palette")
                        .with_key_display("^p")
                        .with_group("command_palette")
                        .with_priority(true)
                        .with_system(true),
                });
            }
        } else {
            self.command_palette_hint = None;
        }
    }

    pub fn set_command_palette_binding(
        &mut self,
        key: KeyBind,
        key_display: impl Into<String>,
        description: impl Into<String>,
    ) {
        self.command_palette_hint = Some(BindingHintEntry {
            key,
            hint: BindingHint::new(key.display_key(), description.into())
                .with_key_display(key_display.into())
                .with_group("command_palette")
                .with_priority(true)
                .with_system(true),
        });
    }

    pub fn add_binding_hint(&mut self, key: KeyBind, description: impl Into<String>) {
        self.add_binding_hint_with_options(key, BindingHint::new(key.display_key(), description));
    }

    pub fn add_binding_hint_with_options(&mut self, key: KeyBind, mut hint: BindingHint) {
        if hint.key.is_empty() {
            hint.key = key.display_key();
        }
        self.custom_binding_hints.retain(|existing| {
            !(existing.key == key && existing.hint.description == hint.description)
        });
        self.custom_binding_hints
            .push(BindingHintEntry { key, hint });
    }

    pub fn add_hidden_binding_hint(&mut self, key: KeyBind, description: impl Into<String>) {
        self.add_binding_hint_with_options(
            key,
            BindingHint::new(key.display_key(), description).hidden(true),
        );
    }

    pub fn clear_binding_hints(&mut self) {
        self.custom_binding_hints.clear();
    }

    pub fn remove_binding_hint(&mut self, key: KeyBind, description: &str) -> bool {
        let before = self.custom_binding_hints.len();
        self.custom_binding_hints
            .retain(|entry| !(entry.key == key && entry.hint.description == description));
        self.custom_binding_hints.len() != before
    }

    pub fn visible_binding_hints(&self) -> Vec<BindingHint> {
        self.binding_hints()
            .into_iter()
            .filter(|hint| hint.show)
            .collect()
    }

    pub fn set_quit_keys(&mut self, quit_keys: Vec<KeyBind>) {
        self.quit_keys = quit_keys;
    }

    pub fn clear_quit_keys(&mut self) {
        self.quit_keys.clear();
    }

    pub fn notify(
        &mut self,
        message: impl Into<String>,
        title: impl Into<String>,
        severity: ToastSeverity,
        timeout: Option<Duration>,
    ) {
        let timeout = timeout.unwrap_or(DEFAULT_NOTIFICATION_TIMEOUT);
        self.notifications
            .push(AppNotification::new(title, message, severity, timeout));
    }

    fn notify_help_quit(&mut self) {
        let key = self
            .quit_keys
            .iter()
            .find(|bind| {
                bind.modifiers.contains(KeyModifiers::CONTROL)
                    || bind.modifiers.contains(KeyModifiers::SUPER)
            })
            .or_else(|| self.quit_keys.first())
            .map(|bind| bind.key_name())
            .unwrap_or_else(|| "ctrl+q".to_string());
        self.notify(
            format!("Press {key} to quit the app"),
            "Do you want to quit?",
            ToastSeverity::Information,
            Some(DEFAULT_NOTIFICATION_TIMEOUT),
        );
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
            self.driver.options().enable_pointer_shapes
        ));
        // Ensure we start in a known state.
        self.set_pointer_shape(PointerShape::Default)?;
        Ok(())
    }

    pub fn finish(&mut self) -> Result<()> {
        Ok(self.driver.stop()?)
    }

    pub fn render(&mut self, renderable: &dyn Renderable) -> Result<()> {
        self.refresh_size()?;
        let base_style = self.theme.base.to_rich();
        let next =
            FrameBuffer::from_renderable(&self.console, &self.options, renderable, base_style);
        let now = Instant::now();
        let dt_ms = now.duration_since(self.last_render_at).as_millis();
        self.last_render_at = now;
        let clear_before_draw = self.clear_on_next_render;
        let diff = prepend_clear_if_needed(next.diff_to_segments(&self.frame), clear_before_draw);
        let stream_stats = analyze_segment_stream(&diff, next.width);
        debug_render(&format!(
            "[render] dt={}ms resized={} clear={} size={}x{} prev={}x{} diff.segments={} (control={} text_segments={} text_bytes={})",
            dt_ms,
            self.resized_since_last_render,
            clear_before_draw,
            next.width,
            next.height,
            self.frame.width,
            self.frame.height,
            diff.len(),
            stream_stats.controls,
            stream_stats.text_segments,
            stream_stats.text_bytes
        ));
        if resize_trace_enabled() && (self.resized_since_last_render || clear_before_draw) {
            debug_render(&format!(
                "[render_trace] kind=render size={}x{} controls={} home={} clear={} cr={} move_to={} cursor_moves={} text_segments={} text_bytes={} newlines={} touch_last_col={} overflow_right={} max_cursor=({}, {}) control_head=[{}]",
                next.width,
                next.height,
                stream_stats.controls,
                stream_stats.home,
                stream_stats.clear,
                stream_stats.carriage_return,
                stream_stats.move_to,
                stream_stats.cursor_moves,
                stream_stats.text_segments,
                stream_stats.text_bytes,
                stream_stats.newline_text,
                stream_stats.touch_last_col,
                stream_stats.overflow_right,
                stream_stats.max_cursor_x,
                stream_stats.max_cursor_y,
                control_head(&diff, 12)
            ));
        }
        self.print_segments(&diff)?;
        self.resized_since_last_render = false;
        self.clear_on_next_render = false;
        self.frame = next;
        Ok(())
    }

    pub fn render_widget(&mut self, widget: &mut dyn Widget) -> Result<()> {
        self.refresh_size()?;
        let mut sheet = self.default_stylesheet.clone();
        sheet.extend(&self.stylesheet);
        let _active = set_app_active(self.app_active);
        let _guard = set_style_context(sheet);
        let segments = if self.debug_layout.enabled {
            widget.render_styled_with_debug(&self.console, &self.options, &self.debug_layout)
        } else {
            widget.render_styled(&self.console, &self.options)
        };
        let (width, height) = self.options.size;
        let lines = rich_rs::Segment::split_and_crop_lines(segments, width, None, true, false);
        let base_style = self.theme.base.to_rich();
        let mut next = FrameBuffer::from_lines(&lines, width, height, base_style);
        self.compose_notifications(&mut next);
        let now = Instant::now();
        let dt_ms = now.duration_since(self.last_render_at).as_millis();
        self.last_render_at = now;
        let clear_before_draw = self.clear_on_next_render;
        let diff = prepend_clear_if_needed(next.diff_to_segments(&self.frame), clear_before_draw);
        let stream_stats = analyze_segment_stream(&diff, next.width);
        debug_render(&format!(
            "[render_widget] dt={}ms resized={} clear={} size={}x{} prev={}x{} diff.segments={} (control={} text_segments={} text_bytes={})",
            dt_ms,
            self.resized_since_last_render,
            clear_before_draw,
            next.width,
            next.height,
            self.frame.width,
            self.frame.height,
            diff.len(),
            stream_stats.controls,
            stream_stats.text_segments,
            stream_stats.text_bytes
        ));
        if resize_trace_enabled() && (self.resized_since_last_render || clear_before_draw) {
            debug_render(&format!(
                "[render_trace] kind=widget size={}x{} controls={} home={} clear={} cr={} move_to={} cursor_moves={} text_segments={} text_bytes={} newlines={} touch_last_col={} overflow_right={} max_cursor=({}, {}) control_head=[{}]",
                next.width,
                next.height,
                stream_stats.controls,
                stream_stats.home,
                stream_stats.clear,
                stream_stats.carriage_return,
                stream_stats.move_to,
                stream_stats.cursor_moves,
                stream_stats.text_segments,
                stream_stats.text_bytes,
                stream_stats.newline_text,
                stream_stats.touch_last_col,
                stream_stats.overflow_right,
                stream_stats.max_cursor_x,
                stream_stats.max_cursor_y,
                control_head(&diff, 12)
            ));
        }
        self.print_segments(&diff)?;
        self.resized_since_last_render = false;
        self.clear_on_next_render = false;
        self.hit_test = HitTestMap::from_frame(&next);
        self.apply_layout_info(widget);
        self.frame = next;
        Ok(())
    }

    fn compose_notifications(&mut self, frame: &mut FrameBuffer) {
        if self.notifications.is_empty() {
            return;
        }

        let mut cursor_bottom = frame.height.saturating_sub(2);
        for note in self.notifications.iter().rev() {
            let mut toast = Toast::new(note.message.clone(), note.severity);
            if !note.title.is_empty() {
                toast = toast.with_title(note.title.clone());
            }

            let max_width = frame.width.saturating_sub(2).max(1);
            let preferred = 60usize.min((frame.width / 2).max(1));
            let toast_width = preferred.min(max_width).max(1);
            let toast_height = toast.layout_height().unwrap_or(3).max(1);
            if toast_height > frame.height {
                continue;
            }
            if cursor_bottom + 1 < toast_height {
                break;
            }

            let mut toast_options = self.options.clone();
            toast_options.size = (toast_width, toast_height);
            toast_options.max_width = toast_width;
            toast_options.max_height = toast_height;

            let rendered = toast.render_styled(&self.console, &toast_options);
            let lines = Segment::split_and_crop_lines(rendered, toast_width, None, true, false);
            let lines = Segment::set_shape(&lines, toast_width, Some(toast_height), None, false);
            let toast_buffer = FrameBuffer::from_lines(&lines, toast_width, toast_height, None);

            let x0 = frame.width.saturating_sub(toast_width + 1);
            let y0 = cursor_bottom + 1 - toast_height;
            for y in 0..toast_height {
                for x in 0..toast_width {
                    let cell = toast_buffer.get(x, y).clone();
                    if cell.continuation {
                        continue;
                    }
                    let tx = x0 + x;
                    let ty = y0 + y;
                    if tx < frame.width && ty < frame.height {
                        *frame.get_mut(tx, ty) = cell;
                    }
                }
            }

            cursor_bottom = y0.saturating_sub(1);
            if cursor_bottom == 0 {
                break;
            }
        }
    }

    fn apply_layout_info(&self, root: &mut dyn Widget) {
        fn visit(w: &mut dyn Widget, hit_test: &HitTestMap) {
            if let Some(rect) = hit_test.rect(w.id()) {
                let meta = crate::css::selector_meta_generic(w);
                let resolved = crate::css::resolve_style(w, &meta);
                let line_pad = resolved.line_pad.unwrap_or(0);
                let (top, bottom, left, right) = border_spacing_from_style(&resolved);
                let full_w = rect.x1.saturating_sub(rect.x0) as usize + 1;
                let full_h = rect.y1.saturating_sub(rect.y0) as usize + 1;
                let content_w = full_w
                    .saturating_sub(left + right)
                    .saturating_sub(line_pad.saturating_mul(2))
                    .max(1) as u16;
                let content_h = full_h.saturating_sub(top + bottom).max(1) as u16;
                w.on_layout(content_w, content_h);
            }
            w.visit_children_mut(&mut |child| visit(child, hit_test));
        }
        visit(root, &self.hit_test);
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
                        if should_quit_key(&key, &self.quit_keys) {
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
                let _ = self.poll_stylesheet();
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

        // Auto-focus the first focusable widget.
        let mut ids = Vec::new();
        crate::widgets::collect_focus_ids(root, &mut ids);
        if let Some(first) = ids.first().copied() {
            crate::widgets::set_focus_by_id(root, Some(first));
        }

        let mut tick: u64 = 0;
        let idle_tick_rate = Duration::from_millis(100);
        let active_tick_rate = Duration::from_millis(16);
        let mut dirty = false;
        let mut prev_any_active = false;
        self.render_widget(root)?;
        let mut last_render = Instant::now();

        'event_loop: loop {
            let now = Instant::now();
            let has_runtime_animation = self.animator.has_animations();
            let tick_rate = if has_runtime_animation || prev_any_active {
                active_tick_rate
            } else {
                idle_tick_rate
            };
            let tick_timeout = tick_rate.saturating_sub(last_render.elapsed());
            let timeout = self
                .animator
                .next_timeout(now)
                .map(|anim_timeout| tick_timeout.min(anim_timeout))
                .unwrap_or(tick_timeout);
            if event::poll(timeout)? {
                let mut sheet = self.default_stylesheet.clone();
                sheet.extend(&self.stylesheet);
                let _active = set_app_active(self.app_active);
                let _guard = set_style_context(sheet);
                match event::read()? {
                    CrosstermEvent::Key(key) => {
                        debug_input(&format!(
                            "[input] key code={:?} mods={:?} kind={:?}",
                            key.code, key.modifiers, key.kind
                        ));
                        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                            continue;
                        }
                        if should_quit_key(&key, &self.quit_keys) {
                            break;
                        }
                        let key = KeyEventData::from_crossterm(key);
                        let bind = KeyBind::from_event(&key);
                        let mapped_action = self.action_map.lookup(&bind);

                        // Priority actions (e.g. command palette) run before raw key dispatch.
                        if let Some(action) = mapped_action.filter(|a| is_priority_action(*a)) {
                            debug_input(&format!(
                                "[input] priority action-map {:?} -> {:?}",
                                bind, action
                            ));
                            let mut outcome = dispatch_event(root, Event::Action(action));
                            debug_input(&format!(
                                "[input] priority action dispatch action={:?} handled={} repaint={} messages={}",
                                action,
                                outcome.handled,
                                outcome.repaint_requested,
                                outcome.messages.len()
                            ));
                            self.absorb_outcome(&mut outcome, &mut dirty);
                            let mut msg_outcome = dispatch_message_queue(root, outcome.messages);
                            self.absorb_outcome(&mut msg_outcome, &mut dirty);
                            if outcome.stop_requested || msg_outcome.stop_requested {
                                break 'event_loop;
                            }
                            if outcome.handled {
                                continue;
                            }
                        }

                        // Dispatch the raw key so focused widgets (e.g. Input) can consume it.
                        let mut key_outcome = dispatch_event(root, Event::Key(key.clone()));
                        debug_input(&format!(
                            "[input] key dispatch handled={} repaint={} messages={}",
                            key_outcome.handled,
                            key_outcome.repaint_requested,
                            key_outcome.messages.len()
                        ));
                        self.absorb_outcome(&mut key_outcome, &mut dirty);
                        let mut msg_outcome = dispatch_message_queue(root, key_outcome.messages);
                        self.absorb_outcome(&mut msg_outcome, &mut dirty);
                        if key_outcome.stop_requested || msg_outcome.stop_requested {
                            break 'event_loop;
                        }
                        if !key_outcome.handled {
                            if let Some(action) = mapped_action.filter(|a| !is_priority_action(*a))
                            {
                                if action == Action::HelpQuit {
                                    self.notify_help_quit();
                                    dirty = true;
                                    continue;
                                }
                                debug_input(&format!(
                                    "[input] action-map {:?} -> {:?}",
                                    bind, action
                                ));
                                let mut outcome = if is_scroll_action(action) {
                                    dispatch_scroll_action(root, action, self.hovered)
                                } else {
                                    dispatch_event(root, Event::Action(action))
                                };
                                debug_input(&format!(
                                    "[input] action dispatch action={:?} handled={} repaint={} messages={}",
                                    action,
                                    outcome.handled,
                                    outcome.repaint_requested,
                                    outcome.messages.len()
                                ));
                                self.absorb_outcome(&mut outcome, &mut dirty);
                                let mut msg_outcome =
                                    dispatch_message_queue(root, outcome.messages);
                                self.absorb_outcome(&mut msg_outcome, &mut dirty);
                                if outcome.stop_requested || msg_outcome.stop_requested {
                                    break 'event_loop;
                                }
                            } else {
                                debug_input(&format!("[input] action-map {:?} -> none", bind));
                            }
                        }
                    }
                    CrosstermEvent::Mouse(mouse) => match mouse.kind {
                        MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                            if self.update_hover_from_frame(mouse.column, mouse.row, root) {
                                dirty = true;
                            }
                        }
                        MouseEventKind::Down(_) => {
                            debug_input(&format!(
                                "[input] mouse down x={} y={} hovered={:?}",
                                mouse.column,
                                mouse.row,
                                self.hovered.map(|id| id.as_u64())
                            ));
                            if let Some(target) = self.widget_at(mouse.column, mouse.row) {
                                let (x, y) = self.hit_test.content_local_coords(
                                    root,
                                    target,
                                    mouse.column,
                                    mouse.row,
                                );
                                debug_input(&format!(
                                    "[input] mouse target id={}",
                                    target.as_u64()
                                ));
                                let mut outcome = dispatch_event(
                                    root,
                                    Event::MouseDown(MouseDownEvent {
                                        target,
                                        screen_x: mouse.column,
                                        screen_y: mouse.row,
                                        x,
                                        y,
                                    }),
                                );
                                self.absorb_outcome(&mut outcome, &mut dirty);
                                let mut msg_outcome =
                                    dispatch_message_queue(root, outcome.messages);
                                self.absorb_outcome(&mut msg_outcome, &mut dirty);
                                if outcome.stop_requested || msg_outcome.stop_requested {
                                    break 'event_loop;
                                }
                            }
                        }
                        MouseEventKind::Up(_) => {
                            let target = self.widget_at(mouse.column, mouse.row);
                            let (x, y) = target
                                .map(|id| {
                                    self.hit_test.content_local_coords(
                                        root,
                                        id,
                                        mouse.column,
                                        mouse.row,
                                    )
                                })
                                .unwrap_or((0, 0));
                            let mut outcome = dispatch_event(
                                root,
                                Event::MouseUp(MouseUpEvent {
                                    target,
                                    screen_x: mouse.column,
                                    screen_y: mouse.row,
                                    x,
                                    y,
                                }),
                            );
                            self.absorb_outcome(&mut outcome, &mut dirty);
                            let mut msg_outcome = dispatch_message_queue(root, outcome.messages);
                            self.absorb_outcome(&mut msg_outcome, &mut dirty);
                            if outcome.stop_requested || msg_outcome.stop_requested {
                                break 'event_loop;
                            }
                        }
                        MouseEventKind::ScrollUp
                        | MouseEventKind::ScrollDown
                        | MouseEventKind::ScrollLeft
                        | MouseEventKind::ScrollRight => {
                            debug_input(&format!(
                                "[input] mouse scroll kind={:?} mods={:?} x={} y={}",
                                mouse.kind, mouse.modifiers, mouse.column, mouse.row
                            ));
                            if self.update_hover_from_frame(mouse.column, mouse.row, root) {
                                dirty = true;
                            }
                            let (delta_x, delta_y) =
                                mouse_scroll_deltas(mouse.kind, mouse.modifiers);
                            let target = self.widget_at(mouse.column, mouse.row);
                            let (local_x, local_y) = target
                                .map(|id| {
                                    self.hit_test.content_local_coords(
                                        root,
                                        id,
                                        mouse.column,
                                        mouse.row,
                                    )
                                })
                                .unwrap_or((0, 0));
                            debug_input(&format!(
                                "[input] mouse scroll route target={:?} dx={} dy={}",
                                target.map(|id| id.as_u64()),
                                delta_x,
                                delta_y
                            ));
                            let mut diag_outcome = if let Some(target) = target {
                                dispatch_event_to_target(
                                    root,
                                    target,
                                    &Event::MouseScroll(MouseScrollEvent {
                                        target: Some(target),
                                        screen_x: mouse.column,
                                        screen_y: mouse.row,
                                        x: local_x,
                                        y: local_y,
                                        delta_x,
                                        delta_y,
                                        modifiers: mouse.modifiers,
                                    }),
                                )
                            } else {
                                dispatch_event(
                                    root,
                                    Event::MouseScroll(MouseScrollEvent {
                                        target: None,
                                        screen_x: mouse.column,
                                        screen_y: mouse.row,
                                        x: local_x,
                                        y: local_y,
                                        delta_x,
                                        delta_y,
                                        modifiers: mouse.modifiers,
                                    }),
                                )
                            };
                            self.absorb_outcome(&mut diag_outcome, &mut dirty);
                            let mut msg_outcome =
                                dispatch_message_queue(root, diag_outcome.messages);
                            self.absorb_outcome(&mut msg_outcome, &mut dirty);
                            let mut outcome = if let Some(target) = target {
                                dispatch_mouse_scroll_to_target(root, target, delta_x, delta_y)
                            } else {
                                dispatch_mouse_scroll(root, delta_x, delta_y)
                            };
                            debug_input(&format!(
                                "[input] mouse scroll dispatch handled={} repaint={} messages={}",
                                outcome.handled,
                                outcome.repaint_requested,
                                outcome.messages.len()
                            ));
                            self.absorb_outcome(&mut outcome, &mut dirty);
                            let mut msg_outcome = dispatch_message_queue(root, outcome.messages);
                            self.absorb_outcome(&mut msg_outcome, &mut dirty);
                            if diag_outcome.stop_requested
                                || outcome.stop_requested
                                || msg_outcome.stop_requested
                            {
                                break 'event_loop;
                            }
                        }
                    },
                    CrosstermEvent::Resize(_, _) => {
                        let size = self.driver.size();
                        debug_render(&format!("[event] Resize({}x{})", size.width, size.height));
                        self.refresh_size()?;
                        let size = self.driver.size();
                        root.on_resize(size.width, size.height);
                        let mut outcome =
                            dispatch_event(root, Event::Resize(size.width, size.height));
                        self.absorb_outcome(&mut outcome, &mut dirty);
                        let mut msg_outcome = dispatch_message_queue(root, outcome.messages);
                        self.absorb_outcome(&mut msg_outcome, &mut dirty);
                        if outcome.stop_requested || msg_outcome.stop_requested {
                            break 'event_loop;
                        }
                    }
                    CrosstermEvent::FocusLost => {
                        self.app_active = false;
                        debug_input("[event] FocusLost");
                        let mut outcome = dispatch_event(root, Event::AppFocus(false));
                        self.absorb_outcome(&mut outcome, &mut dirty);
                        let mut msg_outcome = dispatch_message_queue(root, outcome.messages);
                        self.absorb_outcome(&mut msg_outcome, &mut dirty);
                        if outcome.stop_requested || msg_outcome.stop_requested {
                            break 'event_loop;
                        }
                    }
                    CrosstermEvent::FocusGained => {
                        self.app_active = true;
                        debug_input("[event] FocusGained");
                        let mut outcome = dispatch_event(root, Event::AppFocus(true));
                        self.absorb_outcome(&mut outcome, &mut dirty);
                        let mut msg_outcome = dispatch_message_queue(root, outcome.messages);
                        self.absorb_outcome(&mut msg_outcome, &mut dirty);
                        if outcome.stop_requested || msg_outcome.stop_requested {
                            break 'event_loop;
                        }
                    }
                    _ => {}
                }
            }

            let mut binding_outcome = self.dispatch_binding_hints_changed(root);
            self.absorb_outcome(&mut binding_outcome, &mut dirty);
            if binding_outcome.stop_requested {
                break 'event_loop;
            }

            let mut animation_outcome = self.dispatch_animation_frame(root);
            self.absorb_outcome(&mut animation_outcome, &mut dirty);
            if animation_outcome.stop_requested {
                break 'event_loop;
            }

            if dirty || self.resized_since_last_render {
                self.render_widget(root)?;
                dirty = false;
                last_render = Instant::now();
            }

            if last_render.elapsed() >= tick_rate {
                let mut sheet = self.default_stylesheet.clone();
                sheet.extend(&self.stylesheet);
                let _active = set_app_active(self.app_active);
                let _guard = set_style_context(sheet);
                if self.poll_stylesheet() {
                    dirty = true;
                }
                root.on_tick(tick);
                let mut outcome = dispatch_event(root, Event::Tick(tick));
                self.absorb_outcome(&mut outcome, &mut dirty);
                let mut msg_outcome = dispatch_message_queue(root, outcome.messages);
                self.absorb_outcome(&mut msg_outcome, &mut dirty);
                let notifications_before = self.notifications.len();
                let now = Instant::now();
                self.notifications.retain(|note| note.expires_at > now);
                if self.notifications.len() != notifications_before {
                    dirty = true;
                }
                if outcome.stop_requested || msg_outcome.stop_requested {
                    break 'event_loop;
                }

                let any_active = any_widget_active(root);
                if dirty || self.resized_since_last_render || any_active || prev_any_active {
                    self.render_widget(root)?;
                    dirty = false;
                    last_render = Instant::now();
                }
                prev_any_active = any_active;
                tick += 1;
            }
        }

        root.on_unmount();
        self.finish()?;
        Ok(())
    }

    fn dispatch_binding_hints_changed(&mut self, root: &mut dyn Widget) -> DispatchOutcome {
        let current = self.binding_hints();
        if current == self.last_binding_hints {
            return DispatchOutcome::default();
        }
        self.last_binding_hints = current.clone();
        let outcome = dispatch_event(root, Event::BindingsChanged(current));
        let msg_outcome = dispatch_message_queue(root, outcome.messages);
        DispatchOutcome {
            handled: outcome.handled || msg_outcome.handled,
            repaint_requested: outcome.repaint_requested || msg_outcome.repaint_requested,
            stop_requested: outcome.stop_requested || msg_outcome.stop_requested,
            messages: msg_outcome.messages,
            animation_requests: {
                let mut requests = outcome.animation_requests;
                requests.extend(msg_outcome.animation_requests);
                requests
            },
        }
    }

    fn enqueue_animation_requests(&mut self, requests: Vec<AnimationRequest>) {
        if requests.is_empty() {
            return;
        }
        self.animator.enqueue_many(requests, Instant::now());
    }

    fn absorb_outcome(&mut self, outcome: &mut DispatchOutcome, dirty: &mut bool) {
        *dirty |= outcome.should_repaint();
        let requests = std::mem::take(&mut outcome.animation_requests);
        self.enqueue_animation_requests(requests);
    }

    fn dispatch_animation_frame(&mut self, root: &mut dyn Widget) -> DispatchOutcome {
        let updates = self.animator.step(Instant::now(), self.animation_level);
        if updates.is_empty() {
            return DispatchOutcome::default();
        }

        let mut aggregate = DispatchOutcome::default();
        for update in updates {
            let mut outcome = dispatch_event_to_target(
                root,
                update.target,
                &Event::AnimationValue(AnimationValueEvent {
                    target: update.target,
                    attribute: update.attribute,
                    value: update.value,
                    done: update.done,
                }),
            );
            self.absorb_outcome(&mut outcome, &mut aggregate.repaint_requested);
            let mut msg_outcome = dispatch_message_queue(root, outcome.messages);
            self.absorb_outcome(&mut msg_outcome, &mut aggregate.repaint_requested);

            aggregate.handled |= outcome.handled || msg_outcome.handled;
            aggregate.stop_requested |= outcome.stop_requested || msg_outcome.stop_requested;
            aggregate.messages.extend(msg_outcome.messages);
        }
        aggregate.repaint_requested = true;
        aggregate
    }

    fn print_segments(&mut self, diff: &rich_rs::Segments) -> Result<()> {
        // Some terminals may silently reset runtime modes (including line wrap)
        // during aggressive resize bursts. Reassert before every frame write.
        let _ = self.driver.reassert_runtime_modes();
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

        let hovered_changed = hovered != self.hovered;
        if hovered_changed {
            self.hovered = hovered;
            crate::widgets::set_hover_by_id(root, self.hovered);
            let shape = pointer_shape_for_hover(root, self.hovered);
            let _ = self.set_pointer_shape(shape);
        }

        // Forward updated coordinates so widgets can track intra-widget mouse position.
        let mut moved_changed = false;
        if let Some(id) = self.hovered {
            let (lx, ly) = self
                .hit_test
                .content_local_coords(root, id, x as u16, y as u16);
            moved_changed = call_on_mouse_move(root, id, lx, ly);
        }

        hovered_changed || moved_changed
    }

    fn set_pointer_shape(&mut self, shape: PointerShape) -> Result<()> {
        if shape == self.pointer_shape {
            return Ok(());
        }
        self.pointer_shape = shape;
        if self.driver.options().enable_pointer_shapes {
            // Write via `Console` so it shares the same output writer as the render pipeline.
            // This avoids interleaving issues that can cause OSC sequences to be dropped.
            let seq = format!("\x1b]22;{}\x07", shape.as_kitty_name());
            debug_render(&format!("[app] pointer_shape={}", shape.as_kitty_name()));
            self.console.write_str(&seq)?;
        }
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
                "[app] resize: burst={} dt={}ms frame {}x{} -> {}x{} (reset framebuffer to blanks; clear on next render)",
                self.resize_burst,
                dt_ms,
                self.frame.width,
                self.frame.height,
                size.width,
                size.height
            ));
            if let Err(error) = self.driver.reassert_runtime_modes() {
                debug_render(&format!("[app] resize: mode reassert failed: {error}"));
            }
            self.frame = FrameBuffer::new(size.width as usize, size.height as usize, None);
            self.resized_since_last_render = true;
            self.clear_on_next_render = true;
        }
        Ok(())
    }

    fn poll_stylesheet(&mut self) -> bool {
        let Some(watch) = &mut self.stylesheet_watch else {
            return false;
        };
        if watch.last_checked.elapsed() < watch.interval {
            return false;
        }
        watch.last_checked = Instant::now();
        let Ok(meta) = fs::metadata(&watch.path) else {
            return false;
        };
        let Ok(modified) = meta.modified() else {
            return false;
        };
        let changed = watch
            .last_modified
            .map(|prev| modified > prev)
            .unwrap_or(true);
        if !changed {
            return false;
        }
        if let Ok(css) = fs::read_to_string(&watch.path) {
            self.stylesheet = StyleSheet::parse(&css);
            watch.last_modified = Some(modified);
            return true;
        }
        false
    }
}

fn call_on_mouse_move(root: &mut dyn Widget, target: WidgetId, x: u16, y: u16) -> bool {
    fn visit(w: &mut dyn Widget, id: WidgetId, x: u16, y: u16, out: &mut Option<bool>) {
        if out.is_some() {
            return;
        }
        if w.id() == id {
            *out = Some(w.on_mouse_move(x, y));
            return;
        }
        w.visit_children_mut(&mut |child| visit(child, id, x, y, out));
    }

    let mut out: Option<bool> = None;
    visit(root, target, x, y, &mut out);
    out.unwrap_or(false)
}

fn any_widget_active(root: &mut dyn Widget) -> bool {
    fn visit(w: &mut dyn Widget, out: &mut bool) {
        if *out {
            return;
        }
        if w.is_active() {
            *out = true;
            return;
        }
        w.visit_children_mut(&mut |child| visit(child, out));
    }

    let mut out = false;
    visit(root, &mut out);
    out
}

fn pointer_shape_for_hover(root: &mut dyn Widget, hovered: Option<WidgetId>) -> PointerShape {
    let Some(id) = hovered else {
        return PointerShape::Default;
    };

    // Traverse the widget tree to locate the hovered widget.
    let mut found: Option<(bool, bool, &'static str)> = None; // (mouse_interactive, disabled, type)
    fn visit(w: &mut dyn Widget, id: WidgetId, out: &mut Option<(bool, bool, &'static str)>) {
        if out.is_some() {
            return;
        }
        if w.id() == id {
            *out = Some((w.mouse_interactive(), w.is_disabled(), w.style_type()));
            return;
        }
        w.visit_children_mut(&mut |child| visit(child, id, out));
    }

    visit(root, id, &mut found);

    let Some((mouse_interactive, disabled, ty)) = found else {
        return PointerShape::Default;
    };

    if !mouse_interactive {
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

fn prepend_clear_if_needed(diff: Segments, clear_before_draw: bool) -> Segments {
    if !clear_before_draw {
        return diff;
    }
    let mut out = Segments::new();
    out.push(Segment::control(ControlType::Clear));
    out.extend(diff);
    out
}

#[derive(Debug, Default)]
struct SegmentStreamStats {
    controls: usize,
    home: usize,
    clear: usize,
    carriage_return: usize,
    cursor_moves: usize,
    move_to: usize,
    text_segments: usize,
    text_bytes: usize,
    newline_text: usize,
    touch_last_col: usize,
    overflow_right: usize,
    max_cursor_x: usize,
    max_cursor_y: usize,
}

fn analyze_segment_stream(segments: &Segments, width: usize) -> SegmentStreamStats {
    let mut stats = SegmentStreamStats::default();
    let mut cursor_x = 0usize;
    let mut cursor_y = 0usize;

    for segment in segments.iter() {
        if let Some(control) = segment.control.as_ref() {
            stats.controls += 1;
            match control {
                ControlType::Home => {
                    stats.home += 1;
                    cursor_x = 0;
                    cursor_y = 0;
                }
                ControlType::Clear => {
                    stats.clear += 1;
                    cursor_x = 0;
                    cursor_y = 0;
                }
                ControlType::CarriageReturn => {
                    stats.carriage_return += 1;
                    cursor_x = 0;
                }
                ControlType::CursorUp(n) => {
                    stats.cursor_moves += 1;
                    cursor_y = cursor_y.saturating_sub(*n as usize);
                }
                ControlType::CursorDown(n) => {
                    stats.cursor_moves += 1;
                    cursor_y = cursor_y.saturating_add(*n as usize);
                }
                ControlType::CursorForward(n) => {
                    stats.cursor_moves += 1;
                    cursor_x = cursor_x.saturating_add(*n as usize);
                }
                ControlType::CursorBackward(n) => {
                    stats.cursor_moves += 1;
                    cursor_x = cursor_x.saturating_sub(*n as usize);
                }
                ControlType::MoveTo { x, y } => {
                    stats.move_to += 1;
                    cursor_x = *x as usize;
                    cursor_y = *y as usize;
                }
                _ => {}
            }
            stats.max_cursor_x = stats.max_cursor_x.max(cursor_x);
            stats.max_cursor_y = stats.max_cursor_y.max(cursor_y);
            continue;
        }

        if segment.text.is_empty() {
            continue;
        }

        stats.text_segments += 1;
        stats.text_bytes += segment.text.len();
        let newline_count = segment.text.as_ref().matches('\n').count();
        stats.newline_text += newline_count;

        let text_width = rich_rs::cell_len(segment.text.as_ref());
        if width > 0 && text_width > 0 {
            let end_x = cursor_x.saturating_add(text_width - 1);
            if end_x == width - 1 {
                stats.touch_last_col += 1;
            }
            if end_x >= width {
                stats.overflow_right += 1;
            }
        }
        cursor_x = cursor_x.saturating_add(text_width);
        stats.max_cursor_x = stats.max_cursor_x.max(cursor_x);
        stats.max_cursor_y = stats.max_cursor_y.max(cursor_y);
    }

    stats
}

fn control_head(segments: &Segments, limit: usize) -> String {
    let mut labels: Vec<String> = Vec::new();
    for segment in segments.iter() {
        let Some(control) = segment.control.as_ref() else {
            continue;
        };
        let label = match control {
            ControlType::Home => "Home".to_string(),
            ControlType::Clear => "Clear".to_string(),
            ControlType::CarriageReturn => "CR".to_string(),
            ControlType::CursorUp(n) => format!("Up({n})"),
            ControlType::CursorDown(n) => format!("Down({n})"),
            ControlType::CursorForward(n) => format!("Right({n})"),
            ControlType::CursorBackward(n) => format!("Left({n})"),
            ControlType::MoveTo { x, y } => format!("MoveTo({x},{y})"),
            ControlType::EraseInLine(mode) => format!("EraseInLine({mode})"),
            ControlType::ShowCursor => "ShowCursor".to_string(),
            ControlType::HideCursor => "HideCursor".to_string(),
            _ => format!("{control:?}"),
        };
        labels.push(label);
        if labels.len() >= limit {
            break;
        }
    }
    labels.join(", ")
}

fn resize_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("TEXTUAL_DEBUG_RESIZE_TRACE")
            .ok()
            .map(|value| {
                let normalized = value.trim().to_ascii_lowercase();
                !(normalized.is_empty()
                    || normalized == "0"
                    || normalized == "false"
                    || normalized == "off"
                    || normalized == "no")
            })
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

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

    #[test]
    fn prepend_clear_only_when_requested() {
        let mut diff = Segments::new();
        diff.push(Segment::control(ControlType::Home));
        diff.push(Segment::new("x"));

        let without_clear = prepend_clear_if_needed(diff.clone(), false);
        let with_clear = prepend_clear_if_needed(diff.clone(), true);

        assert_eq!(without_clear.len(), diff.len());
        assert_eq!(with_clear.len(), diff.len() + 1);
        assert!(matches!(
            without_clear
                .iter()
                .next()
                .and_then(|seg| seg.control.as_ref()),
            Some(ControlType::Home)
        ));
        assert!(matches!(
            with_clear
                .iter()
                .next()
                .and_then(|seg| seg.control.as_ref()),
            Some(ControlType::Clear)
        ));
    }

    #[test]
    fn hit_test_translates_screen_to_widget_local_coords() {
        use crate::widgets::{AppRoot, DataTable, Panel, WidgetRenderable};

        let console = rich_rs::Console::new();
        let mut options = console.options().clone();
        options.size = (20, 6);
        options.max_width = 20;
        options.max_height = 6;

        let table = DataTable::new(
            vec!["A".into(), "B".into()],
            vec![
                vec!["r0".into(), "c0".into()],
                vec!["r1".into(), "c1".into()],
            ],
        );
        let table_id = table.id();
        let panel = Panel::new(table);
        let mut root = AppRoot::new().with_child(panel);

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);
        let renderable = WidgetRenderable::new(&root);
        let buf = FrameBuffer::from_renderable(&console, &options, &renderable, None);

        let hit_test = HitTestMap::from_frame(&buf);
        let rect = hit_test.rect(table_id).expect("table bounds missing");
        assert!(
            rect.x0 > 0 || rect.y0 > 0,
            "table should not start at origin"
        );

        let (lx, ly) = hit_test.content_local_coords(&mut root, table_id, rect.x0, rect.y0);
        assert_eq!((lx, ly), (0, 0));
    }

    #[test]
    fn shift_wheel_maps_vertical_to_horizontal() {
        assert_eq!(
            mouse_scroll_deltas(MouseEventKind::ScrollUp, KeyModifiers::SHIFT),
            (-1, 0)
        );
        assert_eq!(
            mouse_scroll_deltas(MouseEventKind::ScrollDown, KeyModifiers::SHIFT),
            (1, 0)
        );
        assert_eq!(
            mouse_scroll_deltas(MouseEventKind::ScrollLeft, KeyModifiers::SHIFT),
            (-1, 0)
        );
        assert_eq!(
            mouse_scroll_deltas(MouseEventKind::ScrollRight, KeyModifiers::SHIFT),
            (1, 0)
        );
        assert_eq!(
            mouse_scroll_deltas(MouseEventKind::ScrollDown, KeyModifiers::empty()),
            (0, 1)
        );
    }

    #[test]
    fn quit_key_matches_defaults() {
        let quit_keys = vec![KeyBind::new(KeyCode::Char('q'), KeyModifiers::CONTROL)];
        let ctrl_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
        let q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty());
        let x = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::empty());

        assert!(should_quit_key(&ctrl_q, &quit_keys));
        assert!(!should_quit_key(&q, &quit_keys));
        assert!(!should_quit_key(&x, &quit_keys));
    }

    #[test]
    fn quit_key_can_require_modifiers() {
        let quit_keys = vec![KeyBind::new(KeyCode::Char('q'), KeyModifiers::CONTROL)];
        let ctrl_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
        let plain_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty());

        assert!(should_quit_key(&ctrl_q, &quit_keys));
        assert!(!should_quit_key(&plain_q, &quit_keys));
    }

    #[test]
    fn default_action_map_binds_ctrl_c_to_help_quit() {
        let map = default_action_map();
        let ctrl_c = KeyBind::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(map.lookup(&ctrl_c), Some(Action::HelpQuit));
    }
}

fn apply_size(options: &mut ConsoleOptions, size: Size) {
    let width = size.width as usize;
    let height = size.height as usize;
    options.size = (width, height);
    options.max_width = width;
    options.max_height = height;
}

fn mouse_scroll_deltas(kind: MouseEventKind, modifiers: KeyModifiers) -> (i32, i32) {
    let (mut delta_x, mut delta_y) = match kind {
        MouseEventKind::ScrollUp => (0, -1),
        MouseEventKind::ScrollDown => (0, 1),
        MouseEventKind::ScrollLeft => (-1, 0),
        MouseEventKind::ScrollRight => (1, 0),
        _ => (0, 0),
    };

    // Common TUI convention: Shift + vertical wheel scrolls horizontally.
    if modifiers.contains(KeyModifiers::SHIFT) && delta_x == 0 && delta_y != 0 {
        delta_x = delta_y;
        delta_y = 0;
    }

    (delta_x, delta_y)
}

fn should_quit_key(key: &crossterm::event::KeyEvent, quit_keys: &[KeyBind]) -> bool {
    let bind = KeyBind::new(key.code, key.modifiers);
    quit_keys.iter().any(|candidate| *candidate == bind)
}

fn default_action_map() -> ActionMap {
    let mut map = ActionMap::new();
    map.bind(
        KeyBind::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        Action::HelpQuit,
    );
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
    map.bind(
        KeyBind::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
        Action::CommandPalette,
    );
    map
}

#[derive(Debug, Clone, Default)]
struct DispatchOutcome {
    handled: bool,
    repaint_requested: bool,
    stop_requested: bool,
    messages: Vec<MessageEvent>,
    animation_requests: Vec<AnimationRequest>,
}

impl DispatchOutcome {
    fn should_repaint(&self) -> bool {
        self.handled || self.repaint_requested
    }
}

fn dispatch_event(root: &mut dyn Widget, event: Event) -> DispatchOutcome {
    let event_debug = format!("{event:?}");
    let mut ctx = EventCtx::default();
    let always_bubble = matches!(&event, Event::MouseUp(..));
    root.on_event_capture(&event, &mut ctx);
    if always_bubble || !ctx.handled() {
        root.on_event(&event, &mut ctx);
    }
    let outcome = DispatchOutcome {
        handled: ctx.handled(),
        repaint_requested: ctx.repaint_requested(),
        stop_requested: ctx.stop_requested(),
        messages: ctx.take_messages(),
        animation_requests: ctx.take_animation_requests(),
    };
    debug_message(&format!(
        "[dispatch_event] event={event_debug} handled={} repaint={} messages={}",
        outcome.handled,
        outcome.repaint_requested,
        outcome.messages.len()
    ));
    outcome
}

fn is_scroll_action(action: Action) -> bool {
    matches!(
        action,
        Action::ScrollUp
            | Action::ScrollDown
            | Action::ScrollPageUp
            | Action::ScrollPageDown
            | Action::ScrollLeft
            | Action::ScrollRight
            | Action::ScrollPageLeft
            | Action::ScrollPageRight
    )
}

fn is_priority_action(action: Action) -> bool {
    matches!(action, Action::CommandPalette)
}

fn focused_widget_id(root: &mut dyn Widget) -> Option<WidgetId> {
    fn visit(widget: &mut dyn Widget, out: &mut Option<WidgetId>) {
        if out.is_some() {
            return;
        }
        if widget.has_focus() {
            *out = Some(widget.id());
            return;
        }
        widget.visit_children_mut(&mut |child| visit(child, out));
    }

    let mut out = None;
    visit(root, &mut out);
    out
}

fn dispatch_event_to_target(
    root: &mut dyn Widget,
    target: WidgetId,
    event: &Event,
) -> DispatchOutcome {
    let mut ctx = EventCtx::default();
    root.on_event_capture(event, &mut ctx);
    if !ctx.handled() {
        let found = dispatch_event_bubble(root, target, event, &mut ctx);
        if !found {
            root.on_event(event, &mut ctx);
        }
    }
    let handled = ctx.handled();
    let repaint_requested = ctx.repaint_requested();
    let messages = ctx.take_messages();
    let animation_requests = ctx.take_animation_requests();
    debug_message(&format!(
        "[dispatch_event_to_target] target={} event={event:?} handled={} repaint={} messages={}",
        target.as_u64(),
        handled,
        repaint_requested,
        messages.len()
    ));
    DispatchOutcome {
        handled,
        repaint_requested,
        stop_requested: ctx.stop_requested(),
        messages,
        animation_requests,
    }
}

fn dispatch_event_bubble(
    widget: &mut dyn Widget,
    target: WidgetId,
    event: &Event,
    ctx: &mut EventCtx,
) -> bool {
    if widget.id() == target {
        widget.on_event(event, ctx);
        return true;
    }

    let mut found_in_child = false;
    widget.visit_children_mut(&mut |child| {
        if found_in_child {
            return;
        }
        found_in_child = dispatch_event_bubble(child, target, event, ctx);
    });

    if found_in_child && !ctx.handled() {
        widget.on_event(event, ctx);
    }

    found_in_child
}

fn dispatch_scroll_action(
    root: &mut dyn Widget,
    action: Action,
    hovered: Option<WidgetId>,
) -> DispatchOutcome {
    let event = Event::Action(action);
    let focused = focused_widget_id(root);

    if let Some(target) = focused {
        let outcome = dispatch_event_to_target(root, target, &event);
        if outcome.handled || outcome.repaint_requested || !outcome.messages.is_empty() {
            return outcome;
        }
    }

    if let Some(target) = hovered.filter(|id| Some(*id) != focused) {
        let outcome = dispatch_event_to_target(root, target, &event);
        if outcome.handled || outcome.repaint_requested || !outcome.messages.is_empty() {
            return outcome;
        }
    }

    dispatch_event(root, event)
}

fn dispatch_mouse_scroll(root: &mut dyn Widget, delta_x: i32, delta_y: i32) -> DispatchOutcome {
    let mut ctx = EventCtx::default();
    root.on_mouse_scroll(delta_x, delta_y, &mut ctx);
    DispatchOutcome {
        handled: ctx.handled(),
        repaint_requested: ctx.repaint_requested(),
        stop_requested: ctx.stop_requested(),
        messages: ctx.take_messages(),
        animation_requests: ctx.take_animation_requests(),
    }
}

fn dispatch_mouse_scroll_to_target(
    root: &mut dyn Widget,
    target: WidgetId,
    delta_x: i32,
    delta_y: i32,
) -> DispatchOutcome {
    let mut ctx = EventCtx::default();
    let found = dispatch_mouse_scroll_bubble(root, target, delta_x, delta_y, &mut ctx);
    if !found {
        root.on_mouse_scroll(delta_x, delta_y, &mut ctx);
    }
    let handled = ctx.handled();
    let repaint_requested = ctx.repaint_requested();
    let messages = ctx.take_messages();
    let animation_requests = ctx.take_animation_requests();
    debug_message(&format!(
        "[dispatch_mouse_scroll] target={} found={} dx={} dy={} handled={} repaint={} messages={}",
        target.as_u64(),
        found,
        delta_x,
        delta_y,
        handled,
        repaint_requested,
        messages.len()
    ));
    DispatchOutcome {
        handled,
        repaint_requested,
        stop_requested: ctx.stop_requested(),
        messages,
        animation_requests,
    }
}

fn dispatch_mouse_scroll_bubble(
    widget: &mut dyn Widget,
    target: WidgetId,
    delta_x: i32,
    delta_y: i32,
    ctx: &mut EventCtx,
) -> bool {
    if widget.id() == target {
        widget.on_mouse_scroll(delta_x, delta_y, ctx);
        return true;
    }

    let mut found_in_child = false;
    widget.visit_children_mut(&mut |child| {
        if found_in_child {
            return;
        }
        found_in_child = dispatch_mouse_scroll_bubble(child, target, delta_x, delta_y, ctx);
    });

    if found_in_child && !ctx.handled() {
        widget.on_mouse_scroll(delta_x, delta_y, ctx);
    }

    found_in_child
}

fn dispatch_message_queue(root: &mut dyn Widget, initial: Vec<MessageEvent>) -> DispatchOutcome {
    use std::collections::VecDeque;

    let mut handled = false;
    let mut repaint_requested = false;
    let mut stop_requested = false;
    let mut queue: VecDeque<MessageEvent> = initial.into();
    let mut emitted: Vec<MessageEvent> = Vec::new();
    let mut animation_requests: Vec<AnimationRequest> = Vec::new();
    debug_message(&format!(
        "[dispatch_message_queue] start initial={}",
        queue.len()
    ));

    // Prevent message storms from hanging the runtime.
    const LIMIT: usize = 1024;
    let mut processed = 0usize;

    while let Some(message) = queue.pop_front() {
        processed += 1;
        if processed > LIMIT {
            debug_message("[dispatch_message_queue] limit reached, dropping remaining messages");
            break;
        }

        debug_message(&format!(
            "[dispatch_message_queue] pop idx={} sender={} payload={:?}",
            processed,
            message.sender.as_u64(),
            message.message
        ));
        let mut ctx = EventCtx::default();
        dispatch_message_tree(root, &message, &mut ctx);
        handled |= ctx.handled();

        repaint_requested |= ctx.repaint_requested();
        stop_requested |= ctx.stop_requested();
        let next = ctx.take_messages();
        let mut next_animation_requests = ctx.take_animation_requests();
        debug_message(&format!(
            "[dispatch_message_queue] delivered idx={} handled={} repaint={} emitted_now={}",
            processed,
            ctx.handled(),
            ctx.repaint_requested(),
            next.len()
        ));
        if !next.is_empty() {
            queue.extend(next.clone());
            emitted.extend(next);
        }
        if !next_animation_requests.is_empty() {
            animation_requests.append(&mut next_animation_requests);
        }
    }

    let outcome = DispatchOutcome {
        handled,
        repaint_requested,
        stop_requested,
        messages: emitted,
        animation_requests,
    };
    debug_message(&format!(
        "[dispatch_message_queue] end handled={} repaint={} emitted_total={} processed={}",
        outcome.handled,
        outcome.repaint_requested,
        outcome.messages.len(),
        processed
    ));
    outcome
}

fn dispatch_message_tree(root: &mut dyn Widget, message: &MessageEvent, ctx: &mut EventCtx) {
    debug_message(&format!(
        "[dispatch_message_tree] visit widget={}#{} sender={} payload={:?}",
        root.style_type(),
        root.id().as_u64(),
        message.sender.as_u64(),
        message.message
    ));
    root.on_message(message, ctx);
    if ctx.handled() {
        debug_message(&format!(
            "[dispatch_message_tree] handled by {}#{}",
            root.style_type(),
            root.id().as_u64()
        ));
        return;
    }
    root.visit_children_mut(&mut |child| {
        if ctx.handled() {
            return;
        }
        dispatch_message_tree(child, message, ctx);
    });
}

#[cfg(test)]
mod message_tests {
    use super::*;
    use crate::event::{MouseDownEvent, MouseUpEvent};
    use crate::message::Message;
    use crate::widgets::{AppRoot, Button, ScrollView};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct Child {
        id: WidgetId,
    }

    impl Child {
        fn new() -> Self {
            Self {
                id: WidgetId::new(),
            }
        }
    }

    impl Widget for Child {
        fn id(&self) -> WidgetId {
            self.id
        }

        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> rich_rs::Segments {
            rich_rs::Segments::new()
        }

        fn focusable(&self) -> bool {
            true
        }

        fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
            if let Event::Key(key) = event {
                if matches!(key.code, KeyCode::Char('x')) {
                    ctx.post_message(
                        self.id,
                        Message::InputChanged {
                            value: "ok".into(),
                            validation: crate::validation::ValidationResult::success(),
                        },
                    );
                    ctx.set_handled();
                }
            }
        }
    }

    struct Parent {
        id: WidgetId,
        child: Box<dyn Widget>,
        seen: usize,
    }

    impl Parent {
        fn new(child: impl Widget + 'static) -> Self {
            Self {
                id: WidgetId::new(),
                child: Box::new(child),
                seen: 0,
            }
        }
    }

    impl Widget for Parent {
        fn id(&self) -> WidgetId {
            self.id
        }

        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> rich_rs::Segments {
            rich_rs::Segments::new()
        }

        fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
            self.child.on_event_capture(event, ctx);
        }

        fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
            self.child.on_event(event, ctx);
        }

        fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
            if matches!(message.message, Message::InputChanged { .. }) {
                self.seen += 1;
                ctx.set_handled();
            }
        }

        fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
            f(self.child.as_mut());
        }
    }

    #[test]
    fn messages_bubble_to_ancestor_handlers() {
        let mut root = Parent::new(Child::new());
        let key = KeyEventData::from_crossterm(crossterm::event::KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::empty(),
        ));
        let outcome = dispatch_event(&mut root, Event::Key(key));
        assert_eq!(outcome.messages.len(), 1);

        let msg_outcome = dispatch_message_queue(&mut root, outcome.messages);
        assert!(msg_outcome.handled);
        assert_eq!(root.seen, 1);
    }

    struct Receiver {
        id: WidgetId,
        child: Box<dyn Widget>,
        seen: usize,
    }

    impl Receiver {
        fn new(child: impl Widget + 'static) -> Self {
            Self {
                id: WidgetId::new(),
                child: Box::new(child),
                seen: 0,
            }
        }
    }

    impl Widget for Receiver {
        fn id(&self) -> WidgetId {
            self.id
        }
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> rich_rs::Segments {
            rich_rs::Segments::new()
        }
        fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
            self.child.on_event_capture(event, ctx);
        }
        fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
            self.child.on_event(event, ctx);
        }
        fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
            if matches!(message.message, Message::ButtonPressed { .. }) {
                self.seen += 1;
                ctx.set_handled();
            }
        }
        fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
            f(self.child.as_mut());
        }
    }

    #[test]
    fn button_pressed_message_reaches_ancestor() {
        let button = Button::new("x");
        let button_id = button.id();
        let mut root = AppRoot::new().with_child(Receiver::new(button));

        let down = dispatch_event(
            &mut root,
            Event::MouseDown(MouseDownEvent {
                target: button_id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
        );
        let _ = dispatch_message_queue(&mut root, down.messages);

        let up = dispatch_event(
            &mut root,
            Event::MouseUp(MouseUpEvent {
                target: Some(button_id),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
        );
        assert!(!up.messages.is_empty());
        let routed = dispatch_message_queue(&mut root, up.messages);
        assert!(routed.handled);
    }

    #[test]
    fn button_pressed_message_survives_scrollview_forwarding() {
        let button = Button::new("x");
        let button_id = button.id();
        let scroll = ScrollView::new(button);
        let mut root = AppRoot::new().with_child(Receiver::new(scroll));

        let down = dispatch_event(
            &mut root,
            Event::MouseDown(MouseDownEvent {
                target: button_id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
        );
        let _ = dispatch_message_queue(&mut root, down.messages);

        let up = dispatch_event(
            &mut root,
            Event::MouseUp(MouseUpEvent {
                target: Some(button_id),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
        );
        assert_eq!(up.messages.len(), 1);
        let routed = dispatch_message_queue(&mut root, up.messages);
        assert!(routed.handled);
    }

    struct ScrollReceiver {
        id: WidgetId,
        child: Box<dyn Widget>,
        seen: usize,
    }

    impl ScrollReceiver {
        fn new(child: impl Widget + 'static) -> Self {
            Self {
                id: WidgetId::new(),
                child: Box::new(child),
                seen: 0,
            }
        }
    }

    impl Widget for ScrollReceiver {
        fn id(&self) -> WidgetId {
            self.id
        }
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> rich_rs::Segments {
            rich_rs::Segments::new()
        }
        fn on_mouse_scroll(&mut self, _delta_x: i32, _delta_y: i32, ctx: &mut EventCtx) {
            self.seen += 1;
            ctx.set_handled();
        }
        fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
            f(self.child.as_mut());
        }
    }

    #[test]
    fn mouse_scroll_bubbles_to_ancestor_handlers() {
        let button = Button::new("x");
        let button_id = button.id();
        let mut root = ScrollReceiver::new(button);

        let outcome = dispatch_mouse_scroll_to_target(&mut root, button_id, 0, 1);
        assert!(outcome.handled);
        assert_eq!(root.seen, 1);
    }

    struct ScrollSink {
        id: WidgetId,
        focused: bool,
        hits: Arc<AtomicUsize>,
    }

    impl ScrollSink {
        fn new(focused: bool, hits: Arc<AtomicUsize>) -> Self {
            Self {
                id: WidgetId::new(),
                focused,
                hits,
            }
        }
    }

    impl Widget for ScrollSink {
        fn id(&self) -> WidgetId {
            self.id
        }

        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> rich_rs::Segments {
            rich_rs::Segments::new()
        }

        fn focusable(&self) -> bool {
            true
        }

        fn set_focus(&mut self, focused: bool) {
            self.focused = focused;
        }

        fn has_focus(&self) -> bool {
            self.focused
        }

        fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
            if matches!(event, Event::Action(Action::ScrollDown)) {
                self.hits.fetch_add(1, Ordering::Relaxed);
                ctx.set_handled();
            }
        }
    }

    #[test]
    fn scroll_actions_prefer_focused_target() {
        let first_hits = Arc::new(AtomicUsize::new(0));
        let second_hits = Arc::new(AtomicUsize::new(0));

        let first = ScrollSink::new(false, first_hits.clone());
        let second = ScrollSink::new(true, second_hits.clone());
        let mut root = AppRoot::new().with_child(first).with_child(second);

        let outcome = dispatch_scroll_action(&mut root, Action::ScrollDown, None);
        assert!(outcome.handled);
        assert_eq!(first_hits.load(Ordering::Relaxed), 0);
        assert_eq!(second_hits.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn scroll_actions_fallback_to_hovered_when_unfocused() {
        let first_hits = Arc::new(AtomicUsize::new(0));
        let second_hits = Arc::new(AtomicUsize::new(0));

        let first = ScrollSink::new(false, first_hits.clone());
        let second = ScrollSink::new(false, second_hits.clone());
        let second_id = second.id();
        let mut root = AppRoot::new().with_child(first).with_child(second);

        let outcome = dispatch_scroll_action(&mut root, Action::ScrollDown, Some(second_id));
        assert!(outcome.handled);
        assert_eq!(first_hits.load(Ordering::Relaxed), 0);
        assert_eq!(second_hits.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn scroll_actions_fallback_to_global_when_no_target_handles() {
        let first_hits = Arc::new(AtomicUsize::new(0));
        let second_hits = Arc::new(AtomicUsize::new(0));

        let first = ScrollSink::new(false, first_hits.clone());
        let second = ScrollSink::new(false, second_hits.clone());
        let mut root = AppRoot::new().with_child(first).with_child(second);

        let outcome = dispatch_scroll_action(&mut root, Action::ScrollDown, None);
        assert!(outcome.handled);
        assert_eq!(first_hits.load(Ordering::Relaxed), 1);
        assert_eq!(second_hits.load(Ordering::Relaxed), 0);
    }
}
