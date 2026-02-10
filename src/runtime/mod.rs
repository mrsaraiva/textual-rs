mod event_loop;
mod helpers;
mod render;
mod routing;
mod types;

use crate::animation::{Animator, animation_level_from_env};
use crate::css::{StyleSheet, default_widget_stylesheet};
use crate::debug::{DebugLayout, debug_render};
use crate::driver::{DriverOptions, KeyboardProtocol, PointerShape, TerminalDriver};
use crate::event::{ActionMap, BindingHint, KeyBind};
use crate::render::FrameBuffer;
use crate::style::Theme;
use crate::widgets::{ToastSeverity, Widget, WidgetId};
use crate::{Error, Result};
use crossterm::event::{KeyCode, KeyModifiers};
use rich_rs::{Console, ConsoleOptions, MetaValue};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use types::{
    AppNotification, BindingHintEntry, DEFAULT_NOTIFICATION_TIMEOUT, HitTestMap, StylesheetWatcher,
};

use helpers::{apply_size, default_action_map};

use helpers::{call_on_mouse_move, pointer_shape_for_hover};

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
        let app = Self {
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
        out.sort_by(|left, right| {
            left.key
                .cmp(&right.key)
                .then_with(|| left.description.cmp(&right.description))
        });
        out.extend(
            self.custom_binding_hints
                .iter()
                .map(|entry| entry.hint.clone()),
        );
        if let Some(entry) = &self.command_palette_hint {
            out.push(entry.hint.clone());
        }

        self.normalize_binding_hints(out)
    }

    pub(super) fn normalize_binding_hints(&self, out: Vec<BindingHint>) -> Vec<BindingHint> {
        let mut unique = BTreeSet::new();
        let mut deduped = Vec::new();
        for entry in out {
            let key = (
                entry.key.clone(),
                entry.description.clone(),
                entry.show,
                entry.key_display.clone(),
                entry.group.clone(),
                entry.priority,
                entry.system,
            );
            if unique.insert(key) {
                deduped.push(entry);
            }
        }

        let mut prioritized = Vec::new();
        let mut regular = Vec::new();
        for entry in deduped {
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
            format!("Press [b]{key}[/b] to quit the app"),
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

    pub fn bind_key(&mut self, key: KeyBind, action: crate::event::Action) {
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
