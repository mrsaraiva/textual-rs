mod devtools;
mod event_loop;
mod helpers;
mod render;
mod routing;
mod tasks;
mod timers;
mod types;

use crate::animation::{Animator, animation_level_from_env};
use crate::compose::{ChildDecl, WidgetBuilder};
use crate::css::{StyleSheet, default_widget_stylesheet};
use crate::debug::{DebugLayout, debug_render};
use crate::driver::{DriverOptions, KeyboardProtocol, PointerShape, TerminalDriver};
use crate::event::{ActionMap, BindingHint, KeyBind};
use crate::message::MessageEvent;
use crate::node_id::NodeId;
use crate::node_id::node_id_from_ffi;
use crate::render::FrameBuffer;
use crate::screen::ScreenStack;
use crate::style::Theme;
use crate::widget_tree::WidgetTree;
use crate::widgets::{ToastSeverity, Widget};
use crate::{Error, Result};
use crossterm::event::{KeyCode, KeyModifiers};
use rich_rs::{Console, ConsoleOptions, MetaValue};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use tasks::AsyncTaskRuntime;
use timers::OneShotTimerRuntime;
use types::{
    AppNotification, BindingHintEntry, DEFAULT_NOTIFICATION_TIMEOUT, HitTestMap, StylesheetReload,
    StylesheetWatcher,
};

use helpers::{ClickTracker, apply_size, default_action_map};

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
    hovered: Option<NodeId>,
    click_tracker: ClickTracker,
    last_render_at: Instant,
    resized_since_last_render: bool,
    clear_on_next_render: bool,
    last_resize_at: Option<Instant>,
    resize_burst: u64,
    sync_output: bool,
    pointer_shape: PointerShape,
    app_active: bool,
    last_binding_hints: Vec<BindingHint>,
    last_binding_hint_sources: Vec<NodeId>,
    last_focused_help_source: Option<NodeId>,
    last_focused_help_markup: Option<String>,
    animator: Animator,
    animation_level: crate::event::AnimationLevel,
    notifications: Vec<AppNotification>,
    clipboard: Option<String>,
    async_tasks: AsyncTaskRuntime,
    one_shot_timers: OneShotTimerRuntime,
    devtools: Option<devtools::DevtoolsRuntime>,
    /// Arena-based widget tree built from `compose()` declarations.
    ///
    /// `None` until `build_widget_tree()` is called during app startup.
    /// When present, the runtime uses tree-based event dispatch and focus
    /// management instead of the legacy recursive `visit_children_mut` paths.
    widget_tree: Option<WidgetTree>,
    /// Stack of screens. Each screen owns an independent widget tree and
    /// optional stylesheet. The topmost screen is the active one.
    screen_stack: ScreenStack,
    /// Mode registry: maps mode names to screen factory functions.
    ///
    /// When `switch_mode()` is called, the current mode screen (if any) is
    /// popped and a new screen is created from the named factory. This follows
    /// the Python Textual MODES pattern.
    modes: HashMap<String, Box<dyn Fn() -> Box<dyn crate::screen::Screen> + Send + Sync>>,
    /// The name of the currently active mode, if any.
    current_mode: Option<String>,
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
            click_tracker: ClickTracker::new(),
            last_render_at: Instant::now(),
            resized_since_last_render: false,
            clear_on_next_render: false,
            last_resize_at: None,
            resize_burst: 0,
            sync_output,
            pointer_shape: PointerShape::Default,
            app_active: true,
            last_binding_hints: Vec::new(),
            last_binding_hint_sources: Vec::new(),
            last_focused_help_source: None,
            last_focused_help_markup: None,
            animator: Animator::new(60),
            animation_level: animation_level_from_env(),
            notifications: Vec::new(),
            clipboard: None,
            async_tasks: AsyncTaskRuntime::default(),
            one_shot_timers: OneShotTimerRuntime::default(),
            devtools: devtools::DevtoolsRuntime::from_env().ok().flatten(),
            widget_tree: None,
            screen_stack: ScreenStack::new(),
            modes: HashMap::new(),
            current_mode: None,
        };
        Ok(app)
    }

    /// Build the arena-based widget tree by extracting children from the root widget.
    ///
    /// Uses `take_composed_children()` to recursively move children out of
    /// containers and into the arena tree. After building, the tree is stored
    /// in `self.widget_tree` and tree-based dispatch paths become active.
    ///
    /// Also processes `compose()` declarations for any widget that provides them.
    pub(crate) fn build_widget_tree(&mut self, root: &mut dyn Widget) {
        let mut tree = WidgetTree::new();

        // Mount a synthetic root node that mirrors the real root widget.
        // We don't move the root widget into the tree — it stays as the
        // `&mut dyn Widget` parameter. The tree tracks structure only.
        let root_node_id = tree.set_root(Box::new(TreeStubWidget::from_widget(root)));

        // Extract children from root into tree, recursively.
        Self::extract_children_to_tree(&mut tree, root_node_id, root);

        // Also process compose() declarations (if any).
        let declarations = root.compose();
        if !declarations.is_empty() {
            Self::mount_declarations(&mut tree, root_node_id, declarations);
        }

        if tree.len() <= 1 {
            // Only root stub, no children — keep legacy path.
            self.widget_tree = None;
            return;
        }

        // Drain lifecycle events from initial build (mount events) — the
        // runtime will call on_mount separately via the existing path.
        let _ = tree.drain_lifecycle();
        self.widget_tree = Some(tree);
    }

    /// Recursively extract children from a widget via `take_composed_children()`
    /// and mount them into the tree.
    fn extract_children_to_tree(
        tree: &mut WidgetTree,
        parent: NodeId,
        widget: &mut dyn Widget,
    ) {
        let children = widget.take_composed_children();
        for mut child in children {
            // Recursively extract grandchildren before mounting the child.
            // We must do this while we still have &mut access to the child.
            let grandchildren = child.take_composed_children();
            // Also collect compose() declarations from the child.
            let child_compose = child.compose();

            let node_id = tree.mount(parent, child);

            // Recursively mount grandchildren under this node.
            for grandchild in grandchildren {
                Self::mount_extracted_recursive(tree, node_id, grandchild);
            }

            // Mount compose() declarations from the child.
            if !child_compose.is_empty() {
                Self::mount_declarations(tree, node_id, child_compose);
            }
        }
    }

    /// Recursively mount an already-extracted child widget and its descendants.
    fn mount_extracted_recursive(
        tree: &mut WidgetTree,
        parent: NodeId,
        mut widget: Box<dyn Widget>,
    ) {
        let grandchildren = widget.take_composed_children();
        let compose_decls = widget.compose();

        let node_id = tree.mount(parent, widget);

        for grandchild in grandchildren {
            Self::mount_extracted_recursive(tree, node_id, grandchild);
        }

        if !compose_decls.is_empty() {
            Self::mount_declarations(tree, node_id, compose_decls);
        }
    }

    /// Recursively mount `ChildDecl` declarations into the tree under `parent`.
    fn mount_declarations(tree: &mut WidgetTree, parent: NodeId, declarations: Vec<ChildDecl>) {
        for decl in declarations {
            let WidgetBuilder::Ready(mut widget) = decl.builder;
            // Extract children from declared widgets too.
            let extracted = widget.take_composed_children();
            let child_compose = widget.compose();
            let node_id = tree.mount(parent, widget);
            // Apply CSS id/classes from the declaration.
            if let Some(id_str) = &decl.id {
                // CSS id is stored on the widget itself (style_id), not
                // on the tree node. Tree-level classes are separate.
                let _ = id_str; // Reserved for future tree-level id support.
            }
            for class in &decl.classes {
                tree.add_class(node_id, class);
            }
            // Mount extracted children first.
            for child in extracted {
                Self::mount_extracted_recursive(tree, node_id, child);
            }
            // Mount explicit child declarations.
            if !decl.children.is_empty() {
                Self::mount_declarations(tree, node_id, decl.children);
            }
            // Then mount compose() declarations from the widget itself.
            if !child_compose.is_empty() {
                Self::mount_declarations(tree, node_id, child_compose);
            }
        }
    }

    pub(super) fn clipboard_message_sender() -> NodeId {
        Self::runtime_message_sender()
    }

    pub(super) fn runtime_message_sender() -> NodeId {
        // Runtime/system-synthesized messages use node id 0.
        node_id_from_ffi(0)
    }

    pub(super) fn clipboard_message_event(target: NodeId, text: String) -> MessageEvent {
        let sender = Self::clipboard_message_sender();
        MessageEvent {
            sender,
            message: crate::message::Message::TextEditClipboardPaste(
                crate::message::TextEditClipboardPaste { target, text },
            ),
            control: Some(sender),
        }
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
            last_css: css,
            interval: interval.max(Duration::from_millis(50)),
            last_checked: Instant::now(),
        });
        Ok(())
    }

    pub fn bind_key(&mut self, key: KeyBind, action: crate::event::Action) {
        self.action_map.bind(key, action);
    }

    pub fn start(&mut self) -> Result<()> {
        self.last_binding_hints.clear();
        self.last_binding_hint_sources.clear();
        self.last_focused_help_source = None;
        self.last_focused_help_markup = None;
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

    /// Push a screen onto the screen stack.
    ///
    /// Suspends the currently active screen (if any) and mounts the new one.
    pub fn push_screen(&mut self, screen: Box<dyn crate::screen::Screen>) {
        self.screen_stack.push(screen);
    }

    /// Push a screen onto the screen stack with a result callback.
    ///
    /// The callback is invoked with the `ScreenResult` when the screen is
    /// popped (either via `pop_screen()` or via `dismiss_screen()`).
    pub fn push_screen_with_callback(
        &mut self,
        screen: Box<dyn crate::screen::Screen>,
        callback: crate::screen::ScreenResultCallback,
    ) {
        self.screen_stack.push_with_callback(screen, callback);
    }

    /// Dismiss the topmost screen with an optional result value.
    ///
    /// Sets the pending result and then pops the screen. If the screen was
    /// pushed with a callback, the callback is invoked with the result.
    /// If the popped screen was a mode screen, `current_mode` is cleared.
    pub fn dismiss_screen(&mut self, result: crate::screen::ScreenResult) -> bool {
        if self.screen_stack.dismiss(result) {
            if let Some((_screen, _result, mode_name)) = self.screen_stack.pop() {
                if mode_name.is_some() {
                    self.current_mode = None;
                }
            }
            true
        } else {
            false
        }
    }

    /// Pop the topmost screen from the screen stack.
    ///
    /// Unmounts the popped screen and resumes the one below (if any).
    /// If the popped screen was a mode screen, `current_mode` is cleared.
    /// Returns `None` if the stack is empty.
    pub fn pop_screen(&mut self) -> Option<crate::screen::ScreenResult> {
        self.screen_stack.pop().map(|(_screen, result, mode_name)| {
            if mode_name.is_some() {
                self.current_mode = None;
            }
            result
        })
    }

    /// Number of screens on the stack.
    pub fn screen_count(&self) -> usize {
        self.screen_stack.len()
    }

    /// Get the title from the active screen (if it defines one).
    pub fn active_screen_title(&self) -> Option<&str> {
        self.screen_stack.active_title()
    }

    /// Get the sub-title from the active screen (if it defines one).
    pub fn active_screen_sub_title(&self) -> Option<&str> {
        self.screen_stack.active_sub_title()
    }

    // -----------------------------------------------------------------
    // Mode system
    // -----------------------------------------------------------------

    /// Register a named mode with a screen factory function.
    ///
    /// When `switch_mode(name)` is called, the factory creates a new screen
    /// instance that is pushed onto the screen stack. This follows the Python
    /// Textual MODES pattern where each mode maps to a screen factory.
    ///
    /// # Example
    /// ```ignore
    /// app.add_mode("help", || Box::new(HelpScreen::new()));
    /// app.add_mode("settings", || Box::new(SettingsScreen::new()));
    /// ```
    pub fn add_mode(
        &mut self,
        name: impl Into<String>,
        factory: impl Fn() -> Box<dyn crate::screen::Screen> + Send + Sync + 'static,
    ) {
        self.modes.insert(name.into(), Box::new(factory));
    }

    /// Switch to a named mode.
    ///
    /// If there is a current mode screen, it is removed from the screen stack
    /// (even if transient screens are on top of it). Then the factory for
    /// `name` is called to create a new screen, which is pushed as a mode
    /// screen.
    ///
    /// Returns `true` if the mode was switched, `false` if the mode name is
    /// not registered or is already the current mode.
    pub fn switch_mode(&mut self, name: &str) -> bool {
        // No-op if already in the requested mode.
        if self.current_mode.as_deref() == Some(name) {
            return false;
        }

        // Verify the mode is registered.
        let factory = match self.modes.get(name) {
            Some(f) => f,
            None => return false,
        };

        // Create the new screen from the factory before popping the old one,
        // so that if the factory panics the old screen is still intact.
        let new_screen = factory();

        // Remove the current mode screen by its mode tag (safe even if
        // transient screens are on top).
        if let Some(mode) = self.current_mode.take() {
            self.screen_stack.pop_mode(&mode);
        }

        // Push the new mode screen with its mode tag.
        self.screen_stack.push_mode(new_screen, name.to_string());
        self.current_mode = Some(name.to_string());
        true
    }

    /// The name of the currently active mode, if any.
    pub fn current_mode(&self) -> Option<&str> {
        self.current_mode.as_deref()
    }

    /// Returns the list of registered mode names.
    pub fn mode_names(&self) -> Vec<&str> {
        self.modes.keys().map(|s| s.as_str()).collect()
    }

    /// Remove a registered mode by name.
    ///
    /// If the removed mode is the current mode, the mode screen is removed
    /// from the stack (even if transient screens are above it) and
    /// `current_mode` is reset to `None`.
    ///
    /// Returns `true` if the mode existed and was removed.
    pub fn remove_mode(&mut self, name: &str) -> bool {
        if self.modes.remove(name).is_none() {
            return false;
        }
        // If we just removed the active mode, pop its tagged screen.
        if self.current_mode.as_deref() == Some(name) {
            self.screen_stack.pop_mode(name);
            self.current_mode = None;
        }
        true
    }

    /// Run the CSS-layout pass on the arena tree (if present).
    ///
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
                MetaValue::Int(n) if *n >= 0 => Some(node_id_from_ffi(*n as u64)),
                _ => None,
            });

        let hovered_changed = hovered != self.hovered;
        if hovered_changed {
            // Update hover state on actual widgets via the tree.
            if let Some(tree) = self.widget_tree.as_mut() {
                if let Some(old_id) = self.hovered {
                    if let Some(node) = tree.get_mut(old_id) {
                        node.widget.set_hovered(false);
                    }
                }
                if let Some(new_id) = hovered {
                    if let Some(node) = tree.get_mut(new_id) {
                        node.widget.set_hovered(true);
                    }
                }
            }
            self.hovered = hovered;
            let shape = self.pointer_shape_for_hover_auto(root, self.hovered);
            let _ = self.set_pointer_shape(shape);
        }

        // Forward updated coordinates so widgets can track intra-widget mouse position.
        let mut moved_changed = false;
        if let Some(id) = self.hovered {
            let (lx, ly) = self.hit_test.content_local_coords(id, x as u16, y as u16);
            moved_changed = self.call_on_mouse_move_auto(root, id, lx, ly);
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

    fn widget_at(&self, x: u16, y: u16) -> Option<NodeId> {
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
                MetaValue::Int(n) if *n >= 0 => Some(node_id_from_ffi(*n as u64)),
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

    fn poll_stylesheet(&mut self) -> Option<StylesheetReload> {
        let Some(watch) = &mut self.stylesheet_watch else {
            return None;
        };
        if watch.last_checked.elapsed() < watch.interval {
            return None;
        }
        watch.last_checked = Instant::now();
        let Ok(meta) = fs::metadata(&watch.path) else {
            return None;
        };
        let Ok(modified) = meta.modified() else {
            return None;
        };
        let changed = watch
            .last_modified
            .map(|prev| modified > prev)
            .unwrap_or(true);
        if !changed {
            return None;
        }
        let Ok(css) = fs::read_to_string(&watch.path) else {
            return None;
        };
        if css == watch.last_css {
            watch.last_modified = Some(modified);
            return None;
        }
        let previous = self.stylesheet.clone();
        let next = StyleSheet::parse(&css);
        let changed_rules = changed_rules_between(&previous, &next);
        let layout_affected = changed_rules
            .iter()
            .any(|rule| style_affects_layout(&rule.style()));
        self.stylesheet = next.clone();
        watch.last_css = css;
        watch.last_modified = Some(modified);
        Some(StylesheetReload {
            previous,
            next,
            changed_rules,
            layout_affected,
        })
    }
}

fn changed_rules_between(previous: &StyleSheet, next: &StyleSheet) -> Vec<crate::css::StyleRule> {
    let old_rules = previous.rules();
    let new_rules = next.rules();
    let limit = old_rules.len().max(new_rules.len());
    let mut changed = Vec::new();
    for idx in 0..limit {
        let old = old_rules.get(idx);
        let new = new_rules.get(idx);
        if old == new {
            continue;
        }
        if let Some(rule) = old {
            changed.push(rule.clone());
        }
        if let Some(rule) = new {
            changed.push(rule.clone());
        }
    }
    changed
}

fn style_affects_layout(style: &crate::style::Style) -> bool {
    style.margin.is_some()
        || style.padding.is_some()
        || style.border_top != crate::style::BorderEdge::Unset
        || style.border_right != crate::style::BorderEdge::Unset
        || style.border_bottom != crate::style::BorderEdge::Unset
        || style.border_left != crate::style::BorderEdge::Unset
        || style.width.is_some()
        || style.height.is_some()
        || style.min_width.is_some()
        || style.max_width.is_some()
        || style.min_height.is_some()
        || style.max_height.is_some()
        || style.layout.is_some()
        || style.display.is_some()
        || style.visibility.is_some()
        || style.dock.is_some()
}

// ---------------------------------------------------------------------------
// TreeStubWidget — lightweight root-slot placeholder for the arena tree
// ---------------------------------------------------------------------------

/// Minimal widget used as the root slot in the arena-based `WidgetTree`.
///
/// The real root widget is owned by the caller (`run_widget_tree` parameter).
/// This stub mirrors its identity metadata so tree-based dispatch and CSS
/// queries can locate the root position. No rendering, events, or focus
/// are handled — those are forwarded to the real root widget.
struct TreeStubWidget {
    type_name: &'static str,
}

impl TreeStubWidget {
    fn from_widget(w: &dyn Widget) -> Self {
        Self {
            type_name: w.style_type(),
        }
    }
}

impl Widget for TreeStubWidget {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> rich_rs::Segments {
        rich_rs::Segments::new()
    }

    fn style_type(&self) -> &'static str {
        self.type_name
    }
}
