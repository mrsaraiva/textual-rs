use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rich_rs::{Console, ConsoleOptions, Segments};

use crate::action::{APP_ACTIONS, ActionDecl, ParsedAction};
use crate::demo_snapshot::{SnapshotArgs, snapshot_widget};
use crate::event::{Action, Event, EventCtx};
use crate::keys::KeyEventData;
use crate::message::{CommandPaletteCommand, Message, MessageEvent};
use crate::node_id::NodeId;
use crate::reactive::{ReactiveCtx, ReactiveWidget};
use crate::style::{Position, Scalar};
use crate::validation::ValidationResult;
use crate::widgets::{AppRoot, BindingDecl, CommandPalette, Spacer, Widget};
use crate::{App, Result};

/// Trait-based, Rust-idiomatic app definition for textual-rs.
///
/// This keeps app authoring concise (similar to Python Textual's `App` subclassing model)
/// while preserving explicit composition and runtime APIs.
pub trait TextualApp: Send + 'static {
    /// Build the root widget tree for this app.
    fn compose(&mut self) -> AppRoot;

    /// Optional stylesheet path to auto-load/watch before running.
    fn css_path(&self) -> Option<&'static str> {
        None
    }

    /// Poll interval used when `css_path` is configured.
    fn stylesheet_watch_interval(&self) -> Duration {
        Duration::from_millis(500)
    }

    /// Optional stylesheet path for snapshot rendering.
    ///
    /// Defaults to `css_path()` so demo apps can stay concise.
    fn snapshot_css_path(&self) -> Option<&'static str> {
        self.css_path()
    }

    /// Build widget tree used by snapshot mode.
    ///
    /// Defaults to `compose()`; override if snapshot layout differs from runtime layout.
    fn compose_for_snapshot(&mut self) -> AppRoot {
        self.compose()
    }

    /// Optional runtime configuration hook (key bindings, debug flags, etc.).
    fn configure(&mut self, _app: &mut App) -> Result<()> {
        Ok(())
    }

    /// Declarative app-level key bindings.
    ///
    /// These are attached to the root adapter widget, so they are available
    /// across the focused path similarly to Python Textual's `App.BINDINGS`.
    fn bindings(&self) -> Vec<BindingDecl> {
        Vec::new()
    }

    /// Called after widget mount, before entering the event loop.
    fn on_mount(&mut self) {}

    /// App-level mount hook with mutable runtime handle and event context.
    ///
    /// Called once after the widget tree is fully built and mounted. This is the
    /// correct place to set initial reactive field values so that `init = true`
    /// watchers fire with the widget tree available.
    ///
    /// Mirrors Python Textual's `on_mount` handler timing: all child widgets
    /// already exist and can be reached via `app.query_one()` / `app.query_mut()`.
    fn on_mount_with_app(&mut self, _app: &mut App, _ctx: &mut EventCtx) {}

    /// Return a mutable `ReactiveWidget` reference if this app uses
    /// `#[derive(Reactive)]` on its struct fields.
    ///
    /// Apps that derive `Reactive` should override this to return `Some(self)`:
    /// ```ignore
    /// fn reactive_widget_mut(&mut self) -> Option<&mut dyn textual::reactive::ReactiveWidget> {
    ///     Some(self)
    /// }
    /// ```
    /// The default returns `None`, disabling app-level reactive dispatch.
    fn reactive_widget_mut(&mut self) -> Option<&mut dyn ReactiveWidget> {
        None
    }

    /// App-level action hook. Called after widget dispatch if the event was not handled.
    fn on_action(&mut self, _action: Action, _ctx: &mut EventCtx) {}

    /// App-level action hook with mutable runtime handle.
    ///
    /// This mirrors Python Textual-style app callbacks where action handlers can
    /// query/mutate application state via the runtime handle.
    fn on_action_with_app(&mut self, app: &mut App, action: Action, ctx: &mut EventCtx) {
        let _ = app;
        self.on_action(action, ctx);
    }

    /// App-level key hook.
    ///
    /// Called during capture phase before child widgets, allowing global key
    /// interception akin to Python Textual's app-level key handling.
    fn on_key(&mut self, _key: &KeyEventData, _ctx: &mut EventCtx) {}

    /// App-level key hook with mutable runtime handle.
    ///
    /// This is the Python Textual-aligned surface for app callbacks that need
    /// query/mutation APIs (`query_one`, `query_mut`, etc.) while handling keys.
    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        let _ = app;
        self.on_key(key, ctx);
    }

    /// App-level tick hook.
    ///
    /// Called once per runtime tick after widget `on_tick` and before `Event::Tick`
    /// dispatch.
    fn on_tick(&mut self, _tick: u64, _ctx: &mut EventCtx) {}

    /// App-level tick hook with mutable runtime handle.
    fn on_tick_with_app(&mut self, app: &mut App, tick: u64, ctx: &mut EventCtx) {
        let _ = app;
        self.on_tick(tick, ctx);
    }

    /// App-level message hook. Called after widget message dispatch if not handled.
    fn on_message(&mut self, _message: &MessageEvent, _ctx: &mut EventCtx) {}

    /// App-level message hook with mutable runtime handle.
    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut EventCtx) {
        let _ = app;
        self.on_message(message, ctx);
    }

    /// Typed convenience hook for button-pressed messages.
    fn on_button_pressed(&mut self, _description: &str, _ctx: &mut EventCtx) {}

    /// Typed convenience hook for input-changed messages.
    fn on_input_changed(
        &mut self,
        _value: &str,
        _validation: &ValidationResult,
        _ctx: &mut EventCtx,
    ) {
    }

    /// Typed convenience hook for input-submitted messages.
    fn on_input_submitted(&mut self, _value: &str, _ctx: &mut EventCtx) {}

    /// Typed convenience hook for text-area-changed messages.
    fn on_text_area_changed(&mut self, _value: &str, _ctx: &mut EventCtx) {}

    /// Typed convenience hook for checkbox-changed messages.
    fn on_checkbox_changed(&mut self, _checked: bool, _ctx: &mut EventCtx) {}

    /// Typed convenience hook for list-view-selection-changed messages.
    fn on_list_view_selection_changed(&mut self, _index: usize, _item: &str, _ctx: &mut EventCtx) {}

    /// Typed convenience hook for list-view-activation messages.
    fn on_list_view_item_activated(&mut self, _index: usize, _item: &str, _ctx: &mut EventCtx) {}

    /// Typed convenience hook for tab-activated messages.
    fn on_tab_activated(&mut self, _index: usize, _title: &str, _ctx: &mut EventCtx) {}

    /// Typed convenience hook for command palette open.
    fn on_command_palette_opened(&mut self, _ctx: &mut EventCtx) {}

    /// Typed convenience hook for command palette close.
    fn on_command_palette_closed(&mut self, _ctx: &mut EventCtx) {}

    /// Typed convenience hook for command palette selection.
    fn on_command_palette_command_selected(
        &mut self,
        _id: &str,
        _title: &str,
        _ctx: &mut EventCtx,
    ) {
    }

    /// Provide command-palette providers for this app.
    ///
    /// Providers are started when the command palette opens and shut down when
    /// it closes.
    fn command_palette_providers(&mut self) -> Vec<Box<dyn CommandPaletteProvider>> {
        Vec::new()
    }

    /// Optional app output returned after the runtime exits.
    fn take_exit_output(&mut self) -> Option<String> {
        None
    }
}

/// Command provider lifecycle for TextualApp command palette integration.
pub trait CommandPaletteProvider: Send + Sync {
    /// Called when the command palette opens.
    fn startup(&mut self, _ctx: &mut EventCtx) {}

    /// Return the commands currently provided by this provider.
    fn commands(&mut self) -> Vec<CommandPaletteCommand>;

    /// Called when one of this provider's commands is selected.
    fn on_command_selected(&mut self, _command_id: &str, _ctx: &mut EventCtx) {}

    /// Called when the command palette closes.
    fn shutdown(&mut self, _ctx: &mut EventCtx) {}
}

/// Explicit push/pop helper for apps that model screen-like navigation with overlays.
///
/// This helper does not introduce a separate runtime path: it only posts existing
/// overlay visibility messages on the message bus.
#[derive(Debug, Clone, Default)]
pub struct OverlayScreenStack {
    stack: Vec<NodeId>,
}

impl OverlayScreenStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn current(&self) -> Option<NodeId> {
        self.stack.last().copied()
    }

    pub fn push(&mut self, _sender: NodeId, overlay: NodeId, ctx: &mut EventCtx) -> bool {
        if self.current() == Some(overlay) {
            return false;
        }
        self.stack.retain(|existing| *existing != overlay);
        if let Some(previous) = self.current() {
            ctx.hide_overlay(previous);
        }
        self.stack.push(overlay);
        ctx.show_overlay(overlay);
        true
    }

    pub fn pop(&mut self, _sender: NodeId, ctx: &mut EventCtx) -> Option<NodeId> {
        let removed = self.stack.pop()?;
        ctx.hide_overlay(removed);
        if let Some(previous) = self.current() {
            ctx.show_overlay(previous);
        }
        Some(removed)
    }

    pub fn clear(&mut self, _sender: NodeId, ctx: &mut EventCtx) {
        while let Some(overlay) = self.stack.pop() {
            ctx.hide_overlay(overlay);
        }
    }
}

struct TextualAppAdapter<T: TextualApp> {
    app: Arc<Mutex<T>>,
    app_child: Box<dyn Widget>,
    command_palette_visible: bool,
    help_panel_visible: bool,
    children_extracted: bool,
    command_palette_providers: Vec<Box<dyn CommandPaletteProvider>>,
    command_palette_provider_index: HashMap<String, (usize, String)>,
}

fn build_textual_app_runtime_root<T: TextualApp>(
    state: Arc<Mutex<T>>,
    composed: AppRoot,
) -> TextualAppAdapter<T> {
    let adapter = TextualAppAdapter::new(state, composed);
    adapter
}

impl<T: TextualApp> TextualAppAdapter<T> {
    fn make_command_palette_host() -> CommandPalette {
        let mut command_palette =
            CommandPalette::new(Spacer::new(1)).with_tree_wrapped_child_visible(false);
        if let Some(styles) = command_palette.styles_mut() {
            // Keep command palette always mounted for global app bindings, but
            // out of normal flow so it behaves as a modal overlay.
            styles.style.position = Some(Position::Absolute);
            styles.style.width = Some(Scalar::Percent(100.0));
            styles.style.height = Some(Scalar::Percent(100.0));
        }
        command_palette
    }

    fn new(app: Arc<Mutex<T>>, child: impl Widget + 'static) -> Self {
        Self {
            app,
            app_child: Box::new(child),
            command_palette_visible: false,
            help_panel_visible: false,
            children_extracted: false,
            command_palette_providers: Vec::new(),
            command_palette_provider_index: HashMap::new(),
        }
    }

    fn system_commands(&self) -> Vec<CommandPaletteCommand> {
        let keys_help = if self.help_panel_visible {
            "Hide the keys and widget help panel"
        } else {
            "Show help for the focused widget and a summary of available keys"
        };
        vec![
            CommandPaletteCommand {
                id: "theme".to_string(),
                title: "Theme".to_string(),
                help: "Change the current theme".to_string(),
            },
            CommandPaletteCommand {
                id: "quit".to_string(),
                title: "Quit".to_string(),
                help: "Quit the application as soon as possible".to_string(),
            },
            CommandPaletteCommand {
                id: "keys".to_string(),
                title: "Keys".to_string(),
                help: keys_help.to_string(),
            },
            CommandPaletteCommand {
                id: "screenshot".to_string(),
                title: "Screenshot".to_string(),
                help: "Save an SVG 'screenshot' of the current screen".to_string(),
            },
        ]
    }

    fn sync_help_panel_visible_from_runtime(&mut self, app: &App) -> bool {
        let runtime_visible = app.query("HelpPanel").is_ok_and(|query| !query.is_empty());
        if runtime_visible != self.help_panel_visible {
            self.help_panel_visible = runtime_visible;
            return true;
        }
        false
    }

    fn publish_command_palette_commands(&mut self, ctx: &mut EventCtx) {
        self.command_palette_provider_index.clear();
        let mut commands = self.system_commands();
        for (provider_index, provider) in self.command_palette_providers.iter_mut().enumerate() {
            for command in provider.commands() {
                self.command_palette_provider_index
                    .insert(command.id.clone(), (provider_index, command.id.clone()));
                commands.push(command);
            }
        }
        if !commands.is_empty() {
            ctx.post_message(Message::CommandPaletteSetCommands(
                crate::message::CommandPaletteSetCommands { commands },
            ));
        }
    }

    fn shutdown_command_palette_providers(&mut self, ctx: &mut EventCtx) {
        for provider in &mut self.command_palette_providers {
            provider.shutdown(ctx);
        }
        self.command_palette_providers.clear();
        self.command_palette_provider_index.clear();
    }

    fn initialize_command_palette_providers(&mut self, ctx: &mut EventCtx) {
        self.shutdown_command_palette_providers(ctx);
        self.command_palette_providers = self
            .app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .command_palette_providers();
        for provider in &mut self.command_palette_providers {
            provider.startup(ctx);
        }
        self.publish_command_palette_commands(ctx);
    }

    fn handle_command_palette_selection(&mut self, command_id: &str, ctx: &mut EventCtx) {
        let Some((provider_index, original_command_id)) =
            self.command_palette_provider_index.get(command_id).cloned()
        else {
            return;
        };
        let Some(provider) = self.command_palette_providers.get_mut(provider_index) else {
            return;
        };
        provider.on_command_selected(&original_command_id, ctx);
    }

    fn command_palette_visible_in_tree(&self) -> bool {
        self.command_palette_visible
    }

    /// Drain any pending reactive changes accumulated on `app.reactive_ctx()`
    /// and dispatch them to the app's `ReactiveWidget` impl (if any).
    ///
    /// Called after every `TextualApp` hook that receives `&mut App` so that
    /// reactive setters called inside hooks trigger watchers and repaint/layout.
    ///
    /// Repaint/layout flags from the setter (stored on `app.reactive_ctx()`) and
    /// from the watcher itself (on the dispatch `rctx`) are both propagated to `ctx`.
    fn dispatch_app_reactive(&self, app: &mut App, ctx: &mut EventCtx) {
        if !app.reactive_ctx().has_changes() {
            return;
        }
        // Capture setter-level repaint/layout flags, drain changes, then reset
        // flags so they don't accumulate across subsequent hook calls.
        let setter_needs_repaint = app.reactive_ctx().needs_repaint();
        let setter_needs_layout = app.reactive_ctx().needs_layout();
        let changes = app.reactive_ctx().take_changes();
        app.reactive_ctx().reset_flags();
        let mut rctx = ReactiveCtx::new(NodeId::default());
        if let Some(rw) = self
            .app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .reactive_widget_mut()
        {
            rw.reactive_dispatch(&changes, &mut rctx);
        }
        if setter_needs_repaint || rctx.needs_repaint() {
            ctx.request_repaint();
        }
        if setter_needs_layout || rctx.needs_layout() {
            ctx.request_layout_invalidation();
        }
    }
}

impl<T: TextualApp> Widget for TextualAppAdapter<T> {
    fn bindings(&self) -> Vec<BindingDecl> {
        let mut bindings = vec![
            BindingDecl::new("tab", "focus_next", "Focus Next")
                .with_namespace("screen")
                .hidden(),
            BindingDecl::new("shift+tab", "focus_previous", "Focus Previous")
                .with_namespace("screen")
                .hidden(),
            BindingDecl::new("ctrl+c,super+c", "copy_selected_text", "Copy selected text")
                .with_namespace("screen")
                .hidden(),
            BindingDecl::new("ctrl+q", "quit", "Quit")
                .with_tooltip("Quit the app and return to the command prompt.")
                .with_namespace("app.system")
                .hidden()
                .priority(),
        ];
        bindings.extend(
            self.app
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .bindings(),
        );
        bindings
    }

    fn action_namespace(&self) -> &str {
        "app"
    }

    fn action_registry(&self) -> &[ActionDecl] {
        APP_ACTIONS
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut EventCtx) -> bool {
        fn selector_and_class(action: &ParsedAction) -> Option<(&str, &str)> {
            if action.arguments.len() != 2 {
                return None;
            }
            Some((&action.arguments[0], &action.arguments[1]))
        }
        fn single_arg(action: &ParsedAction) -> Option<&str> {
            if action.arguments.len() != 1 {
                return None;
            }
            Some(&action.arguments[0])
        }
        fn no_args(action: &ParsedAction) -> bool {
            action.arguments.is_empty()
        }

        match action.name.as_str() {
            "quit" => {
                if !no_args(action) {
                    return false;
                }
                ctx.request_stop();
                ctx.set_handled();
                true
            }
            "back" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(Message::AppBack(crate::message::AppBack));
                ctx.set_handled();
                true
            }
            "bell" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(Message::AppBell(crate::message::AppBell));
                ctx.set_handled();
                true
            }
            "change_theme" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(Message::AppChangeTheme(crate::message::AppChangeTheme));
                ctx.set_handled();
                true
            }
            "command_palette" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(Message::AppCommandPalette(
                    crate::message::AppCommandPalette,
                ));
                ctx.set_handled();
                true
            }
            "focus" => {
                let Some(widget_id) = single_arg(action) else {
                    return false;
                };
                ctx.post_message(Message::AppFocus(crate::message::AppFocus {
                    widget_id: widget_id.to_string(),
                }));
                ctx.set_handled();
                true
            }
            "focus_next" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(Message::AppFocusNext(crate::message::AppFocusNext));
                ctx.set_handled();
                true
            }
            "focus_previous" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(Message::AppFocusPrevious(crate::message::AppFocusPrevious));
                ctx.set_handled();
                true
            }
            "help_quit" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(Message::AppHelpQuit(crate::message::AppHelpQuit));
                ctx.set_handled();
                true
            }
            "copy_selected_text" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(Message::AppCopySelectedText(
                    crate::message::AppCopySelectedText,
                ));
                ctx.set_handled();
                true
            }
            "hide_help_panel" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(Message::AppHideHelpPanel(crate::message::AppHideHelpPanel));
                ctx.set_handled();
                true
            }
            "add_class" => {
                let Some((selector, class_name)) = selector_and_class(action) else {
                    return false;
                };
                ctx.post_message(Message::AppAddClass(crate::message::AppAddClass {
                    selector: selector.to_string(),
                    class_name: class_name.to_string(),
                }));
                ctx.set_handled();
                true
            }
            "remove_class" => {
                let Some((selector, class_name)) = selector_and_class(action) else {
                    return false;
                };
                ctx.post_message(Message::AppRemoveClass(crate::message::AppRemoveClass {
                    selector: selector.to_string(),
                    class_name: class_name.to_string(),
                }));
                ctx.set_handled();
                true
            }
            "toggle_class" => {
                let Some((selector, class_name)) = selector_and_class(action) else {
                    return false;
                };
                ctx.post_message(Message::AppToggleClass(crate::message::AppToggleClass {
                    selector: selector.to_string(),
                    class_name: class_name.to_string(),
                }));
                ctx.set_handled();
                true
            }
            "notify" => {
                if action.arguments.is_empty() || action.arguments.len() > 3 {
                    return false;
                }
                let message = action.arguments[0].clone();
                let title = action.arguments.get(1).cloned().unwrap_or_default();
                let severity = action
                    .arguments
                    .get(2)
                    .cloned()
                    .unwrap_or_else(|| "information".to_string());
                ctx.post_message(Message::AppNotify(crate::message::AppNotify {
                    message,
                    title,
                    severity,
                }));
                ctx.set_handled();
                true
            }
            "pop_screen" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(Message::AppPopScreen(crate::message::AppPopScreen));
                ctx.set_handled();
                true
            }
            "push_screen" => {
                let Some(screen) = single_arg(action) else {
                    return false;
                };
                ctx.post_message(Message::AppPushScreen(crate::message::AppPushScreen {
                    screen: screen.to_string(),
                }));
                ctx.set_handled();
                true
            }
            "screenshot" => {
                if action.arguments.len() > 2 {
                    return false;
                }
                ctx.post_message(Message::AppScreenshot(crate::message::AppScreenshot {
                    filename: action.arguments.first().cloned(),
                    path: action.arguments.get(1).cloned(),
                }));
                ctx.set_handled();
                true
            }
            "show_help_panel" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(Message::AppShowHelpPanel(crate::message::AppShowHelpPanel));
                ctx.set_handled();
                true
            }
            "simulate_key" => {
                let Some(key) = single_arg(action) else {
                    return false;
                };
                ctx.post_message(Message::AppSimulateKey(crate::message::AppSimulateKey {
                    key: key.to_string(),
                }));
                ctx.set_handled();
                true
            }
            "suspend_process" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(Message::AppSuspendProcess(
                    crate::message::AppSuspendProcess,
                ));
                ctx.set_handled();
                true
            }
            "switch_mode" => {
                let Some(mode) = single_arg(action) else {
                    return false;
                };
                ctx.post_message(Message::AppSwitchMode(crate::message::AppSwitchMode {
                    mode: mode.to_string(),
                }));
                ctx.set_handled();
                true
            }
            "switch_screen" => {
                let Some(screen) = single_arg(action) else {
                    return false;
                };
                ctx.post_message(Message::AppSwitchScreen(crate::message::AppSwitchScreen {
                    screen: screen.to_string(),
                }));
                ctx.set_handled();
                true
            }
            "toggle_dark" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(Message::AppToggleDark(crate::message::AppToggleDark));
                ctx.set_handled();
                true
            }
            _ => false,
        }
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        if self.children_extracted {
            return Vec::new();
        }
        self.children_extracted = true;
        let app_child = std::mem::replace(&mut self.app_child, Box::new(Spacer::new(1)));
        let command_palette = Self::make_command_palette_host();
        vec![app_child, Box::new(command_palette)]
    }

    fn child_display_for_tree(&self, child_index: usize) -> Option<bool> {
        match child_index {
            1 => Some(self.command_palette_visible_in_tree()),
            _ => None,
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.app_child.render_styled(console, options)
    }

    fn on_mount(&mut self) {
        self.app_child.on_mount();
        self.app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_mount();
    }

    fn on_unmount(&mut self) {
        let mut ctx = EventCtx::default();
        self.shutdown_command_palette_providers(&mut ctx);
        self.app_child.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.app_child.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.app_child.on_resize(width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.app_child.on_layout(width, height);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.app_child.on_mouse_move(x, y)
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        self.app_child.on_mouse_scroll(delta_x, delta_y, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.app_child.on_event_capture(event, ctx);
    }

    fn on_app_key(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        self.app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_key_with_app(app, key, ctx);
        self.dispatch_app_reactive(app, ctx);
    }

    fn on_app_action(&mut self, app: &mut App, action: Action, ctx: &mut EventCtx) {
        self.app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_action_with_app(app, action, ctx);
        self.dispatch_app_reactive(app, ctx);
    }

    fn on_app_message(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut EventCtx) {
        if self.sync_help_panel_visible_from_runtime(app) && self.command_palette_visible {
            self.publish_command_palette_commands(ctx);
        }
        self.app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_message_with_app(app, message, ctx);
        self.dispatch_app_reactive(app, ctx);
    }

    fn on_app_tick(&mut self, app: &mut App, tick: u64, ctx: &mut EventCtx) {
        self.app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_tick_with_app(app, tick, ctx);
        self.dispatch_app_reactive(app, ctx);
    }

    fn on_app_mount(&mut self, app: &mut App, ctx: &mut EventCtx) {
        self.app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_mount_with_app(app, ctx);
        self.dispatch_app_reactive(app, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.app_child.on_event(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        self.app_child.on_message(message, ctx);
        if ctx.handled() {
            return;
        }
        match &message.message {
            Message::CommandPaletteOpened(_) => {
                self.command_palette_visible = true;
                self.initialize_command_palette_providers(ctx);
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_command_palette_opened(ctx);
                if ctx.handled() {
                    return;
                }
            }
            Message::CommandPaletteClosed(_) => {
                self.command_palette_visible = false;
                self.shutdown_command_palette_providers(ctx);
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_command_palette_closed(ctx);
                if ctx.handled() {
                    return;
                }
            }
            Message::CommandPaletteCommandSelected(
                crate::message::CommandPaletteCommandSelected { id, title },
            ) => {
                self.handle_command_palette_selection(id, ctx);
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_command_palette_command_selected(id, title, ctx);
                if ctx.handled() {
                    return;
                }
            }
            Message::AppShowHelpPanel(_) => {
                self.help_panel_visible = true;
                if self.command_palette_visible {
                    self.publish_command_palette_commands(ctx);
                }
            }
            Message::AppHideHelpPanel(_) => {
                self.help_panel_visible = false;
                if self.command_palette_visible {
                    self.publish_command_palette_commands(ctx);
                }
            }
            _ => {}
        }
        match &message.message {
            Message::ButtonPressed(crate::message::ButtonPressed { description, .. }) => {
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_button_pressed(description, ctx);
            }
            Message::InputChanged(crate::message::InputChanged { value, validation }) => {
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_input_changed(value, validation, ctx);
            }
            Message::InputSubmitted(crate::message::InputSubmitted { value }) => {
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_input_submitted(value, ctx);
            }
            Message::TextAreaChanged(crate::message::TextAreaChanged { value }) => {
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_text_area_changed(value, ctx);
            }
            Message::CheckboxChanged(crate::message::CheckboxChanged { checked }) => {
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_checkbox_changed(*checked, ctx);
            }
            Message::ListViewSelectionChanged(crate::message::ListViewSelectionChanged {
                index,
                item,
            }) => {
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_list_view_selection_changed(*index, item, ctx);
            }
            Message::ListViewItemActivated(crate::message::ListViewItemActivated {
                index,
                item,
            }) => {
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_list_view_item_activated(*index, item, ctx);
            }
            Message::TabActivated(crate::message::TabActivated { index, title, .. }) => {
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_tab_activated(*index, title, ctx);
            }
            _ => {}
        }
        if ctx.handled() {
            return;
        }
    }
}

/// Run a `TextualApp` definition using the standard `App` runtime and return
/// optional app output.
pub async fn run_with_output<T: TextualApp>(definition: T) -> Result<Option<String>> {
    let state = Arc::new(Mutex::new(definition));
    let mut app = App::new()?;

    let css_path = state.lock().unwrap_or_else(|e| e.into_inner()).css_path();
    if let Some(path) = css_path.filter(|path| Path::new(path).exists()) {
        let interval = state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .stylesheet_watch_interval();
        app.watch_stylesheet(path, interval)?;
    }

    state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .configure(&mut app)?;
    let composed = state.lock().unwrap_or_else(|e| e.into_inner()).compose();
    let mut root = build_textual_app_runtime_root(state.clone(), composed);
    app.run_widget_tree(&mut root).await?;
    Ok(state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take_exit_output())
}

/// Run a `TextualApp` definition using the standard `App` runtime.
pub async fn run<T: TextualApp>(definition: T) -> Result<()> {
    let _ = run_with_output(definition).await?;
    Ok(())
}

/// Compatibility alias for [`run`].
pub async fn run_textual_app<T: TextualApp>(definition: T) -> Result<()> {
    run(definition).await
}

/// Compatibility alias for [`run_with_output`].
pub async fn run_textual_app_with_output<T: TextualApp>(definition: T) -> Result<Option<String>> {
    run_with_output(definition).await
}

/// Optional helper for example/dev binaries that support both runtime and snapshot output.
///
/// This keeps snapshot wiring out of example `main()` bodies while remaining opt-in.
pub async fn run_snapshot<T: TextualApp>(definition: T) -> Result<()> {
    let _ = run_snapshot_with_output(definition).await?;
    Ok(())
}

/// Compatibility alias for [`run_snapshot`].
pub async fn run_textual_app_or_snapshot<T: TextualApp>(definition: T) -> Result<()> {
    run_snapshot(definition).await
}

/// Compatibility alias for [`run_snapshot_with_output`].
pub async fn run_textual_app_or_snapshot_with_output<T: TextualApp>(
    definition: T,
) -> Result<Option<String>> {
    run_snapshot_with_output(definition).await
}

/// Variant of `run_snapshot` that returns optional app output.
pub async fn run_snapshot_with_output<T: TextualApp>(mut definition: T) -> Result<Option<String>> {
    if let Some(args) = SnapshotArgs::parse() {
        let widget = definition.compose_for_snapshot();
        let css_path = definition.snapshot_css_path().map(Path::new);
        snapshot_widget(&widget, &args, css_path)?;
        return Ok(None);
    }
    run_with_output(definition).await
}

/// Blocking/synchronous variant of [`run_with_output`].
pub fn run_sync_with_output<T: TextualApp>(definition: T) -> Result<Option<String>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run_with_output(definition))
}

/// Blocking/synchronous variant of [`run`].
pub fn run_sync<T: TextualApp>(definition: T) -> Result<()> {
    let _ = run_sync_with_output(definition)?;
    Ok(())
}

/// Blocking/synchronous variant of [`run_snapshot_with_output`].
pub fn run_sync_snapshot_with_output<T: TextualApp>(definition: T) -> Result<Option<String>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run_snapshot_with_output(definition))
}

/// Blocking/synchronous variant of [`run_snapshot`].
pub fn run_sync_snapshot<T: TextualApp>(definition: T) -> Result<()> {
    let _ = run_sync_snapshot_with_output(definition)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::parse_action;
    use crate::keys::KeyEventData;
    use crate::node_id::node_id_from_ffi;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use rich_rs::{Console, ConsoleOptions, Segments};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct HookState {
        last_button: Option<String>,
        input_changed: Option<(String, bool)>,
        input_submitted: Option<String>,
        text_area_changed: Option<String>,
        checkbox_changed: Option<bool>,
        list_selection: Option<(usize, String)>,
        list_activated: Option<(usize, String)>,
        tab_activated: Option<(usize, String)>,
        command_palette_events: Vec<String>,
        fallback_count: usize,
    }

    #[derive(Clone)]
    struct ProviderState {
        startup_count: Arc<AtomicUsize>,
        shutdown_count: Arc<AtomicUsize>,
        selected_count: Arc<AtomicUsize>,
    }

    struct TestProvider {
        state: ProviderState,
    }

    impl CommandPaletteProvider for TestProvider {
        fn startup(&mut self, _ctx: &mut EventCtx) {
            self.state.startup_count.fetch_add(1, Ordering::SeqCst);
        }

        fn commands(&mut self) -> Vec<CommandPaletteCommand> {
            vec![CommandPaletteCommand {
                id: "deploy".to_string(),
                title: "Deploy".to_string(),
                help: "Ship the current build".to_string(),
            }]
        }

        fn on_command_selected(&mut self, command_id: &str, _ctx: &mut EventCtx) {
            if command_id == "deploy" {
                self.state.selected_count.fetch_add(1, Ordering::SeqCst);
            }
        }

        fn shutdown(&mut self, _ctx: &mut EventCtx) {
            self.state.shutdown_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct TestApp {
        provider_state: ProviderState,
        hooks: HookState,
    }

    impl TextualApp for TestApp {
        fn compose(&mut self) -> crate::widgets::AppRoot {
            crate::widgets::AppRoot::new()
        }

        fn command_palette_providers(&mut self) -> Vec<Box<dyn CommandPaletteProvider>> {
            vec![Box::new(TestProvider {
                state: self.provider_state.clone(),
            })]
        }

        fn on_button_pressed(&mut self, description: &str, _ctx: &mut EventCtx) {
            self.hooks.last_button = Some(description.to_string());
        }

        fn on_input_changed(
            &mut self,
            value: &str,
            validation: &ValidationResult,
            _ctx: &mut EventCtx,
        ) {
            self.hooks
                .input_changed
                .replace((value.to_string(), validation.is_valid));
        }

        fn on_input_submitted(&mut self, value: &str, _ctx: &mut EventCtx) {
            self.hooks.input_submitted = Some(value.to_string());
        }

        fn on_text_area_changed(&mut self, value: &str, _ctx: &mut EventCtx) {
            self.hooks.text_area_changed = Some(value.to_string());
        }

        fn on_checkbox_changed(&mut self, checked: bool, _ctx: &mut EventCtx) {
            self.hooks.checkbox_changed = Some(checked);
        }

        fn on_list_view_selection_changed(
            &mut self,
            index: usize,
            item: &str,
            _ctx: &mut EventCtx,
        ) {
            self.hooks.list_selection = Some((index, item.to_string()));
        }

        fn on_list_view_item_activated(&mut self, index: usize, item: &str, _ctx: &mut EventCtx) {
            self.hooks.list_activated = Some((index, item.to_string()));
        }

        fn on_tab_activated(&mut self, index: usize, title: &str, _ctx: &mut EventCtx) {
            self.hooks.tab_activated = Some((index, title.to_string()));
        }

        fn on_command_palette_opened(&mut self, _ctx: &mut EventCtx) {
            self.hooks.command_palette_events.push("opened".to_string());
        }

        fn on_command_palette_closed(&mut self, _ctx: &mut EventCtx) {
            self.hooks.command_palette_events.push("closed".to_string());
        }

        fn on_command_palette_command_selected(
            &mut self,
            id: &str,
            _title: &str,
            _ctx: &mut EventCtx,
        ) {
            self.hooks
                .command_palette_events
                .push(format!("selected:{id}"));
        }

        fn on_message(&mut self, _message: &MessageEvent, _ctx: &mut EventCtx) {
            self.hooks.fallback_count += 1;
        }
    }

    struct NoopWidget;

    impl NoopWidget {
        fn new() -> Self {
            Self
        }
    }

    impl Widget for NoopWidget {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }
    }

    struct CaptureProbe {
        capture_hits: Arc<AtomicUsize>,
    }

    impl CaptureProbe {
        fn new(capture_hits: Arc<AtomicUsize>) -> Self {
            Self { capture_hits }
        }
    }

    impl Widget for CaptureProbe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn on_event_capture(&mut self, event: &Event, _ctx: &mut EventCtx) {
            if matches!(event, Event::Key(_)) {
                self.capture_hits.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    struct KeyHookApp {
        key_hits: Arc<AtomicUsize>,
        handle_key: bool,
    }

    impl TextualApp for KeyHookApp {
        fn compose(&mut self) -> crate::widgets::AppRoot {
            crate::widgets::AppRoot::new()
        }

        fn on_key(&mut self, _key: &KeyEventData, ctx: &mut EventCtx) {
            self.key_hits.fetch_add(1, Ordering::SeqCst);
            if self.handle_key {
                ctx.set_handled();
            }
        }
    }

    struct LegacyAppHookForwardingApp {
        action_hits: Arc<AtomicUsize>,
        message_hits: Arc<AtomicUsize>,
        tick_hits: Arc<AtomicUsize>,
    }

    impl TextualApp for LegacyAppHookForwardingApp {
        fn compose(&mut self) -> crate::widgets::AppRoot {
            crate::widgets::AppRoot::new()
        }

        fn on_action(&mut self, _action: Action, _ctx: &mut EventCtx) {
            self.action_hits.fetch_add(1, Ordering::SeqCst);
        }

        fn on_message(&mut self, _message: &MessageEvent, _ctx: &mut EventCtx) {
            self.message_hits.fetch_add(1, Ordering::SeqCst);
        }

        fn on_tick(&mut self, _tick: u64, _ctx: &mut EventCtx) {
            self.tick_hits.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct AppHandleHooksApp {
        action_hits: Arc<AtomicUsize>,
        message_hits: Arc<AtomicUsize>,
        tick_hits: Arc<AtomicUsize>,
    }

    impl TextualApp for AppHandleHooksApp {
        fn compose(&mut self) -> crate::widgets::AppRoot {
            crate::widgets::AppRoot::new()
        }

        fn on_action_with_app(&mut self, app: &mut App, _action: Action, _ctx: &mut EventCtx) {
            app.set_css_runtime_pseudos(true, false, true);
            self.action_hits.fetch_add(1, Ordering::SeqCst);
        }

        fn on_message_with_app(
            &mut self,
            app: &mut App,
            _message: &MessageEvent,
            _ctx: &mut EventCtx,
        ) {
            let (inline, ansi, nocolor) = app.css_runtime_pseudos();
            if inline && !ansi && nocolor {
                self.message_hits.fetch_add(1, Ordering::SeqCst);
            }
        }

        fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, _ctx: &mut EventCtx) {
            let _ = app.query_one_optional("Button");
            self.tick_hits.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn command_palette_providers_start_set_commands_select_and_shutdown() {
        let state = ProviderState {
            startup_count: Arc::new(AtomicUsize::new(0)),
            shutdown_count: Arc::new(AtomicUsize::new(0)),
            selected_count: Arc::new(AtomicUsize::new(0)),
        };
        let app = Arc::new(Mutex::new(TestApp {
            provider_state: state.clone(),
            hooks: HookState::default(),
        }));
        let mut adapter = TextualAppAdapter::new(app, NoopWidget::new());

        let mut open_ctx = EventCtx::default();
        adapter.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::CommandPaletteOpened(crate::message::CommandPaletteOpened),
                control: None,
            },
            &mut open_ctx,
        );
        assert_eq!(state.startup_count.load(Ordering::SeqCst), 1);
        let open_messages = open_ctx.take_messages();
        assert!(
            open_messages
                .iter()
                .any(|event| matches!(event.message, Message::CommandPaletteSetCommands(..)))
        );

        let mut select_ctx = EventCtx::default();
        adapter.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::CommandPaletteCommandSelected(
                    crate::message::CommandPaletteCommandSelected {
                        id: "deploy".to_string(),
                        title: "Deploy".to_string(),
                    },
                ),
                control: None,
            },
            &mut select_ctx,
        );
        assert_eq!(state.selected_count.load(Ordering::SeqCst), 1);

        let mut close_ctx = EventCtx::default();
        adapter.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::CommandPaletteClosed(crate::message::CommandPaletteClosed),
                control: None,
            },
            &mut close_ctx,
        );
        assert_eq!(state.shutdown_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn command_palette_providers_restart_on_reopen() {
        let state = ProviderState {
            startup_count: Arc::new(AtomicUsize::new(0)),
            shutdown_count: Arc::new(AtomicUsize::new(0)),
            selected_count: Arc::new(AtomicUsize::new(0)),
        };
        let app = Arc::new(Mutex::new(TestApp {
            provider_state: state.clone(),
            hooks: HookState::default(),
        }));
        let mut adapter = TextualAppAdapter::new(app, NoopWidget::new());

        let mut first_open_ctx = EventCtx::default();
        adapter.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::CommandPaletteOpened(crate::message::CommandPaletteOpened),
                control: None,
            },
            &mut first_open_ctx,
        );
        let mut first_close_ctx = EventCtx::default();
        adapter.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::CommandPaletteClosed(crate::message::CommandPaletteClosed),
                control: None,
            },
            &mut first_close_ctx,
        );
        let mut second_open_ctx = EventCtx::default();
        adapter.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::CommandPaletteOpened(crate::message::CommandPaletteOpened),
                control: None,
            },
            &mut second_open_ctx,
        );
        let mut second_close_ctx = EventCtx::default();
        adapter.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::CommandPaletteClosed(crate::message::CommandPaletteClosed),
                control: None,
            },
            &mut second_close_ctx,
        );

        assert_eq!(state.startup_count.load(Ordering::SeqCst), 2);
        assert_eq!(state.shutdown_count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn command_palette_system_keys_help_updates_with_help_panel_state() {
        let state = ProviderState {
            startup_count: Arc::new(AtomicUsize::new(0)),
            shutdown_count: Arc::new(AtomicUsize::new(0)),
            selected_count: Arc::new(AtomicUsize::new(0)),
        };
        let app = Arc::new(Mutex::new(TestApp {
            provider_state: state,
            hooks: HookState::default(),
        }));
        let mut adapter = TextualAppAdapter::new(app, NoopWidget::new());

        let mut open_ctx = EventCtx::default();
        adapter.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::CommandPaletteOpened(crate::message::CommandPaletteOpened),
                control: None,
            },
            &mut open_ctx,
        );

        let open_messages = open_ctx.take_messages();
        let open_commands = open_messages
            .iter()
            .find_map(|event| match &event.message {
                Message::CommandPaletteSetCommands(crate::message::CommandPaletteSetCommands {
                    commands,
                }) => Some(commands.clone()),
                _ => None,
            })
            .expect("open should publish command palette commands");
        let keys_help = open_commands
            .iter()
            .find(|command| command.id == "keys")
            .map(|command| command.help.clone())
            .expect("keys command should be present");
        assert_eq!(
            keys_help,
            "Show help for the focused widget and a summary of available keys"
        );

        assert!(
            !open_commands.iter().any(|command| command.id == "maximize"),
            "unsupported maximize command should not be published until runtime maximize exists"
        );

        let mut show_ctx = EventCtx::default();
        adapter.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::AppShowHelpPanel(crate::message::AppShowHelpPanel),
                control: None,
            },
            &mut show_ctx,
        );
        let show_messages = show_ctx.take_messages();
        let show_commands = show_messages
            .iter()
            .find_map(|event| match &event.message {
                Message::CommandPaletteSetCommands(crate::message::CommandPaletteSetCommands {
                    commands,
                }) => Some(commands.clone()),
                _ => None,
            })
            .expect("show-help should republish command palette commands while open");
        let keys_help = show_commands
            .iter()
            .find(|command| command.id == "keys")
            .map(|command| command.help.clone())
            .expect("keys command should be present");
        assert_eq!(keys_help, "Hide the keys and widget help panel");

        let mut hide_ctx = EventCtx::default();
        adapter.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::AppHideHelpPanel(crate::message::AppHideHelpPanel),
                control: None,
            },
            &mut hide_ctx,
        );
        let hide_messages = hide_ctx.take_messages();
        let hide_commands = hide_messages
            .iter()
            .find_map(|event| match &event.message {
                Message::CommandPaletteSetCommands(crate::message::CommandPaletteSetCommands {
                    commands,
                }) => Some(commands.clone()),
                _ => None,
            })
            .expect("hide-help should republish command palette commands while open");
        let keys_help = hide_commands
            .iter()
            .find(|command| command.id == "keys")
            .map(|command| command.help.clone())
            .expect("keys command should be present");
        assert_eq!(
            keys_help,
            "Show help for the focused widget and a summary of available keys"
        );
    }

    #[test]
    fn typed_hooks_receive_common_message_payloads() {
        let state = ProviderState {
            startup_count: Arc::new(AtomicUsize::new(0)),
            shutdown_count: Arc::new(AtomicUsize::new(0)),
            selected_count: Arc::new(AtomicUsize::new(0)),
        };
        let app = Arc::new(Mutex::new(TestApp {
            provider_state: state,
            hooks: HookState::default(),
        }));
        let mut adapter = TextualAppAdapter::new(app.clone(), NoopWidget::new());

        let mut messages = vec![
            Message::ButtonPressed(crate::message::ButtonPressed {
                description: "ok".to_string(),
                button_id: None,
            }),
            Message::InputChanged(crate::message::InputChanged {
                value: "42".to_string(),
                validation: ValidationResult::success(),
            }),
            Message::InputSubmitted(crate::message::InputSubmitted {
                value: "submit".to_string(),
            }),
            Message::TextAreaChanged(crate::message::TextAreaChanged {
                value: "textarea".to_string(),
            }),
            Message::CheckboxChanged(crate::message::CheckboxChanged { checked: true }),
            Message::ListViewSelectionChanged(crate::message::ListViewSelectionChanged {
                index: 2,
                item: "gamma".to_string(),
            }),
            Message::ListViewItemActivated(crate::message::ListViewItemActivated {
                index: 3,
                item: "delta".to_string(),
            }),
            Message::TabActivated(crate::message::TabActivated {
                id: "general".to_string(),
                index: 1,
                title: "General".to_string(),
            }),
        ];
        for message in messages.drain(..) {
            let mut ctx = EventCtx::default();
            adapter.on_message(
                &MessageEvent {
                    sender: NodeId::default(),
                    message,
                    control: None,
                },
                &mut ctx,
            );
        }

        let app = app.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(app.hooks.last_button.as_deref(), Some("ok"));
        assert_eq!(app.hooks.input_changed, Some(("42".to_string(), true)));
        assert_eq!(app.hooks.input_submitted.as_deref(), Some("submit"));
        assert_eq!(app.hooks.text_area_changed.as_deref(), Some("textarea"));
        assert_eq!(app.hooks.checkbox_changed, Some(true));
        assert_eq!(app.hooks.list_selection, Some((2, "gamma".to_string())));
        assert_eq!(app.hooks.list_activated, Some((3, "delta".to_string())));
        assert_eq!(app.hooks.tab_activated, Some((1, "General".to_string())));
        assert_eq!(app.hooks.fallback_count, 0);
    }

    #[test]
    fn adapter_exposes_composed_child_once_for_tree_build() {
        let app = Arc::new(Mutex::new(TestApp {
            provider_state: ProviderState {
                startup_count: Arc::new(AtomicUsize::new(0)),
                shutdown_count: Arc::new(AtomicUsize::new(0)),
                selected_count: Arc::new(AtomicUsize::new(0)),
            },
            hooks: HookState::default(),
        }));
        let mut adapter = TextualAppAdapter::new(app, NoopWidget::new());

        let mut first = adapter.take_composed_children();
        assert_eq!(first.len(), 2);
        assert!(
            first
                .iter()
                .any(|child| child.style_type() == "CommandPalette"),
            "runtime root should include a live CommandPalette host child"
        );
        let palette = first
            .iter()
            .find(|child| child.style_type() == "CommandPalette")
            .expect("command palette child should be present");
        assert_eq!(
            palette.style().and_then(|style| style.position),
            Some(Position::Absolute),
            "command palette host should be out-of-flow overlay in runtime root"
        );
        let palette = first
            .iter_mut()
            .find(|child| child.style_type() == "CommandPalette")
            .expect("command palette child should be present");
        let palette_children = palette.take_composed_children();
        assert_eq!(
            palette_children.len(),
            1,
            "command palette host should expose one wrapped child subtree"
        );
        assert_eq!(
            palette.child_display_for_tree(0),
            Some(false),
            "command palette host wrapped child should stay hidden in tree mode"
        );
        assert_eq!(
            adapter.child_display_for_tree(1),
            Some(false),
            "command palette host should be hidden in tree when closed"
        );

        let second = adapter.take_composed_children();
        assert!(second.is_empty());
    }

    #[test]
    fn adapter_shows_command_palette_host_when_palette_opens() {
        let app = Arc::new(Mutex::new(TestApp {
            provider_state: ProviderState {
                startup_count: Arc::new(AtomicUsize::new(0)),
                shutdown_count: Arc::new(AtomicUsize::new(0)),
                selected_count: Arc::new(AtomicUsize::new(0)),
            },
            hooks: HookState::default(),
        }));
        let mut adapter = TextualAppAdapter::new(app, NoopWidget::new());
        let _ = adapter.take_composed_children();
        assert_eq!(adapter.child_display_for_tree(1), Some(false));

        let mut ctx = EventCtx::default();
        adapter.on_message(
            &MessageEvent {
                sender: node_id_from_ffi(42),
                message: Message::CommandPaletteOpened(crate::message::CommandPaletteOpened),
                control: Some(node_id_from_ffi(42)),
            },
            &mut ctx,
        );
        assert_eq!(
            adapter.child_display_for_tree(1),
            Some(true),
            "command palette host should become visible in tree once opened"
        );
    }

    #[test]
    fn overlay_screen_stack_posts_visibility_messages_for_push_pop() {
        let sender = node_id_from_ffi(9);
        let first = node_id_from_ffi(111);
        let second = node_id_from_ffi(222);
        let mut stack = OverlayScreenStack::new();
        let mut ctx = EventCtx::default();

        assert!(stack.push(sender, first, &mut ctx));
        assert!(stack.push(sender, second, &mut ctx));
        assert_eq!(stack.current(), Some(second));
        assert_eq!(stack.pop(sender, &mut ctx), Some(second));
        assert_eq!(stack.current(), Some(first));
        stack.clear(sender, &mut ctx);
        assert!(stack.is_empty());

        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 6);
        assert!(matches!(
            messages[0].message,
            Message::OverlaySetVisible(crate::message::OverlaySetVisible {
                overlay,
                visible: true,
            }) if overlay == first
        ));
        assert!(matches!(
            messages[1].message,
            Message::OverlaySetVisible(crate::message::OverlaySetVisible {
                overlay,
                visible: false,
            }) if overlay == first
        ));
        assert!(matches!(
            messages[2].message,
            Message::OverlaySetVisible(crate::message::OverlaySetVisible {
                overlay,
                visible: true,
            }) if overlay == second
        ));
        assert!(matches!(
            messages[3].message,
            Message::OverlaySetVisible(crate::message::OverlaySetVisible {
                overlay,
                visible: false,
            }) if overlay == second
        ));
        assert!(matches!(
            messages[4].message,
            Message::OverlaySetVisible(crate::message::OverlaySetVisible {
                overlay,
                visible: true,
            }) if overlay == first
        ));
        assert!(matches!(
            messages[5].message,
            Message::OverlaySetVisible(crate::message::OverlaySetVisible {
                overlay,
                visible: false,
            }) if overlay == first
        ));
    }

    #[test]
    fn on_key_hook_runs_before_child_capture_and_can_handle() {
        let key_hits = Arc::new(AtomicUsize::new(0));
        let capture_hits = Arc::new(AtomicUsize::new(0));
        let app = Arc::new(Mutex::new(KeyHookApp {
            key_hits: key_hits.clone(),
            handle_key: true,
        }));
        let mut adapter = TextualAppAdapter::new(app, CaptureProbe::new(capture_hits.clone()));
        let mut runtime = App::new().expect("app runtime should initialize");
        let mut ctx = EventCtx::default();
        let key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));

        adapter.on_app_key(&mut runtime, &key, &mut ctx);

        assert_eq!(key_hits.load(Ordering::SeqCst), 1);
        assert_eq!(capture_hits.load(Ordering::SeqCst), 0);
        assert!(ctx.handled());
    }

    #[test]
    fn on_key_hook_passthrough_allows_child_capture() {
        let key_hits = Arc::new(AtomicUsize::new(0));
        let capture_hits = Arc::new(AtomicUsize::new(0));
        let app = Arc::new(Mutex::new(KeyHookApp {
            key_hits: key_hits.clone(),
            handle_key: false,
        }));
        let mut adapter = TextualAppAdapter::new(app, CaptureProbe::new(capture_hits.clone()));
        let mut runtime = App::new().expect("app runtime should initialize");
        let mut ctx = EventCtx::default();
        let key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));

        adapter.on_app_key(&mut runtime, &key, &mut ctx);
        if !ctx.handled() {
            adapter.on_event_capture(&Event::Key(key), &mut ctx);
        }

        assert_eq!(key_hits.load(Ordering::SeqCst), 1);
        assert_eq!(capture_hits.load(Ordering::SeqCst), 1);
        assert!(!ctx.handled());
    }

    #[test]
    fn legacy_action_message_and_tick_hooks_forward_from_with_app_variants() {
        let action_hits = Arc::new(AtomicUsize::new(0));
        let message_hits = Arc::new(AtomicUsize::new(0));
        let tick_hits = Arc::new(AtomicUsize::new(0));
        let app = Arc::new(Mutex::new(LegacyAppHookForwardingApp {
            action_hits: Arc::clone(&action_hits),
            message_hits: Arc::clone(&message_hits),
            tick_hits: Arc::clone(&tick_hits),
        }));
        let mut adapter = TextualAppAdapter::new(app, NoopWidget::new());
        let mut runtime = App::new().expect("app runtime should initialize");

        let mut action_ctx = EventCtx::default();
        adapter.on_app_action(&mut runtime, Action::HelpQuit, &mut action_ctx);

        let mut message_ctx = EventCtx::default();
        adapter.on_app_message(
            &mut runtime,
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::FooterBindingsUpdated(crate::message::FooterBindingsUpdated {
                    count: 0,
                }),
                control: None,
            },
            &mut message_ctx,
        );

        let mut tick_ctx = EventCtx::default();
        adapter.on_app_tick(&mut runtime, 7, &mut tick_ctx);

        assert_eq!(action_hits.load(Ordering::SeqCst), 1);
        assert_eq!(message_hits.load(Ordering::SeqCst), 1);
        assert_eq!(tick_hits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn with_app_hooks_receive_runtime_handle() {
        let action_hits = Arc::new(AtomicUsize::new(0));
        let message_hits = Arc::new(AtomicUsize::new(0));
        let tick_hits = Arc::new(AtomicUsize::new(0));
        let app = Arc::new(Mutex::new(AppHandleHooksApp {
            action_hits: Arc::clone(&action_hits),
            message_hits: Arc::clone(&message_hits),
            tick_hits: Arc::clone(&tick_hits),
        }));
        let mut adapter = TextualAppAdapter::new(app, NoopWidget::new());
        let mut runtime = App::new().expect("app runtime should initialize");

        let mut action_ctx = EventCtx::default();
        adapter.on_app_action(&mut runtime, Action::HelpQuit, &mut action_ctx);

        let mut message_ctx = EventCtx::default();
        adapter.on_app_message(
            &mut runtime,
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::FooterBindingsUpdated(crate::message::FooterBindingsUpdated {
                    count: 0,
                }),
                control: None,
            },
            &mut message_ctx,
        );

        let mut tick_ctx = EventCtx::default();
        adapter.on_app_tick(&mut runtime, 9, &mut tick_ctx);

        assert_eq!(action_hits.load(Ordering::SeqCst), 1);
        assert_eq!(message_hits.load(Ordering::SeqCst), 1);
        assert_eq!(tick_hits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn app_selector_class_actions_emit_runtime_messages() {
        let app = Arc::new(Mutex::new(TestApp {
            provider_state: ProviderState {
                startup_count: Arc::new(AtomicUsize::new(0)),
                shutdown_count: Arc::new(AtomicUsize::new(0)),
                selected_count: Arc::new(AtomicUsize::new(0)),
            },
            hooks: HookState::default(),
        }));
        let mut adapter = TextualAppAdapter::new(app, NoopWidget::new());
        let mut ctx = EventCtx::default();

        let add =
            parse_action("app.add_class('Button', 'primary')").expect("add action should parse");
        let remove = parse_action("app.remove_class('Button', 'primary')")
            .expect("remove action should parse");
        let toggle = parse_action("app.toggle_class('Button', 'primary')")
            .expect("toggle action should parse");
        assert!(adapter.execute_action(&add, &mut ctx));
        assert!(adapter.execute_action(&remove, &mut ctx));
        assert!(adapter.execute_action(&toggle, &mut ctx));

        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 3);
        assert!(matches!(
            messages[0].message,
            Message::AppAddClass(crate::message::AppAddClass {
                ref selector,
                ref class_name
            }) if selector == "Button" && class_name == "primary"
        ));
        assert!(matches!(
            messages[1].message,
            Message::AppRemoveClass(crate::message::AppRemoveClass {
                ref selector,
                ref class_name
            }) if selector == "Button" && class_name == "primary"
        ));
        assert!(matches!(
            messages[2].message,
            Message::AppToggleClass(crate::message::AppToggleClass {
                ref selector,
                ref class_name
            }) if selector == "Button" && class_name == "primary"
        ));
    }

    #[test]
    fn app_adapter_action_registry_matches_python_action_matrix() {
        let app = Arc::new(Mutex::new(TestApp {
            provider_state: ProviderState {
                startup_count: Arc::new(AtomicUsize::new(0)),
                shutdown_count: Arc::new(AtomicUsize::new(0)),
                selected_count: Arc::new(AtomicUsize::new(0)),
            },
            hooks: HookState::default(),
        }));
        let adapter = TextualAppAdapter::new(app, NoopWidget::new());
        let names: std::collections::HashSet<&str> =
            adapter.action_registry().iter().map(|a| a.name).collect();
        let expected = [
            "add_class",
            "back",
            "bell",
            "change_theme",
            "copy_selected_text",
            "command_palette",
            "focus",
            "focus_next",
            "focus_previous",
            "help_quit",
            "hide_help_panel",
            "notify",
            "pop_screen",
            "push_screen",
            "quit",
            "remove_class",
            "screenshot",
            "show_help_panel",
            "simulate_key",
            "suspend_process",
            "switch_mode",
            "switch_screen",
            "toggle_class",
            "toggle_dark",
        ];
        assert_eq!(names.len(), expected.len());
        for name in expected {
            assert!(names.contains(name), "missing action {name}");
        }
    }

    #[test]
    fn app_action_caller_inventory_rows_are_complete() {
        struct CallerRow {
            action: &'static str,
            python_callers: &'static [&'static str],
            rust_callers: &'static [&'static str],
        }

        // APIG-14: keep a concrete caller audit per Python action row.
        // Each row requires at least one Python callsite class and one Rust
        // equivalent caller path.
        let rows = [
            CallerRow {
                action: "add_class",
                python_callers: &["app.py:4400", "app.py:4407"],
                rust_callers: &["textual_app.rs:424", "runtime/event_loop.rs:361"],
            },
            CallerRow {
                action: "back",
                python_callers: &["app.py:4387", "screen.py:1913"],
                rust_callers: &["textual_app.rs:350", "runtime/event_loop.rs:409"],
            },
            CallerRow {
                action: "bell",
                python_callers: &["app.py:4345", "_footer.py:134"],
                rust_callers: &["textual_app.rs:358", "runtime/event_loop.rs:416"],
            },
            CallerRow {
                action: "change_theme",
                python_callers: &["app.py:1761", "app.py:1281"],
                rust_callers: &["textual_app.rs:366", "runtime/event_loop.rs:419"],
            },
            CallerRow {
                action: "copy_selected_text",
                python_callers: &["screen.py:960", "screen.py:978"],
                rust_callers: &["textual_app.rs:508", "runtime/event_loop.rs:713"],
            },
            CallerRow {
                action: "command_palette",
                python_callers: &["app.py:4584", "_header.py:46"],
                rust_callers: &["textual_app.rs:374", "runtime/event_loop.rs:424"],
            },
            CallerRow {
                action: "focus",
                python_callers: &["app.py:4349", "screen.py:999"],
                rust_callers: &["textual_app.rs:382", "runtime/event_loop.rs:432"],
            },
            CallerRow {
                action: "focus_next",
                python_callers: &["app.py:4435", "app.py:4437"],
                rust_callers: &["textual_app.rs:392", "runtime/event_loop.rs:446"],
            },
            CallerRow {
                action: "focus_previous",
                python_callers: &["app.py:4439", "app.py:4441"],
                rust_callers: &["textual_app.rs:400", "runtime/event_loop.rs:451"],
            },
            CallerRow {
                action: "help_quit",
                python_callers: &["app.py:3880", "app.py:3889"],
                rust_callers: &["textual_app.rs:408", "runtime/event_loop.rs:456"],
            },
            CallerRow {
                action: "hide_help_panel",
                python_callers: &["app.py:4443", "app.py:1293"],
                rust_callers: &["textual_app.rs:416", "runtime/event_loop.rs:460"],
            },
            CallerRow {
                action: "notify",
                python_callers: &["app.py:4456", "app.py:4460"],
                rust_callers: &["textual_app.rs:457", "runtime/event_loop.rs:474"],
            },
            CallerRow {
                action: "pop_screen",
                python_callers: &["app.py:4379", "app.py:4381"],
                rust_callers: &["textual_app.rs:476", "runtime/event_loop.rs:482"],
            },
            CallerRow {
                action: "push_screen",
                python_callers: &["app.py:4371", "app.py:4587"],
                rust_callers: &["textual_app.rs:484", "runtime/event_loop.rs:489"],
            },
            CallerRow {
                action: "quit",
                python_callers: &["app.py:4341", "app.py:1286"],
                rust_callers: &["textual_app.rs:342", "textual_app.rs:1676"],
            },
            CallerRow {
                action: "remove_class",
                python_callers: &["app.py:4409", "app.py:4415"],
                rust_callers: &["textual_app.rs:435", "runtime/event_loop.rs:377"],
            },
            CallerRow {
                action: "screenshot",
                python_callers: &["app.py:1765", "demo_app.py:63"],
                rust_callers: &["textual_app.rs:494", "runtime/event_loop.rs:500"],
            },
            CallerRow {
                action: "show_help_panel",
                python_callers: &["app.py:4447", "app.py:1299"],
                rust_callers: &["textual_app.rs:502", "runtime/event_loop.rs:508"],
            },
            CallerRow {
                action: "simulate_key",
                python_callers: &["app.py:4331", "_footer.py:136"],
                rust_callers: &["textual_app.rs:510", "runtime/event_loop.rs:522"],
            },
            CallerRow {
                action: "suspend_process",
                python_callers: &["app.py:4651", "app.py:4631"],
                rust_callers: &["textual_app.rs:520", "runtime/event_loop.rs:565"],
            },
            CallerRow {
                action: "switch_mode",
                python_callers: &["app.py:4383", "demo_app.py:39"],
                rust_callers: &["textual_app.rs:528", "runtime/event_loop.rs:573"],
            },
            CallerRow {
                action: "switch_screen",
                python_callers: &["app.py:4363", "app.py:4369"],
                rust_callers: &["textual_app.rs:538", "runtime/event_loop.rs:582"],
            },
            CallerRow {
                action: "toggle_class",
                python_callers: &["app.py:4417", "app.py:4424"],
                rust_callers: &["textual_app.rs:446", "runtime/event_loop.rs:393"],
            },
            CallerRow {
                action: "toggle_dark",
                python_callers: &["app.py:4426", "app.py:4431"],
                rust_callers: &["textual_app.rs:548", "runtime/event_loop.rs:593"],
            },
        ];

        assert_eq!(rows.len(), 24);
        for row in rows {
            assert!(
                APP_ACTIONS.iter().any(|action| action.name == row.action),
                "missing APP_ACTIONS row for {}",
                row.action
            );
            assert!(
                !row.python_callers.is_empty(),
                "missing python caller audit for {}",
                row.action
            );
            assert!(
                !row.rust_callers.is_empty(),
                "missing rust caller alignment for {}",
                row.action
            );
        }
    }

    #[test]
    fn app_actions_emit_runtime_messages_for_full_matrix() {
        let app = Arc::new(Mutex::new(TestApp {
            provider_state: ProviderState {
                startup_count: Arc::new(AtomicUsize::new(0)),
                shutdown_count: Arc::new(AtomicUsize::new(0)),
                selected_count: Arc::new(AtomicUsize::new(0)),
            },
            hooks: HookState::default(),
        }));
        let mut adapter = TextualAppAdapter::new(app, NoopWidget::new());
        let mut ctx = EventCtx::default();

        let ok_actions = [
            "app.back",
            "app.bell",
            "app.change_theme",
            "app.copy_selected_text",
            "app.command_palette",
            "app.focus('sidebar')",
            "app.focus_next",
            "app.focus_previous",
            "app.help_quit",
            "app.hide_help_panel",
            "app.add_class('Button', 'x')",
            "app.remove_class('Button', 'x')",
            "app.toggle_class('Button', 'x')",
            "app.notify('hello')",
            "app.notify('hello', 'title', 'warning')",
            "app.pop_screen",
            "app.push_screen('home')",
            "app.screenshot",
            "app.screenshot('shot.svg')",
            "app.screenshot('shot.svg', '/tmp')",
            "app.show_help_panel",
            "app.simulate_key('tab')",
            "app.suspend_process",
            "app.switch_mode('home')",
            "app.switch_screen('main')",
            "app.toggle_dark",
        ];

        for action in ok_actions {
            let parsed = parse_action(action).expect("action should parse");
            assert!(
                adapter.execute_action(&parsed, &mut ctx),
                "expected handled: {action}"
            );
        }

        let messages = ctx.take_messages();
        assert!(!messages.is_empty());
        assert!(
            messages
                .iter()
                .any(|m| matches!(m.message, Message::AppBack(_)))
        );
        assert!(
            messages
                .iter()
                .any(|m| matches!(m.message, Message::AppBell(_)))
        );
        assert!(
            messages
                .iter()
                .any(|m| matches!(m.message, Message::AppFocus(_)))
        );
        assert!(
            messages
                .iter()
                .any(|m| matches!(m.message, Message::AppNotify(_)))
        );
        assert!(
            messages
                .iter()
                .any(|m| matches!(m.message, Message::AppSwitchMode(_)))
        );
        assert!(
            messages
                .iter()
                .any(|m| matches!(m.message, Message::AppToggleDark(_)))
        );
    }

    #[test]
    fn app_action_argument_validation_rejects_invalid_arity() {
        let app = Arc::new(Mutex::new(TestApp {
            provider_state: ProviderState {
                startup_count: Arc::new(AtomicUsize::new(0)),
                shutdown_count: Arc::new(AtomicUsize::new(0)),
                selected_count: Arc::new(AtomicUsize::new(0)),
            },
            hooks: HookState::default(),
        }));
        let mut adapter = TextualAppAdapter::new(app, NoopWidget::new());
        let mut ctx = EventCtx::default();

        for action in [
            "app.focus",
            "app.add_class('Button')",
            "app.notify",
            "app.notify('a','b','c','d')",
            "app.push_screen",
            "app.screenshot('a','b','c')",
            "app.switch_mode",
            "app.switch_screen",
            "app.simulate_key",
        ] {
            let parsed = parse_action(action).expect("action should parse");
            assert!(
                !adapter.execute_action(&parsed, &mut ctx),
                "expected invalid arity for {action}"
            );
        }
    }

    #[test]
    fn textual_app_runtime_root_includes_command_palette_host() {
        let app = Arc::new(Mutex::new(TestApp {
            provider_state: ProviderState {
                startup_count: Arc::new(AtomicUsize::new(0)),
                shutdown_count: Arc::new(AtomicUsize::new(0)),
                selected_count: Arc::new(AtomicUsize::new(0)),
            },
            hooks: HookState::default(),
        }));
        let mut root = build_textual_app_runtime_root(app, crate::widgets::AppRoot::new());
        assert!(
            root.take_composed_children()
                .iter()
                .any(|child| child.style_type() == "CommandPalette"),
            "runtime root should compose a command palette host widget"
        );
    }

    #[test]
    fn app_quit_action_requests_stop() {
        let app = Arc::new(Mutex::new(TestApp {
            provider_state: ProviderState {
                startup_count: Arc::new(AtomicUsize::new(0)),
                shutdown_count: Arc::new(AtomicUsize::new(0)),
                selected_count: Arc::new(AtomicUsize::new(0)),
            },
            hooks: HookState::default(),
        }));
        let mut adapter = TextualAppAdapter::new(app, NoopWidget::new());
        let mut ctx = EventCtx::default();
        let quit = parse_action("app.quit").expect("quit action should parse");
        assert!(adapter.execute_action(&quit, &mut ctx));
        assert!(ctx.stop_requested());
        assert!(ctx.handled());
    }

    // =========================================================================
    // App-level reactive bridge tests (Work Item 2)
    // =========================================================================

    /// A TextualApp that exposes a `count` field with a manual reactive setter
    /// and records watcher calls in `watch_log`. Overrides `reactive_widget_mut`
    /// to enable dispatch.
    struct ReactiveTestApp {
        count: i32,
        watch_log: Vec<(i32, i32)>, // (old, new) per watcher call
    }

    impl ReactiveTestApp {
        fn new() -> Self {
            Self {
                count: 0,
                watch_log: Vec::new(),
            }
        }

        fn set_count(&mut self, new_val: i32, ctx: &mut ReactiveCtx) {
            use crate::reactive::ReactiveFlags;
            let old = self.count;
            self.count = new_val;
            ctx.record_change(
                "count",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(new_val),
            );
        }
    }

    impl TextualApp for ReactiveTestApp {
        fn compose(&mut self) -> AppRoot {
            AppRoot::new()
        }

        fn on_key_with_app(&mut self, app: &mut App, _key: &KeyEventData, _ctx: &mut EventCtx) {
            self.set_count(self.count + 1, app.reactive_ctx());
        }

        fn on_action_with_app(
            &mut self,
            app: &mut App,
            _action: Action,
            _ctx: &mut EventCtx,
        ) {
            self.set_count(self.count + 10, app.reactive_ctx());
        }

        fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, _ctx: &mut EventCtx) {
            self.set_count(self.count + 100, app.reactive_ctx());
        }

        fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
            // Simulate init: call setter so watcher fires once at mount.
            self.set_count(self.count, app.reactive_ctx());
        }

        fn reactive_widget_mut(&mut self) -> Option<&mut dyn ReactiveWidget> {
            Some(self)
        }
    }

    impl ReactiveWidget for ReactiveTestApp {
        fn reactive_dispatch(&mut self, changes: &[crate::reactive::ReactiveChange], _ctx: &mut ReactiveCtx) {
            for change in changes {
                if change.field_name == "count" {
                    if let (Some(&old), Some(&new)) = (
                        change.old_value.downcast_ref::<i32>(),
                        change.new_value.downcast_ref::<i32>(),
                    ) {
                        self.watch_log.push((old, new));
                    }
                }
            }
        }
    }

    #[test]
    fn app_reactive_bridge_setter_triggers_watcher_via_on_key() {
        let app_state = Arc::new(Mutex::new(ReactiveTestApp::new()));
        let mut adapter = TextualAppAdapter::new(app_state.clone(), NoopWidget::new());
        let mut runtime = App::new().expect("runtime init");
        let mut ctx = EventCtx::default();
        let key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));

        adapter.on_app_key(&mut runtime, &key, &mut ctx);

        let guard = app_state.lock().unwrap();
        assert_eq!(guard.count, 1, "setter should increment count to 1");
        assert_eq!(
            guard.watch_log,
            vec![(0, 1)],
            "watcher should record (old=0, new=1)"
        );
    }

    #[test]
    fn app_reactive_bridge_setter_triggers_watcher_via_on_action() {
        let app_state = Arc::new(Mutex::new(ReactiveTestApp::new()));
        let mut adapter = TextualAppAdapter::new(app_state.clone(), NoopWidget::new());
        let mut runtime = App::new().expect("runtime init");
        let mut ctx = EventCtx::default();

        adapter.on_app_action(&mut runtime, Action::HelpQuit, &mut ctx);

        let guard = app_state.lock().unwrap();
        assert_eq!(guard.count, 10);
        assert_eq!(guard.watch_log, vec![(0, 10)]);
    }

    #[test]
    fn app_reactive_bridge_setter_triggers_watcher_via_on_tick() {
        let app_state = Arc::new(Mutex::new(ReactiveTestApp::new()));
        let mut adapter = TextualAppAdapter::new(app_state.clone(), NoopWidget::new());
        let mut runtime = App::new().expect("runtime init");
        let mut ctx = EventCtx::default();

        adapter.on_app_tick(&mut runtime, 0, &mut ctx);

        let guard = app_state.lock().unwrap();
        assert_eq!(guard.count, 100);
        assert_eq!(guard.watch_log, vec![(0, 100)]);
    }

    #[test]
    fn app_reactive_bridge_init_dispatch_via_on_app_mount() {
        // Init dispatch: on_mount_with_app calls set_count(count, ctx) with old == new.
        // Watcher should record (0, 0) signalling init.
        let app_state = Arc::new(Mutex::new(ReactiveTestApp::new()));
        let mut adapter = TextualAppAdapter::new(app_state.clone(), NoopWidget::new());
        let mut runtime = App::new().expect("runtime init");
        let mut ctx = EventCtx::default();

        adapter.on_app_mount(&mut runtime, &mut ctx);

        let guard = app_state.lock().unwrap();
        assert_eq!(guard.count, 0, "count unchanged on init dispatch");
        assert_eq!(
            guard.watch_log,
            vec![(0, 0)],
            "watcher should record init (old==new)"
        );
    }

    #[test]
    fn app_reactive_bridge_repaint_requested_when_setter_flags_repaint() {
        // ReactiveFlags::reactive() sets repaint=true.  The adapter should
        // propagate that to EventCtx::request_repaint().
        let app_state = Arc::new(Mutex::new(ReactiveTestApp::new()));
        let mut adapter = TextualAppAdapter::new(app_state.clone(), NoopWidget::new());
        let mut runtime = App::new().expect("runtime init");
        let mut ctx = EventCtx::default();
        let key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));

        adapter.on_app_key(&mut runtime, &key, &mut ctx);

        assert!(
            ctx.repaint_requested(),
            "repaint should be requested when setter uses ReactiveFlags::reactive()"
        );
    }

    #[test]
    fn app_reactive_bridge_no_dispatch_when_reactive_widget_mut_returns_none() {
        // A TextualApp that does NOT override reactive_widget_mut returns None.
        // dispatch_app_reactive must not panic and should not fire any watcher.
        struct NoReactiveApp {
            key_count: usize,
        }

        impl TextualApp for NoReactiveApp {
            fn compose(&mut self) -> AppRoot {
                AppRoot::new()
            }

            fn on_key_with_app(
                &mut self,
                app: &mut App,
                _key: &KeyEventData,
                _ctx: &mut EventCtx,
            ) {
                use crate::reactive::ReactiveFlags;
                // Even if setter is called, reactive_widget_mut() == None means
                // no watcher dispatch — changes are just discarded.
                app.reactive_ctx().record_change(
                    "key_count",
                    ReactiveFlags::reactive(),
                    Box::new(self.key_count),
                    Box::new(self.key_count + 1),
                );
                self.key_count += 1;
            }
            // reactive_widget_mut() not overridden → returns None (default)
        }

        let app_state = Arc::new(Mutex::new(NoReactiveApp { key_count: 0 }));
        let mut adapter = TextualAppAdapter::new(app_state.clone(), NoopWidget::new());
        let mut runtime = App::new().expect("runtime init");
        let mut ctx = EventCtx::default();
        let key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));

        adapter.on_app_key(&mut runtime, &key, &mut ctx);

        assert_eq!(
            app_state.lock().unwrap().key_count,
            1,
            "key count still increments even without reactive dispatch"
        );
        // Repaint IS requested because the setter flags repaint via ReactiveFlags::reactive().
        // dispatch_app_reactive propagates setter flags regardless of reactive_widget_mut().
        assert!(ctx.repaint_requested(), "setter repaint flag propagated");
    }

    #[test]
    fn app_reactive_bridge_flags_reset_across_multiple_hook_calls() {
        // After on_app_key (which sets repaint), a subsequent on_app_tick with
        // no setter call should NOT re-trigger repaint.
        let app_state = Arc::new(Mutex::new(ReactiveTestApp::new()));
        let mut adapter = TextualAppAdapter::new(app_state.clone(), NoopWidget::new());
        let mut runtime = App::new().expect("runtime init");

        let mut key_ctx = EventCtx::default();
        let key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
        adapter.on_app_key(&mut runtime, &key, &mut key_ctx);
        assert!(key_ctx.repaint_requested(), "key handler should request repaint");

        // Tick with on_tick_with_app always calls set_count(+100), so watcher fires
        // and repaint is requested.
        let mut tick_ctx = EventCtx::default();
        adapter.on_app_tick(&mut runtime, 1, &mut tick_ctx);
        assert!(tick_ctx.repaint_requested(), "tick handler requests repaint via setter");

        // Verify flags were reset: count should be 101 (1 + 100), watch_log has 2 entries.
        let guard = app_state.lock().unwrap();
        assert_eq!(guard.count, 101);
        assert_eq!(guard.watch_log.len(), 2);
    }
}
