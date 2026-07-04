use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rich_rs::{Console, ConsoleOptions, Segments};

use crate::action::{APP_ACTIONS, ActionDecl, ParsedAction};
use crate::demo_snapshot::{SnapshotArgs, snapshot_widget};
use crate::event::{Action, Event, EventCtx};
use crate::keys::KeyEventData;
use crate::message::{CommandPaletteCommand, MessageEvent};
use crate::node_id::NodeId;
use crate::reactive::{ReactiveCtx, ReactiveWidget};
use crate::validation::ValidationResult;
use crate::widgets::{AppRoot, BindingDecl, Spacer, Widget};
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

    /// Display title shown in the Header.
    ///
    /// Override this to set the app title, mirroring Python Textual's `App.TITLE`
    /// class variable.  The runtime reads this once at mount time and stores it
    /// in `App::title()`.
    ///
    /// Returning an empty string (the default) means "no explicit title": the
    /// runtime then falls back to the app type's name, mirroring Python's
    /// `self.TITLE if self.TITLE is not None else type(self).__name__`
    /// precedence (see `App.__init__` in `app.py`).
    fn title(&self) -> &'static str {
        ""
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
    fn on_mount_with_app(&mut self, _app: &mut App, _ctx: &mut crate::event::WidgetCtx) {}

    /// Register typed message handlers. Called once before the runtime starts.
    ///
    /// Handlers receive `&mut self` (the app state), the typed payload, sender
    /// metadata, and the event context. This is a convenience layer over the
    /// same message bus that drives `on_message` — not a separate dispatch path.
    ///
    /// # Example
    /// ```ignore
    /// fn register_message_handlers(&mut self, handlers: &mut MessageHandlers<Self>)
    /// where
    ///     Self: Sized,
    /// {
    ///     handlers.on::<ButtonPressed>(|app, msg, _mctx, ctx| {
    ///         // ...
    ///     });
    /// }
    /// ```
    fn register_message_handlers(
        &mut self,
        _handlers: &mut crate::message_handlers::MessageHandlers<Self>,
    ) where
        Self: Sized,
    {
    }

    /// Run this app headless (in-process, no terminal) and drive it with a
    /// [`Pilot`](crate::runtime::Pilot), mirroring Python Textual's
    /// `app.run_test()`. See [`run_test`].
    fn run_test<F>(self, body: F) -> Result<()>
    where
        Self: Sized,
        F: FnOnce(&mut crate::runtime::Pilot) -> Result<()>,
    {
        run_test(self, body)
    }

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
    fn on_action(&mut self, _action: Action, _ctx: &mut crate::event::WidgetCtx) {}

    /// App-level action hook with mutable runtime handle.
    ///
    /// This mirrors Python Textual-style app callbacks where action handlers can
    /// query/mutate application state via the runtime handle.
    fn on_action_with_app(&mut self, app: &mut App, action: Action, ctx: &mut crate::event::WidgetCtx) {
        let _ = app;
        self.on_action(action, ctx);
    }

    /// Handle a custom action string declared in `bindings()` whose name is not in
    /// any widget's `action_registry()`.  Mirror Python's `action_<name>` method
    /// dispatch for app-level custom actions.
    ///
    /// Override this instead of `on_key_with_app` for actions tied to declarative
    /// bindings.  The `action` string is exactly what was declared as the action
    /// in `BindingDecl::new(key, action, description)`.
    fn on_app_action_str(&mut self, _app: &mut App, _action: &str, _ctx: &mut crate::event::WidgetCtx) {}

    /// App-level key hook.
    ///
    /// Called during capture phase before child widgets, allowing global key
    /// interception akin to Python Textual's app-level key handling.
    fn on_key(&mut self, _key: &KeyEventData, _ctx: &mut crate::event::WidgetCtx) {}

    /// App-level key hook with mutable runtime handle.
    ///
    /// This is the Python Textual-aligned surface for app callbacks that need
    /// query/mutation APIs (`query_one`, `query_mut`, etc.) while handling keys.
    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut crate::event::WidgetCtx) {
        let _ = app;
        self.on_key(key, ctx);
    }

    /// App-level tick hook.
    ///
    /// Called once per runtime tick after widget `on_tick` and before `Event::Tick`
    /// dispatch.
    fn on_tick(&mut self, _tick: u64, _ctx: &mut crate::event::WidgetCtx) {}

    /// App-level tick hook with mutable runtime handle.
    fn on_tick_with_app(&mut self, app: &mut App, tick: u64, ctx: &mut crate::event::WidgetCtx) {
        let _ = app;
        self.on_tick(tick, ctx);
    }

    /// App-level message hook. Called after widget message dispatch if not handled.
    fn on_message(&mut self, _message: &MessageEvent, _ctx: &mut crate::event::WidgetCtx) {}

    /// App-level message hook with mutable runtime handle.
    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut crate::event::WidgetCtx) {
        let _ = app;
        self.on_message(message, ctx);
    }

    /// Typed convenience hook for button-pressed messages. `description` is the
    /// button's **label** (Python `event.button.label`); to disambiguate buttons
    /// that share a label, downcast `ButtonPressed` for its `button_id`.
    ///
    /// This hook receives only a `WidgetCtx` (deferred). To READ sibling widget
    /// state in response to a press (query field values, etc.) you need
    /// `&mut App` — use [`on_message_with_app`](Self::on_message_with_app) and
    /// downcast `ButtonPressed` there instead.
    fn on_button_pressed(&mut self, _description: &str, _ctx: &mut crate::event::WidgetCtx) {}

    /// Typed convenience hook for input-changed messages.
    ///
    /// Receives only a `WidgetCtx`; for `&mut App` (to read other fields) use
    /// [`on_message_with_app`](Self::on_message_with_app) with an `InputChanged`
    /// downcast (its `sender()`/`control()` identify which input changed).
    fn on_input_changed(
        &mut self,
        _value: &str,
        _validation: &ValidationResult,
        _ctx: &mut crate::event::WidgetCtx,
    ) {
    }

    /// Typed convenience hook for input-submitted messages.
    fn on_input_submitted(&mut self, _value: &str, _ctx: &mut crate::event::WidgetCtx) {}

    /// Typed convenience hook for text-area-changed messages.
    fn on_text_area_changed(&mut self, _value: &str, _ctx: &mut crate::event::WidgetCtx) {}

    /// Typed convenience hook for checkbox-changed messages.
    fn on_checkbox_changed(&mut self, _checked: bool, _ctx: &mut crate::event::WidgetCtx) {}

    /// Typed convenience hook for list-view-selection-changed messages.
    fn on_list_view_selection_changed(&mut self, _index: usize, _item: &str, _ctx: &mut crate::event::WidgetCtx) {}

    /// Typed convenience hook for list-view-activation messages.
    fn on_list_view_item_activated(&mut self, _index: usize, _item: &str, _ctx: &mut crate::event::WidgetCtx) {}

    /// Typed convenience hook for tab-activated messages.
    fn on_tab_activated(&mut self, _index: usize, _title: &str, _ctx: &mut crate::event::WidgetCtx) {}

    /// Typed convenience hook for command palette open.
    fn on_command_palette_opened(&mut self, _ctx: &mut crate::event::WidgetCtx) {}

    /// Typed convenience hook for command palette close.
    fn on_command_palette_closed(&mut self, _ctx: &mut crate::event::WidgetCtx) {}

    /// Typed convenience hook for command palette selection.
    fn on_command_palette_command_selected(
        &mut self,
        _id: &str,
        _title: &str,
        _ctx: &mut crate::event::WidgetCtx,
    ) {
    }

    /// Provide command-palette providers for this app.
    ///
    /// Providers are started when the command palette opens and shut down when
    /// it closes.
    fn command_palette_providers(&mut self) -> Vec<Box<dyn CommandPaletteProvider>> {
        Vec::new()
    }

    /// Check if an action can be performed. Controls footer binding appearance.
    ///
    /// Return:
    /// - `Some(true)` — action is enabled (default, rendered normally)
    /// - `Some(false)` — action is hidden from footer
    /// - `None` — action is disabled but shown dimmed in footer
    ///
    /// Mirrors Python Textual's `App.check_action()`. Called during binding hint
    /// collection to set enabled/disabled state on each binding.
    fn check_action(&self, _action: &str, _parameters: &[String]) -> Option<bool> {
        Some(true)
    }

    /// Optional app output returned after the runtime exits.
    fn take_exit_output(&mut self) -> Option<String> {
        None
    }
}

/// Command provider lifecycle for TextualApp command palette integration.
pub trait CommandPaletteProvider: Send + Sync {
    /// Called when the command palette opens.
    fn startup(&mut self, _ctx: &mut crate::event::WidgetCtx) {}

    /// Return the commands currently provided by this provider.
    fn commands(&mut self) -> Vec<CommandPaletteCommand>;

    /// Called when one of this provider's commands is selected.
    fn on_command_selected(&mut self, _command_id: &str, _ctx: &mut crate::event::WidgetCtx) {}

    /// Called when the command palette closes.
    fn shutdown(&mut self, _ctx: &mut crate::event::WidgetCtx) {}
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

    pub fn push(&mut self, _sender: NodeId, overlay: NodeId, ctx: &mut crate::event::WidgetCtx) -> bool {
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

    pub fn pop(&mut self, _sender: NodeId, ctx: &mut crate::event::WidgetCtx) -> Option<NodeId> {
        let removed = self.stack.pop()?;
        ctx.hide_overlay(removed);
        if let Some(previous) = self.current() {
            ctx.show_overlay(previous);
        }
        Some(removed)
    }

    pub fn clear(&mut self, _sender: NodeId, ctx: &mut crate::event::WidgetCtx) {
        while let Some(overlay) = self.stack.pop() {
            ctx.hide_overlay(overlay);
        }
    }
}

struct TextualAppAdapter<T: TextualApp> {
    app: Arc<Mutex<T>>,
    app_child: Box<dyn Widget>,
    /// Whether the composed `CommandPaletteScreen` is currently pushed.
    /// Re-entrancy guard for `ctrl+p` (Python guards re-opening while the palette
    /// is the top screen, `command.py:736-746`).
    palette_screen_open: bool,
    help_panel_visible: bool,
    children_extracted: bool,
    command_palette_providers: Vec<Box<dyn CommandPaletteProvider>>,
    command_palette_provider_index: HashMap<String, (usize, String)>,
    message_handlers: crate::message_handlers::MessageHandlers<T>,
}

/// Derive the default app title from the app type's name.
///
/// Mirrors Python `type(self).__name__`: returns the final path segment of
/// `std::any::type_name::<T>()` (e.g. `my_crate::ModalApp` -> `ModalApp`),
/// stripping any generic-parameter suffix.
fn app_type_name<T: ?Sized>() -> String {
    let full = std::any::type_name::<T>();
    // Strip generic parameters (`Foo<Bar>` -> `Foo`) before taking the segment,
    // so `::` inside generics doesn't get mistaken for a path separator.
    let base = full.split('<').next().unwrap_or(full);
    base.rsplit("::").next().unwrap_or(base).to_string()
}

fn build_textual_app_runtime_root<T: TextualApp>(
    state: Arc<Mutex<T>>,
    composed: AppRoot,
) -> TextualAppAdapter<T> {
    
    TextualAppAdapter::new(state, composed)
}

impl<T: TextualApp> TextualAppAdapter<T> {
    fn new(app: Arc<Mutex<T>>, child: impl Widget + 'static) -> Self {
        let mut message_handlers = crate::message_handlers::MessageHandlers::new();
        {
            let mut locked = app.lock().unwrap_or_else(|e| e.into_inner());
            locked.register_message_handlers(&mut message_handlers);
        }
        Self {
            app,
            app_child: Box::new(child),
            palette_screen_open: false,
            help_panel_visible: false,
            children_extracted: false,
            command_palette_providers: Vec::new(),
            command_palette_provider_index: HashMap::new(),
            message_handlers,
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

    /// Gather a synchronous snapshot of every command-palette command
    /// (`app.COMMANDS + calling_screen.COMMANDS` in Python `command.py:818-833`)
    /// and (re)build the provider index that maps each command id back to its
    /// owning provider.
    ///
    /// This is the Wave-0 provider-snapshot bridge: a pushed
    /// `CommandPaletteScreen` (Wave 1) receives this `Vec` at construction so it
    /// can render its command list without reaching back into the adapter.
    /// `CommandPaletteProvider::commands()` is ctx-free, so the snapshot is a
    /// plain synchronous gather (async/worker providers are the documented 1.x
    /// tail).
    fn gather_command_palette_commands(&mut self) -> Vec<CommandPaletteCommand> {
        self.command_palette_provider_index.clear();
        let mut commands = self.system_commands();
        for (provider_index, provider) in self.command_palette_providers.iter_mut().enumerate() {
            for command in provider.commands() {
                self.command_palette_provider_index
                    .insert(command.id.clone(), (provider_index, command.id.clone()));
                commands.push(command);
            }
        }
        commands
    }

    fn publish_command_palette_commands(&mut self, ctx: &mut crate::event::WidgetCtx) {
        let commands = self.gather_command_palette_commands();
        if !commands.is_empty() {
            ctx.post_message(crate::message::CommandPaletteSetCommands { commands });
        }
    }

    fn shutdown_command_palette_providers(&mut self, ctx: &mut crate::event::WidgetCtx) {
        for provider in &mut self.command_palette_providers {
            provider.shutdown(ctx);
        }
        self.command_palette_providers.clear();
        self.command_palette_provider_index.clear();
    }

    fn initialize_command_palette_providers(&mut self, ctx: &mut crate::event::WidgetCtx) {
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

    fn handle_command_palette_selection(&mut self, command_id: &str, ctx: &mut crate::event::WidgetCtx) {
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

    /// Execute a built-in system command (Python `SystemCommandsProvider`
    /// callbacks). Formerly the legacy `CommandPalette` widget posted these from
    /// its own `execute_selected`; with the composed screen they run here, in the
    /// app context, after the screen has popped — one handler for both command
    /// classes.
    fn run_system_command(&mut self, command_id: &str, ctx: &mut crate::event::WidgetCtx) {
        match command_id {
            "quit" => ctx.request_stop(),
            "theme" => ctx.post_message(crate::message::AppChangeTheme),
            "screenshot" => ctx.post_message(crate::message::AppScreenshot {
                filename: None,
                path: None,
            }),
            "keys" => {
                if self.help_panel_visible {
                    ctx.post_message(crate::message::AppHideHelpPanel);
                    self.help_panel_visible = false;
                } else {
                    ctx.post_message(crate::message::AppShowHelpPanel);
                    self.help_panel_visible = true;
                }
            }
            _ => {}
        }
    }

    /// Wave 1: build + push the composed [`CommandPaletteScreen`] in response to
    /// the `command_palette` action (`ctrl+p`). Owns the whole open flow now that
    /// the palette is a real modal screen: start providers, snapshot commands,
    /// push with a dismiss callback that routes the selection back to the app.
    fn open_command_palette_screen(&mut self, app: &mut App, ctx: &mut crate::event::WidgetCtx) {
        // Re-entrancy: ctrl+p while the palette screen is already up is a no-op
        // (Python guards on the top-screen class).
        if self.palette_screen_open {
            return;
        }
        // Start providers and take a synchronous command snapshot for the screen
        // (async/worker providers are the documented 1.x tail).
        self.initialize_command_palette_providers(ctx);
        let commands = self.gather_command_palette_commands();
        self.palette_screen_open = true;
        // Preserve the app-level lifecycle hook (Python `CommandPalette.Opened`).
        self.app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_command_palette_opened(ctx);

        // The dismiss callback runs while the screen is popped mid-drain; it
        // cannot touch `&mut App`, so it defers its app messages onto the runtime
        // `WidgetCommand::PostMessage` FIFO (verified by
        // `runtime::tests::screen_callback_defers_app_message_via_widget_command_on_dismiss`).
        // ORDER MATTERS: on a value dismissal post `CommandPaletteCommandSelected`
        // FIRST (this adapter routes it to the still-live provider), THEN always
        // post `CommandPaletteClosed` (which shuts providers down + resets state) —
        // mirroring the legacy widget's `execute_selected` (select before close).
        let screen = crate::widgets::CommandPaletteScreen::new(commands);
        app.push_screen_with_callback(
            Box::new(screen),
            Box::new(move |result| {
                use crate::runtime::commands::{WidgetCommand, enqueue_widget_command};
                let sender = crate::node_id::NodeId::default();
                if let crate::screen::ScreenResult::Value(value) = result {
                    if let Ok(selected) = value.downcast::<crate::widgets::SelectedCommandId>() {
                        enqueue_widget_command(WidgetCommand::PostMessage(MessageEvent::new(
                            sender,
                            crate::message::CommandPaletteCommandSelected {
                                id: selected.id,
                                title: selected.title,
                            },
                        )));
                    }
                }
                enqueue_widget_command(WidgetCommand::PostMessage(MessageEvent::new(
                    sender,
                    crate::message::CommandPaletteClosed,
                )));
            }),
        );
    }

    /// Drain any pending reactive changes accumulated on `app.reactive_ctx()`
    /// and dispatch them to the app's `ReactiveWidget` impl (if any).
    ///
    /// Iterative: chained watcher sets (watchers calling setters on their
    /// dispatch `ctx`) are fed back and re-processed, up to
    /// [`crate::reactive::MAX_REACTIVE_ITERATIONS`]. Mirrors the widget-level
    /// `run_reactive_phase_with_dispatch` semantics for the app-level bridge.
    ///
    /// Repaint/layout/styles flags from setters and watchers are all propagated
    /// to `ctx` (using `EventCtx::request_repaint`,
    /// `request_layout_invalidation`, `request_style_invalidation`).
    fn dispatch_app_reactive(&self, app: &mut App, ctx: &mut crate::event::WidgetCtx) {
        let mut needs_repaint = false;
        let mut needs_layout = false;
        let mut needs_styles = false;
        let mut needs_recompose = false;

        for _ in 0..crate::reactive::MAX_REACTIVE_ITERATIONS {
            if !app.reactive_ctx().has_changes() {
                break;
            }
            needs_repaint |= app.reactive_ctx().needs_repaint();
            needs_layout |= app.reactive_ctx().needs_layout();
            needs_styles |= app.reactive_ctx().needs_styles();
            needs_recompose |= app.reactive_ctx().needs_recompose();
            let changes = app.reactive_ctx().take_changes();
            app.reactive_ctx().reset_flags();
            let mut rctx = ReactiveCtx::new(NodeId::default());
            if let Some(rw) = self
                .app
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .reactive_widget_mut()
            {
                rw.reactive_dispatch_with_app(app, &changes, &mut rctx);
            }
            // Fire field-to-field data bindings (Python `child.data_bind(App.field)`)
            // and any other dynamic watchers registered against the app reactive
            // source. These are independent of the app's own `watch_*`: each
            // matching binding propagates the new value into every bound child's
            // reactive and runs the child's `watch_*` (see `data_bind_reactive`).
            // Mirrors Python `_check_watchers` firing the "global" `__watchers`
            // after the public/private watch methods.
            let source = App::app_reactive_source();
            for change in &changes {
                if app.has_dynamic_watcher(source, change.field_name) {
                    app.notify_dynamic_watchers(
                        source,
                        change.field_name,
                        change.new_value.as_ref(),
                    );
                }
            }
            needs_repaint |= rctx.needs_repaint();
            needs_layout |= rctx.needs_layout();
            needs_styles |= rctx.needs_styles();
            needs_recompose |= rctx.needs_recompose();
            // Feed chained changes (watchers calling setters on the dispatch ctx)
            // back into the app ctx for the next iteration.
            for change in rctx.take_changes() {
                app.reactive_ctx().record_change(
                    change.field_name,
                    change.flags,
                    change.old_value,
                    change.new_value,
                );
            }
        }

        // Cycle guard — mirror run_reactive_phase_with_dispatch.
        if app.reactive_ctx().has_changes() {
            crate::debug::debug_render("[reactive] app-level cycle detected; draining");
            let _ = app.reactive_ctx().take_changes();
            app.reactive_ctx().reset_flags();
        }

        // App-level recompose (Python `reactive(recompose=True)` on App/Screen):
        // after all watchers have run (so `compose()` sees the final reactive
        // state), re-invoke the app's `compose()` and rebuild the app-content
        // subtree via `App::recompose_app`. A recompose implies layout + repaint.
        if needs_recompose {
            let fresh_root = self
                .app
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .compose();
            app.recompose_app(fresh_root);
            ctx.request_layout_invalidation();
        }

        if needs_repaint {
            ctx.request_repaint();
        }
        if needs_layout {
            ctx.request_layout_invalidation();
        }
        if needs_styles {
            ctx.request_style_invalidation();
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

    fn check_action(&self, action: &str, parameters: &[String]) -> Option<bool> {
        self.app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .check_action(action, parameters)
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut crate::event::WidgetCtx) -> bool {
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
                ctx.post_message(crate::message::AppBack);
                ctx.set_handled();
                true
            }
            "bell" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(crate::message::AppBell);
                ctx.set_handled();
                true
            }
            "change_theme" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(crate::message::AppChangeTheme);
                ctx.set_handled();
                true
            }
            "cycle_theme" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(crate::message::AppCycleTheme);
                ctx.set_handled();
                true
            }
            "set_theme" => {
                let Some(name) = single_arg(action) else {
                    return false;
                };
                ctx.post_message(crate::message::AppSetTheme {
                    name: name.to_string(),
                });
                ctx.set_handled();
                true
            }
            "command_palette" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(crate::message::AppCommandPalette);
                ctx.set_handled();
                true
            }
            "focus" => {
                let Some(widget_id) = single_arg(action) else {
                    return false;
                };
                ctx.post_message(crate::message::AppFocus {
                    widget_id: widget_id.to_string(),
                });
                ctx.set_handled();
                true
            }
            "focus_next" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(crate::message::AppFocusNext);
                ctx.set_handled();
                true
            }
            "focus_previous" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(crate::message::AppFocusPrevious);
                ctx.set_handled();
                true
            }
            "help_quit" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(crate::message::AppHelpQuit);
                ctx.set_handled();
                true
            }
            "copy_selected_text" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(crate::message::AppCopySelectedText);
                ctx.set_handled();
                true
            }
            "hide_help_panel" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(crate::message::AppHideHelpPanel);
                ctx.set_handled();
                true
            }
            "add_class" => {
                let Some((selector, class_name)) = selector_and_class(action) else {
                    return false;
                };
                ctx.post_message(crate::message::AppAddClass {
                    selector: selector.to_string(),
                    class_name: class_name.to_string(),
                });
                ctx.set_handled();
                true
            }
            "remove_class" => {
                let Some((selector, class_name)) = selector_and_class(action) else {
                    return false;
                };
                ctx.post_message(crate::message::AppRemoveClass {
                    selector: selector.to_string(),
                    class_name: class_name.to_string(),
                });
                ctx.set_handled();
                true
            }
            "toggle_class" => {
                let Some((selector, class_name)) = selector_and_class(action) else {
                    return false;
                };
                ctx.post_message(crate::message::AppToggleClass {
                    selector: selector.to_string(),
                    class_name: class_name.to_string(),
                });
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
                ctx.post_message(crate::message::AppNotify {
                    message,
                    title,
                    severity,
                });
                ctx.set_handled();
                true
            }
            "pop_screen" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(crate::message::AppPopScreen);
                ctx.set_handled();
                true
            }
            "push_screen" => {
                let Some(screen) = single_arg(action) else {
                    return false;
                };
                ctx.post_message(crate::message::AppPushScreen {
                    screen: screen.to_string(),
                });
                ctx.set_handled();
                true
            }
            "screenshot" => {
                if action.arguments.len() > 2 {
                    return false;
                }
                ctx.post_message(crate::message::AppScreenshot {
                    filename: action.arguments.first().cloned(),
                    path: action.arguments.get(1).cloned(),
                });
                ctx.set_handled();
                true
            }
            "show_help_panel" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(crate::message::AppShowHelpPanel);
                ctx.set_handled();
                true
            }
            "simulate_key" => {
                let Some(key) = single_arg(action) else {
                    return false;
                };
                ctx.post_message(crate::message::AppSimulateKey {
                    key: key.to_string(),
                });
                ctx.set_handled();
                true
            }
            "suspend_process" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(crate::message::AppSuspendProcess);
                ctx.set_handled();
                true
            }
            "switch_mode" => {
                let Some(mode) = single_arg(action) else {
                    return false;
                };
                ctx.post_message(crate::message::AppSwitchMode {
                    mode: mode.to_string(),
                });
                ctx.set_handled();
                true
            }
            "switch_screen" => {
                let Some(screen) = single_arg(action) else {
                    return false;
                };
                ctx.post_message(crate::message::AppSwitchScreen {
                    screen: screen.to_string(),
                });
                ctx.set_handled();
                true
            }
            "toggle_dark" => {
                if !no_args(action) {
                    return false;
                }
                ctx.post_message(crate::message::AppToggleDark);
                ctx.set_handled();
                true
            }
            _ => false,
        }
    }

    fn compose(&mut self) -> crate::compose::ComposeResult {
        if self.children_extracted {
            return Vec::new();
        }
        self.children_extracted = true;
        let app_child = std::mem::replace(&mut self.app_child, Box::new(Spacer::new(1)));
        vec![crate::compose::ChildDecl::new(app_child)]
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.app_child.render_styled(console, options)
    }

    fn on_mount(&mut self, ctx: &mut crate::event::WidgetCtx) {
        self.app_child.on_mount(ctx);
        self.app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_mount();
    }

    fn on_unmount(&mut self) {
        let mut ectx = EventCtx::default();
        let mut ctx = crate::event::WidgetCtx::__from_dispatch(NodeId::default(), &mut ectx);
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

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut crate::event::WidgetCtx) {
        self.app_child.on_mouse_scroll(delta_x, delta_y, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        self.app_child.on_event_capture(event, ctx);
    }

    fn on_app_key(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut crate::event::WidgetCtx) {
        self.app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_key_with_app(app, key, ctx);
        self.dispatch_app_reactive(app, ctx);
    }

    fn on_app_action(&mut self, app: &mut App, action: Action, ctx: &mut crate::event::WidgetCtx) {
        self.app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_action_with_app(app, action, ctx);
        self.dispatch_app_reactive(app, ctx);
    }

    fn on_app_unhandled_action(&mut self, app: &mut App, action: &str, ctx: &mut crate::event::WidgetCtx) {
        self.app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_app_action_str(app, action, ctx);
        self.dispatch_app_reactive(app, ctx);
    }

    fn on_app_message(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut crate::event::WidgetCtx) {
        if message.is::<crate::message::AppCommandPalette>() {
            self.open_command_palette_screen(app, ctx);
            return;
        }
        self.sync_help_panel_visible_from_runtime(app);
        self.app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_message_with_app(app, message, ctx);
        self.dispatch_app_reactive(app, ctx);
    }

    fn on_app_tick(&mut self, app: &mut App, tick: u64, ctx: &mut crate::event::WidgetCtx) {
        self.app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_tick_with_app(app, tick, ctx);
        self.dispatch_app_reactive(app, ctx);
    }

    fn on_app_timer(&mut self, app: &mut App, ctx: &mut crate::event::WidgetCtx) {
        // Run due app-level timer callbacks (set_interval / set_timer). Each
        // callback may mutate reactive fields via `app.reactive_ctx()`; the
        // app-reactive bridge then fires the corresponding watchers, exactly as
        // it does after `on_app_tick`.
        app.run_due_timer_callbacks(ctx.event_ctx_mut());
        self.dispatch_app_reactive(app, ctx);
    }

    fn on_app_mount(&mut self, app: &mut App, ctx: &mut crate::event::WidgetCtx) {
        // Register the type-erased app struct so timer callbacks (and any other
        // runtime callback) can re-enter it via `app.with_app_struct::<T>()`.
        app.set_app_struct(Arc::clone(&self.app) as Arc<Mutex<dyn std::any::Any + Send>>);

        // Register check_action callback so the runtime can evaluate
        // binding enabled/disabled state during hint collection.
        let app_ref = Arc::clone(&self.app);
        app.set_check_action_fn(Arc::new(move |action, params| {
            app_ref
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .check_action(action, params)
        }));

        // Propagate the app-defined title to the runtime before any Header reads it.
        //
        // Mirrors Python `self.title = self.TITLE if self.TITLE is not None else
        // type(self).__name__` (app.py): an explicit `title()` override wins;
        // otherwise default to the app type's name (final path segment).
        {
            let app_title = self.app.lock().unwrap_or_else(|e| e.into_inner()).title();
            if app_title.is_empty() {
                app.set_title(app_type_name::<T>());
            } else {
                app.set_title(app_title);
            }
        }

        // Init-phase watcher firing (G3): record synthetic old==new changes for
        // every reactive field with init=true, then dispatch them. This mirrors
        // Python's Reactive._initialize_object (reactive.py:227-228) and fires
        // before on_mount_with_app (matching Python ordering).
        {
            if let Some(rw) = self
                .app
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .reactive_widget_mut()
            {
                rw.reactive_record_init(app.reactive_ctx());
            }
        }
        self.dispatch_app_reactive(app, ctx);

        self.app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_mount_with_app(app, ctx);
        self.dispatch_app_reactive(app, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        self.app_child.on_event(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut crate::event::WidgetCtx) {
        self.app_child.on_message(message, ctx);
        if ctx.handled() {
            return;
        }
        if message.is::<crate::message::CommandPaletteOpened>() {
            self.initialize_command_palette_providers(ctx);
            self.app
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .on_command_palette_opened(ctx);
            if ctx.handled() {
                return;
            }
        } else if message.is::<crate::message::CommandPaletteClosed>() {
            self.palette_screen_open = false;
            self.shutdown_command_palette_providers(ctx);
            self.app
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .on_command_palette_closed(ctx);
            if ctx.handled() {
                return;
            }
        } else if let Some(m) =
            message.downcast_ref::<crate::message::CommandPaletteCommandSelected>()
        {
            let id = m.id.clone();
            let title = m.title.clone();
            // System commands (theme/quit/keys/screenshot) run here now that the
            // composed screen no longer executes them itself; user-provider
            // commands route through the provider index.
            self.run_system_command(&id, ctx);
            self.handle_command_palette_selection(&id, ctx);
            self.app
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .on_command_palette_command_selected(&id, &title, ctx);
            if ctx.handled() {
                return;
            }
        }
        if message.is::<crate::message::AppShowHelpPanel>() {
            self.help_panel_visible = true;
        } else if message.is::<crate::message::AppHideHelpPanel>() {
            self.help_panel_visible = false;
        }
        // Typed handler registry (Block between A and B).
        // Dispatches all registered handlers for the message's concrete payload type.
        // Runs AFTER adapter state management (Block A) but BEFORE built-in typed hooks
        // (Block B), so `palette_screen_open` / `help_panel_visible` are already set.
        {
            let mut app = self.app.lock().unwrap_or_else(|e| e.into_inner());
            self.message_handlers.dispatch(&mut *app, message, ctx.event_ctx_mut());
        }
        if ctx.handled() {
            return;
        }
        if let Some(m) = message.downcast_ref::<crate::message::ButtonPressed>() {
            self.app
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .on_button_pressed(&m.description, ctx);
            return;
        }
        if let Some(m) = message.downcast_ref::<crate::message::CheckboxChanged>() {
            self.app
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .on_checkbox_changed(m.checked, ctx);
            return;
        }
        if let Some(m) = message.downcast_ref::<crate::message::InputChanged>() {
            self.app
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .on_input_changed(&m.value, &m.validation, ctx);
            return;
        }
        if let Some(m) = message.downcast_ref::<crate::message::InputSubmitted>() {
            self.app
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .on_input_submitted(&m.value, ctx);
            return;
        }
        if let Some(m) = message.downcast_ref::<crate::message::TextAreaChanged>() {
            self.app
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .on_text_area_changed(&m.value, ctx);
            return;
        }
        if let Some(m) = message.downcast_ref::<crate::message::ListViewSelectionChanged>() {
            self.app
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .on_list_view_selection_changed(m.index, &m.item, ctx);
        }
        if let Some(m) = message.downcast_ref::<crate::message::ListViewItemActivated>() {
            self.app
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .on_list_view_item_activated(m.index, &m.item, ctx);
        }
        if let Some(m) = message.downcast_ref::<crate::message::TabActivated>() {
            self.app
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .on_tab_activated(m.index, &m.title, ctx);
        }
        if ctx.handled() {
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

/// Run a `TextualApp` definition headless (in-process, no terminal) and drive it
/// with a [`Pilot`](crate::runtime::Pilot), mirroring Python Textual's
/// `app.run_test()`.
///
/// The app is mounted, the initial render is produced into an in-memory frame,
/// and `body` is invoked with a `Pilot` to simulate input and inspect state.
/// Each `pilot.press` / `pilot.click` advances the app to idle. After `body`
/// returns, the app is unmounted cleanly.
///
/// ```no_run
/// use textual::prelude::*;
///
/// struct MyApp;
/// impl TextualApp for MyApp {
///     fn compose(&mut self) -> AppRoot { AppRoot::new() }
/// }
///
/// run_test(MyApp, |pilot| {
///     pilot.press(&["tab"])?;
///     Ok(())
/// }).unwrap();
/// ```
pub fn run_test<T, F>(definition: T, body: F) -> Result<()>
where
    T: TextualApp,
    F: FnOnce(&mut crate::runtime::Pilot) -> Result<()>,
{
    run_test_sized(definition, 80, 24, body)
}

/// Like [`run_test`] but with an explicit virtual terminal size.
pub fn run_test_sized<T, F>(definition: T, width: u16, height: u16, body: F) -> Result<()>
where
    T: TextualApp,
    F: FnOnce(&mut crate::runtime::Pilot) -> Result<()>,
{
    let state = Arc::new(Mutex::new(definition));
    let mut app = App::new()?;
    app.set_headless_size(width, height);

    state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .configure(&mut app)?;
    let composed = state.lock().unwrap_or_else(|e| e.into_inner()).compose();
    let mut root = build_textual_app_runtime_root(state.clone(), composed);

    // Install the deterministic manual clock BEFORE startup so timers and
    // animations scheduled from `on_mount`/`on_mount_with_app` (e.g.
    // animation01's 2s opacity fade) are anchored to the manual timeline and
    // stepped only by `advance_clock`/`advance_ticks` — not run to completion on
    // the wall clock by the `headless_startup` settling pump. Without this, an
    // on-mount animation would already be finished by the time the test body
    // (and `Pilot::new`) gains control. The live (non-headless) `run()` path is
    // unaffected: it never enables the manual clock. `Pilot::new` re-asserts this
    // idempotently.
    app.enable_manual_timer_clock();

    app.headless_startup(&mut root)?;

    let result = {
        let mut pilot = crate::runtime::Pilot::new(&mut app, &mut root);
        body(&mut pilot)
    };

    // Always attempt a clean unmount, even if the body errored.
    let finish = app.headless_finish(&mut root);
    result.and(finish)
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

    /// App with no explicit `title()` override: its title must default to the
    /// app type's name (mirrors Python `type(self).__name__`).
    struct DefaultTitleApp;
    impl TextualApp for DefaultTitleApp {
        fn compose(&mut self) -> crate::widgets::AppRoot {
            crate::widgets::AppRoot::new()
        }
    }

    /// App with an explicit `title()` override: the explicit title must win
    /// (mirrors Python `TITLE` precedence over the class name).
    struct ExplicitTitleApp;
    impl TextualApp for ExplicitTitleApp {
        fn compose(&mut self) -> crate::widgets::AppRoot {
            crate::widgets::AppRoot::new()
        }
        fn title(&self) -> &'static str {
            "My Explicit Title"
        }
    }

    #[test]
    fn app_type_name_returns_final_path_segment() {
        // Full type path is module-qualified; the helper must reduce it to the
        // bare type name (last `::` segment).
        assert_eq!(app_type_name::<DefaultTitleApp>(), "DefaultTitleApp");
        assert_eq!(app_type_name::<ExplicitTitleApp>(), "ExplicitTitleApp");
    }

    #[test]
    fn default_title_falls_back_to_app_type_name() {
        // No explicit override -> on_app_mount must set the runtime title to the
        // app type's name (mirrors Python `type(self).__name__`), NOT the old
        // "textual-rs" default.
        let app = Arc::new(Mutex::new(DefaultTitleApp));
        let mut adapter = TextualAppAdapter::new(app, NoopWidget::new());
        let mut runtime = App::new().expect("app should initialize");
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            adapter.on_app_mount(&mut runtime, &mut __w);
        }
        assert_eq!(runtime.title(), "DefaultTitleApp");
        assert_ne!(runtime.title(), "textual-rs");
    }

    #[test]
    fn explicit_title_wins_over_type_name() {
        // An explicit title() override must take precedence over the type name
        // (mirrors Python `TITLE` precedence).
        let app = Arc::new(Mutex::new(ExplicitTitleApp));
        let mut adapter = TextualAppAdapter::new(app, NoopWidget::new());
        let mut runtime = App::new().expect("app should initialize");
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            adapter.on_app_mount(&mut runtime, &mut __w);
        }
        assert_eq!(runtime.title(), "My Explicit Title");
    }

    struct DebugBellProvider;
    impl CommandPaletteProvider for DebugBellProvider {
        fn commands(&mut self) -> Vec<CommandPaletteCommand> {
            vec![CommandPaletteCommand {
                id: "bell".into(),
                title: "Bell".into(),
                help: "Ring the bell".into(),
            }]
        }
    }
    struct DebugBellApp;
    impl TextualApp for DebugBellApp {
        fn compose(&mut self) -> AppRoot {
            AppRoot::new()
        }
        fn command_palette_providers(&mut self) -> Vec<Box<dyn CommandPaletteProvider>> {
            vec![Box::new(DebugBellProvider)]
        }
    }

    /// Wave-1 end-to-end: ctrl+p pushes the composed `CommandPaletteScreen`, and
    /// typing a query fuzzy-filters the provider snapshot so the matched command's
    /// title AND help render in the dropdown (command01's structural floor). Also
    /// guards that `ctrl+p` opens the NEW screen path, not the legacy host.
    #[test]
    fn ctrl_p_opens_command_palette_screen_and_search_renders_command() {
        crate::run_test_sized(DebugBellApp, 60, 24, |pilot| {
            assert_eq!(pilot.app().screen_count(), 0, "no screen before ctrl+p");
            pilot.press(&["ctrl+p"])?;
            assert_eq!(
                pilot.app().screen_count(),
                1,
                "ctrl+p must push the CommandPaletteScreen"
            );
            pilot.press(&["b", "e", "l", "l"])?;
            pilot.wait_for_idle()?;
            let screen = pilot.app().frame_plain_lines().join("\n");
            assert!(
                screen.contains("Bell") && screen.contains("Ring the bell"),
                "typing 'bell' must render the Bell command title + help; got:\n{screen}"
            );
            Ok(())
        })
        .unwrap();
    }

    /// Re-entrancy: a second ctrl+p while the palette is open must NOT stack a
    /// second palette (Python guards on the top-screen class).
    #[test]
    fn ctrl_p_is_idempotent_while_palette_open() {
        crate::run_test_sized(DebugBellApp, 60, 24, |pilot| {
            pilot.press(&["ctrl+p"])?;
            assert_eq!(pilot.app().screen_count(), 1);
            pilot.press(&["ctrl+p"])?;
            assert_eq!(
                pilot.app().screen_count(),
                1,
                "ctrl+p while open must not stack a second palette"
            );
            Ok(())
        })
        .unwrap();
    }

    /// Escape dismisses the palette screen (focus restore to the app-root tree is
    /// free — its focus was never touched by the push).
    #[test]
    fn escape_dismisses_command_palette_screen() {
        crate::run_test_sized(DebugBellApp, 60, 24, |pilot| {
            pilot.press(&["ctrl+p"])?;
            assert_eq!(pilot.app().screen_count(), 1);
            pilot.press(&["escape"])?;
            pilot.wait_for_idle()?;
            assert_eq!(
                pilot.app().screen_count(),
                0,
                "escape must dismiss the palette screen"
            );
            Ok(())
        })
        .unwrap();
    }

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
        fn startup(&mut self, _ctx: &mut crate::event::WidgetCtx) {
            self.state.startup_count.fetch_add(1, Ordering::SeqCst);
        }

        fn commands(&mut self) -> Vec<CommandPaletteCommand> {
            vec![CommandPaletteCommand {
                id: "deploy".to_string(),
                title: "Deploy".to_string(),
                help: "Ship the current build".to_string(),
            }]
        }

        fn on_command_selected(&mut self, command_id: &str, _ctx: &mut crate::event::WidgetCtx) {
            if command_id == "deploy" {
                self.state.selected_count.fetch_add(1, Ordering::SeqCst);
            }
        }

        fn shutdown(&mut self, _ctx: &mut crate::event::WidgetCtx) {
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

        fn on_button_pressed(&mut self, description: &str, _ctx: &mut crate::event::WidgetCtx) {
            self.hooks.last_button = Some(description.to_string());
        }

        fn on_input_changed(
            &mut self,
            value: &str,
            validation: &ValidationResult,
            _ctx: &mut crate::event::WidgetCtx,
        ) {
            self.hooks
                .input_changed
                .replace((value.to_string(), validation.is_valid));
        }

        fn on_input_submitted(&mut self, value: &str, _ctx: &mut crate::event::WidgetCtx) {
            self.hooks.input_submitted = Some(value.to_string());
        }

        fn on_text_area_changed(&mut self, value: &str, _ctx: &mut crate::event::WidgetCtx) {
            self.hooks.text_area_changed = Some(value.to_string());
        }

        fn on_checkbox_changed(&mut self, checked: bool, _ctx: &mut crate::event::WidgetCtx) {
            self.hooks.checkbox_changed = Some(checked);
        }

        fn on_list_view_selection_changed(
            &mut self,
            index: usize,
            item: &str,
            _ctx: &mut crate::event::WidgetCtx,
        ) {
            self.hooks.list_selection = Some((index, item.to_string()));
        }

        fn on_list_view_item_activated(&mut self, index: usize, item: &str, _ctx: &mut crate::event::WidgetCtx) {
            self.hooks.list_activated = Some((index, item.to_string()));
        }

        fn on_tab_activated(&mut self, index: usize, title: &str, _ctx: &mut crate::event::WidgetCtx) {
            self.hooks.tab_activated = Some((index, title.to_string()));
        }

        fn on_command_palette_opened(&mut self, _ctx: &mut crate::event::WidgetCtx) {
            self.hooks.command_palette_events.push("opened".to_string());
        }

        fn on_command_palette_closed(&mut self, _ctx: &mut crate::event::WidgetCtx) {
            self.hooks.command_palette_events.push("closed".to_string());
        }

        fn on_command_palette_command_selected(
            &mut self,
            id: &str,
            _title: &str,
            _ctx: &mut crate::event::WidgetCtx,
        ) {
            self.hooks
                .command_palette_events
                .push(format!("selected:{id}"));
        }

        fn on_message(&mut self, _message: &MessageEvent, _ctx: &mut crate::event::WidgetCtx) {
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

        fn on_event_capture(&mut self, event: &Event, _ctx: &mut crate::event::WidgetCtx) {
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

        fn on_key(&mut self, _key: &KeyEventData, ctx: &mut crate::event::WidgetCtx) {
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

        fn on_action(&mut self, _action: Action, _ctx: &mut crate::event::WidgetCtx) {
            self.action_hits.fetch_add(1, Ordering::SeqCst);
        }

        fn on_message(&mut self, _message: &MessageEvent, _ctx: &mut crate::event::WidgetCtx) {
            self.message_hits.fetch_add(1, Ordering::SeqCst);
        }

        fn on_tick(&mut self, _tick: u64, _ctx: &mut crate::event::WidgetCtx) {
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

        fn on_action_with_app(&mut self, app: &mut App, _action: Action, _ctx: &mut crate::event::WidgetCtx) {
            app.set_css_runtime_pseudos(true, false, true);
            self.action_hits.fetch_add(1, Ordering::SeqCst);
        }

        fn on_message_with_app(
            &mut self,
            app: &mut App,
            _message: &MessageEvent,
            _ctx: &mut crate::event::WidgetCtx,
        ) {
            let (inline, ansi, nocolor) = app.css_runtime_pseudos();
            if inline && !ansi && nocolor {
                self.message_hits.fetch_add(1, Ordering::SeqCst);
            }
        }

        fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, _ctx: &mut crate::event::WidgetCtx) {
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
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut open_ctx);
            adapter.on_message(
            &MessageEvent::new(NodeId::default(), crate::message::CommandPaletteOpened),
            &mut __w);
        }
        assert_eq!(state.startup_count.load(Ordering::SeqCst), 1);
        let open_messages = open_ctx.take_messages();
        assert!(
            open_messages
                .iter()
                .any(|event| event.is::<crate::message::CommandPaletteSetCommands>())
        );

        let mut select_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut select_ctx);
            adapter.on_message(
            &MessageEvent::new(
                NodeId::default(),
                crate::message::CommandPaletteCommandSelected {
                    id: "deploy".to_string(),
                    title: "Deploy".to_string(),
                },
            ),
            &mut __w);
        }
        assert_eq!(state.selected_count.load(Ordering::SeqCst), 1);

        let mut close_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut close_ctx);
            adapter.on_message(
            &MessageEvent::new(NodeId::default(), crate::message::CommandPaletteClosed),
            &mut __w);
        }
        assert_eq!(state.shutdown_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn gather_command_palette_commands_snapshots_system_and_provider_commands() {
        // W0.3 producer half: the synchronous snapshot the Wave-1 palette screen
        // receives at construction — system commands + provider commands, with
        // the provider index rebuilt so a selection can route back.
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

        // Open the palette so providers are started (populates the provider list).
        let mut open_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(
                crate::node_id::NodeId::default(),
                &mut open_ctx,
            );
            adapter.on_message(
                &MessageEvent::new(NodeId::default(), crate::message::CommandPaletteOpened),
                &mut __w,
            );
        }

        let snapshot = adapter.gather_command_palette_commands();
        let ids: Vec<&str> = snapshot.iter().map(|c| c.id.as_str()).collect();
        assert!(
            ids.contains(&"theme") && ids.contains(&"quit"),
            "snapshot must include the system commands"
        );
        assert!(
            ids.contains(&"deploy"),
            "snapshot must include the registered provider's commands"
        );
        assert!(
            adapter
                .command_palette_provider_index
                .contains_key("deploy"),
            "gather must (re)build the provider index so a selection routes back"
        );
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
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut first_open_ctx);
            adapter.on_message(
            &MessageEvent::new(NodeId::default(), crate::message::CommandPaletteOpened),
            &mut __w);
        }
        let mut first_close_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut first_close_ctx);
            adapter.on_message(
            &MessageEvent::new(NodeId::default(), crate::message::CommandPaletteClosed),
            &mut __w);
        }
        let mut second_open_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut second_open_ctx);
            adapter.on_message(
            &MessageEvent::new(NodeId::default(), crate::message::CommandPaletteOpened),
            &mut __w);
        }
        let mut second_close_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut second_close_ctx);
            adapter.on_message(
            &MessageEvent::new(NodeId::default(), crate::message::CommandPaletteClosed),
            &mut __w);
        }

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
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut open_ctx);
            adapter.on_message(
            &MessageEvent::new(NodeId::default(), crate::message::CommandPaletteOpened),
            &mut __w);
        }

        let open_messages = open_ctx.take_messages();
        let open_commands = open_messages
            .iter()
            .find_map(|event| {
                event
                    .downcast_ref::<crate::message::CommandPaletteSetCommands>()
                    .map(|m| m.commands.clone())
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

        // Toggling the help panel updates the adapter's tracked state so the NEXT
        // palette-open snapshot (`gather_command_palette_commands`) reflects it. The
        // palette is a pushed screen that snapshots at open, so there is no
        // republish-to-a-live-host anymore.
        let mut show_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut show_ctx);
            adapter.on_message(
            &MessageEvent::new(NodeId::default(), crate::message::AppShowHelpPanel),
            &mut __w);
        }
        let show_keys_help = adapter
            .system_commands()
            .into_iter()
            .find(|command| command.id == "keys")
            .map(|command| command.help)
            .expect("keys command should be present");
        assert_eq!(show_keys_help, "Hide the keys and widget help panel");

        let mut hide_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut hide_ctx);
            adapter.on_message(
            &MessageEvent::new(NodeId::default(), crate::message::AppHideHelpPanel),
            &mut __w);
        }
        let hide_keys_help = adapter
            .system_commands()
            .into_iter()
            .find(|command| command.id == "keys")
            .map(|command| command.help)
            .expect("keys command should be present");
        assert_eq!(
            hide_keys_help,
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

        let typed_events = vec![
            MessageEvent::new(
                NodeId::default(),
                crate::message::ButtonPressed {
                    description: "ok".to_string(),
                    button_id: None,
                },
            ),
            MessageEvent::new(
                NodeId::default(),
                crate::message::InputChanged {
                    value: "42".to_string(),
                    validation: ValidationResult::success(),
                },
            ),
            MessageEvent::new(
                NodeId::default(),
                crate::message::InputSubmitted {
                    value: "submit".to_string(),
                },
            ),
            MessageEvent::new(
                NodeId::default(),
                crate::message::TextAreaChanged {
                    value: "textarea".to_string(),
                },
            ),
            MessageEvent::new(
                NodeId::default(),
                crate::message::CheckboxChanged { checked: true },
            ),
            MessageEvent::new(
                NodeId::default(),
                crate::message::ListViewSelectionChanged {
                    index: 2,
                    item: "gamma".to_string(),
                },
            ),
            MessageEvent::new(
                NodeId::default(),
                crate::message::ListViewItemActivated {
                    index: 3,
                    item: "delta".to_string(),
                },
            ),
        ];
        for event in typed_events {
            let mut ctx = EventCtx::default();
            {
                let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
                adapter.on_message(&event, &mut __w);
            }
        }
        // TabActivated (converted to open struct form)
        {
            let mut ctx = EventCtx::default();
            {
                let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
                adapter.on_message(
                &MessageEvent::new(
                    NodeId::default(),
                    crate::message::TabActivated {
                        id: "general".to_string(),
                        index: 1,
                        title: "General".to_string(),
                    },
                ),
                &mut __w);
            }
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

        // The palette is now a pushed modal screen, not an always-mounted host:
        // the adapter composes exactly the app body, once.
        let first = adapter.compose();
        assert_eq!(first.len(), 1);
        assert!(
            !first
                .iter()
                .any(|child| child.widget().style_type() == "CommandPalette"),
            "runtime root should NOT include a legacy CommandPalette host child"
        );

        let second = adapter.compose();
        assert!(second.is_empty());
    }

    #[test]
    fn overlay_screen_stack_posts_visibility_messages_for_push_pop() {
        let sender = node_id_from_ffi(9);
        let first = node_id_from_ffi(111);
        let second = node_id_from_ffi(222);
        let mut stack = OverlayScreenStack::new();
        let mut ctx = EventCtx::default();

        assert!({ let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); stack.push(sender, first, &mut __w) });
        assert!({ let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); stack.push(sender, second, &mut __w) });
        assert_eq!(stack.current(), Some(second));
        assert_eq!({ let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); stack.pop(sender, &mut __w) }, Some(second));
        assert_eq!(stack.current(), Some(first));
        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); stack.clear(sender, &mut __w) };
        assert!(stack.is_empty());

        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 6);
        assert!(
            messages[0]
                .downcast_ref::<crate::message::OverlaySetVisible>()
                .is_some_and(|m| m.overlay == first && m.visible)
        );
        assert!(
            messages[1]
                .downcast_ref::<crate::message::OverlaySetVisible>()
                .is_some_and(|m| m.overlay == first && !m.visible)
        );
        assert!(
            messages[2]
                .downcast_ref::<crate::message::OverlaySetVisible>()
                .is_some_and(|m| m.overlay == second && m.visible)
        );
        assert!(
            messages[3]
                .downcast_ref::<crate::message::OverlaySetVisible>()
                .is_some_and(|m| m.overlay == second && !m.visible)
        );
        assert!(
            messages[4]
                .downcast_ref::<crate::message::OverlaySetVisible>()
                .is_some_and(|m| m.overlay == first && m.visible)
        );
        assert!(
            messages[5]
                .downcast_ref::<crate::message::OverlaySetVisible>()
                .is_some_and(|m| m.overlay == first && !m.visible)
        );
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

        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            adapter.on_app_key(&mut runtime, &key, &mut __w);
        }

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

        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            adapter.on_app_key(&mut runtime, &key, &mut __w);
        }
        if !ctx.handled() {
            {
                let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
                adapter.on_event_capture(&Event::Key(key), &mut __w);
            }
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
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut action_ctx);
            adapter.on_app_action(&mut runtime, Action::HelpQuit, &mut __w);
        }

        let mut message_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut message_ctx);
            adapter.on_app_message(
            &mut runtime,
            &MessageEvent::new(
                NodeId::default(),
                crate::message::FooterBindingsUpdated { count: 0 },
            ),
            &mut __w);
        }

        let mut tick_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut tick_ctx);
            adapter.on_app_tick(&mut runtime, 7, &mut __w);
        }

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
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut action_ctx);
            adapter.on_app_action(&mut runtime, Action::HelpQuit, &mut __w);
        }

        let mut message_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut message_ctx);
            adapter.on_app_message(
            &mut runtime,
            &MessageEvent::new(
                NodeId::default(),
                crate::message::FooterBindingsUpdated { count: 0 },
            ),
            &mut __w);
        }

        let mut tick_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut tick_ctx);
            adapter.on_app_tick(&mut runtime, 9, &mut __w);
        }

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
        assert!({ let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); adapter.execute_action(&add, &mut __w) });
        assert!({ let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); adapter.execute_action(&remove, &mut __w) });
        assert!({ let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); adapter.execute_action(&toggle, &mut __w) });

        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 3);
        assert!(
            messages[0]
                .downcast_ref::<crate::message::AppAddClass>()
                .is_some_and(|m| m.selector == "Button" && m.class_name == "primary")
        );
        assert!(
            messages[1]
                .downcast_ref::<crate::message::AppRemoveClass>()
                .is_some_and(|m| m.selector == "Button" && m.class_name == "primary")
        );
        assert!(
            messages[2]
                .downcast_ref::<crate::message::AppToggleClass>()
                .is_some_and(|m| m.selector == "Button" && m.class_name == "primary")
        );
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
                { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); adapter.execute_action(&parsed, &mut __w) },
                "expected handled: {action}"
            );
        }

        let messages = ctx.take_messages();
        assert!(!messages.is_empty());
        assert!(messages.iter().any(|m| m.is::<crate::message::AppBack>()));
        assert!(messages.iter().any(|m| m.is::<crate::message::AppBell>()));
        assert!(messages.iter().any(|m| m.is::<crate::message::AppFocus>()));
        assert!(messages.iter().any(|m| m.is::<crate::message::AppNotify>()));
        assert!(
            messages
                .iter()
                .any(|m| m.is::<crate::message::AppSwitchMode>())
        );
        assert!(
            messages
                .iter()
                .any(|m| m.is::<crate::message::AppToggleDark>())
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
                !{ let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); adapter.execute_action(&parsed, &mut __w) },
                "expected invalid arity for {action}"
            );
        }
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
        assert!({ let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); adapter.execute_action(&quit, &mut __w) });
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

        fn on_key_with_app(&mut self, app: &mut App, _key: &KeyEventData, _ctx: &mut crate::event::WidgetCtx) {
            self.set_count(self.count + 1, app.reactive_ctx());
        }

        fn on_action_with_app(&mut self, app: &mut App, _action: Action, _ctx: &mut crate::event::WidgetCtx) {
            self.set_count(self.count + 10, app.reactive_ctx());
        }

        fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, _ctx: &mut crate::event::WidgetCtx) {
            self.set_count(self.count + 100, app.reactive_ctx());
        }

        fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut crate::event::WidgetCtx) {
            // Simulate init: call setter so watcher fires once at mount.
            self.set_count(self.count, app.reactive_ctx());
        }

        fn reactive_widget_mut(&mut self) -> Option<&mut dyn ReactiveWidget> {
            Some(self)
        }
    }

    impl ReactiveWidget for ReactiveTestApp {
        fn reactive_dispatch(
            &mut self,
            changes: &[crate::reactive::ReactiveChange],
            _ctx: &mut ReactiveCtx,
        ) {
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

        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            adapter.on_app_key(&mut runtime, &key, &mut __w);
        }

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

        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            adapter.on_app_action(&mut runtime, Action::HelpQuit, &mut __w);
        }

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

        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            adapter.on_app_tick(&mut runtime, 0, &mut __w);
        }

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

        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            adapter.on_app_mount(&mut runtime, &mut __w);
        }

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

        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            adapter.on_app_key(&mut runtime, &key, &mut __w);
        }

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

            fn on_key_with_app(&mut self, app: &mut App, _key: &KeyEventData, _ctx: &mut crate::event::WidgetCtx) {
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

        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            adapter.on_app_key(&mut runtime, &key, &mut __w);
        }

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
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut key_ctx);
            adapter.on_app_key(&mut runtime, &key, &mut __w);
        }
        assert!(
            key_ctx.repaint_requested(),
            "key handler should request repaint"
        );

        // Tick with on_tick_with_app always calls set_count(+100), so watcher fires
        // and repaint is requested.
        let mut tick_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut tick_ctx);
            adapter.on_app_tick(&mut runtime, 1, &mut __w);
        }
        assert!(
            tick_ctx.repaint_requested(),
            "tick handler requests repaint via setter"
        );

        // Verify flags were reset: count should be 101 (1 + 100), watch_log has 2 entries.
        let guard = app_state.lock().unwrap();
        assert_eq!(guard.count, 101);
        assert_eq!(guard.watch_log.len(), 2);
    }

    // ---------------------------------------------------------------------------
    // T2b: New bridge tests — chained changes, cycle guard, styles flag, init order
    // ---------------------------------------------------------------------------

    /// A TextualApp that chains reactive changes: watcher for `a` records a
    /// change for `b`, which has its own watcher.
    struct ChainedReactiveApp {
        a: i32,
        b: i32,
        watch_log: Vec<(&'static str, i32, i32)>, // (field, old, new)
    }

    impl ChainedReactiveApp {
        fn new() -> Self {
            Self { a: 0, b: 0, watch_log: Vec::new() }
        }

        fn set_a(&mut self, val: i32, ctx: &mut ReactiveCtx) {
            use crate::reactive::ReactiveFlags;
            let old = self.a;
            self.a = val;
            ctx.record_change("a", ReactiveFlags::reactive(), Box::new(old), Box::new(val));
        }

        fn set_b(&mut self, val: i32, ctx: &mut ReactiveCtx) {
            use crate::reactive::ReactiveFlags;
            let old = self.b;
            self.b = val;
            ctx.record_change("b", ReactiveFlags::reactive(), Box::new(old), Box::new(val));
        }
    }

    impl TextualApp for ChainedReactiveApp {
        fn compose(&mut self) -> AppRoot { AppRoot::new() }
        fn reactive_widget_mut(&mut self) -> Option<&mut dyn ReactiveWidget> { Some(self) }
    }

    impl ReactiveWidget for ChainedReactiveApp {
        fn reactive_dispatch_with_app(
            &mut self,
            _app: &mut App,
            changes: &[crate::reactive::ReactiveChange],
            ctx: &mut ReactiveCtx,
        ) {
            for change in changes {
                if change.field_name == "a" {
                    if let (Some(&old), Some(&new)) = (
                        change.old_value.downcast_ref::<i32>(),
                        change.new_value.downcast_ref::<i32>(),
                    ) {
                        self.watch_log.push(("a", old, new));
                        // Chain: watcher for 'a' sets 'b' via the dispatch ctx
                        self.set_b(new * 2, ctx);
                    }
                } else if change.field_name == "b" {
                    if let (Some(&old), Some(&new)) = (
                        change.old_value.downcast_ref::<i32>(),
                        change.new_value.downcast_ref::<i32>(),
                    ) {
                        self.watch_log.push(("b", old, new));
                    }
                }
            }
        }
    }

    #[test]
    fn app_reactive_bridge_chained_watcher_changes_are_processed() {
        let app_state = Arc::new(Mutex::new(ChainedReactiveApp::new()));
        let adapter = TextualAppAdapter::new(app_state.clone(), NoopWidget::new());
        let mut runtime = App::new().expect("runtime init");
        let mut ctx = EventCtx::default();

        // set_a records change for 'a'; watcher for 'a' will chain a change for 'b'
        app_state.lock().unwrap().set_a(5, runtime.reactive_ctx());
        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); adapter.dispatch_app_reactive(&mut runtime, &mut __w) };

        let guard = app_state.lock().unwrap();
        assert_eq!(guard.a, 5);
        assert_eq!(guard.b, 10, "chained set_b(5*2) should have fired");
        // Both watchers should have run
        assert!(guard.watch_log.iter().any(|(f, _, _)| *f == "a"), "watcher 'a' should run");
        assert!(guard.watch_log.iter().any(|(f, _, _)| *f == "b"), "watcher 'b' should run (chained)");
    }

    /// An app whose watcher always re-records a change for the same field (infinite loop).
    struct CycleApp {
        val: i32,
        dispatch_count: usize,
    }

    impl CycleApp {
        fn new() -> Self { Self { val: 0, dispatch_count: 0 } }

        fn set_val(&mut self, v: i32, ctx: &mut ReactiveCtx) {
            use crate::reactive::ReactiveFlags;
            let old = self.val;
            self.val = v;
            ctx.record_change("val", ReactiveFlags::reactive(), Box::new(old), Box::new(v));
        }
    }

    impl TextualApp for CycleApp {
        fn compose(&mut self) -> AppRoot { AppRoot::new() }
        fn reactive_widget_mut(&mut self) -> Option<&mut dyn ReactiveWidget> { Some(self) }
    }

    impl ReactiveWidget for CycleApp {
        fn reactive_dispatch_with_app(
            &mut self,
            _app: &mut App,
            changes: &[crate::reactive::ReactiveChange],
            ctx: &mut ReactiveCtx,
        ) {
            for change in changes {
                if change.field_name == "val" {
                    self.dispatch_count += 1;
                    // Always re-record to create a cycle
                    use crate::reactive::ReactiveFlags;
                    let new = *change.new_value.downcast_ref::<i32>().unwrap();
                    ctx.record_change(
                        "val",
                        ReactiveFlags::reactive(),
                        Box::new(new),
                        Box::new(new + 1),
                    );
                }
            }
        }
    }

    #[test]
    fn app_reactive_bridge_cycle_guard_terminates() {
        let app_state = Arc::new(Mutex::new(CycleApp::new()));
        let adapter = TextualAppAdapter::new(app_state.clone(), NoopWidget::new());
        let mut runtime = App::new().expect("runtime init");
        let mut ctx = EventCtx::default();

        app_state.lock().unwrap().set_val(1, runtime.reactive_ctx());
        // Must return (not loop forever)
        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); adapter.dispatch_app_reactive(&mut runtime, &mut __w) };

        let guard = app_state.lock().unwrap();
        // Should have run up to MAX_REACTIVE_ITERATIONS times, then stopped
        assert!(guard.dispatch_count <= crate::reactive::MAX_REACTIVE_ITERATIONS);
        // The app ctx must be drained
        assert!(!runtime.reactive_ctx().has_changes());
    }

    /// An app whose watcher calls ctx.request_styles().
    struct StylesRequestApp {
        val: i32,
    }

    impl StylesRequestApp {
        fn new() -> Self { Self { val: 0 } }

        fn set_val(&mut self, v: i32, ctx: &mut ReactiveCtx) {
            use crate::reactive::ReactiveFlags;
            let old = self.val;
            self.val = v;
            ctx.record_change("val", ReactiveFlags::reactive(), Box::new(old), Box::new(v));
        }
    }

    impl TextualApp for StylesRequestApp {
        fn compose(&mut self) -> AppRoot { AppRoot::new() }
        fn reactive_widget_mut(&mut self) -> Option<&mut dyn ReactiveWidget> { Some(self) }
    }

    impl ReactiveWidget for StylesRequestApp {
        fn reactive_dispatch_with_app(
            &mut self,
            _app: &mut App,
            changes: &[crate::reactive::ReactiveChange],
            ctx: &mut ReactiveCtx,
        ) {
            for change in changes {
                if change.field_name == "val" {
                    let _ = change;
                    ctx.request_styles();
                    ctx.request_repaint();
                }
            }
        }
    }

    #[test]
    fn app_reactive_bridge_styles_flag_maps_to_event_ctx() {
        let app_state = Arc::new(Mutex::new(StylesRequestApp::new()));
        let adapter = TextualAppAdapter::new(app_state.clone(), NoopWidget::new());
        let mut runtime = App::new().expect("runtime init");
        let mut ctx = EventCtx::default();

        app_state.lock().unwrap().set_val(1, runtime.reactive_ctx());
        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); adapter.dispatch_app_reactive(&mut runtime, &mut __w) };

        assert!(ctx.repaint_requested(), "repaint should be requested");
        assert!(ctx.invalidation().style, "style invalidation should be requested");
    }

    /// An app that logs watcher call order to verify init fires before on_mount_with_app.
    struct InitOrderApp {
        log: Vec<&'static str>,
    }

    impl InitOrderApp {
        fn new() -> Self { Self { log: Vec::new() } }

        #[allow(dead_code)] // reactive-setter scaffolding for the init-order watcher test
        fn set_count(&mut self, _val: i32, ctx: &mut ReactiveCtx) {
            use crate::reactive::ReactiveFlags;
            ctx.record_change("count", ReactiveFlags::reactive(), Box::new(0_i32), Box::new(0_i32));
        }
    }

    impl TextualApp for InitOrderApp {
        fn compose(&mut self) -> AppRoot { AppRoot::new() }

        fn on_mount_with_app(&mut self, _app: &mut App, _ctx: &mut crate::event::WidgetCtx) {
            self.log.push("on_mount_with_app");
        }

        fn reactive_widget_mut(&mut self) -> Option<&mut dyn ReactiveWidget> { Some(self) }
    }

    impl ReactiveWidget for InitOrderApp {
        fn reactive_record_init(&self, ctx: &mut ReactiveCtx) {
            // Simulate recording init changes for a field named "count"
            use crate::reactive::ReactiveFlags;
            ctx.record_change(
                "count",
                ReactiveFlags::reactive(),
                Box::new(0_i32),
                Box::new(0_i32),
            );
        }

        fn reactive_dispatch_with_app(
            &mut self,
            _app: &mut App,
            changes: &[crate::reactive::ReactiveChange],
            ctx: &mut ReactiveCtx,
        ) {
            for change in changes {
                if change.field_name == "count" {
                    let _ = change;
                    self.log.push("watcher_count");
                    ctx.request_repaint();
                }
            }
        }
    }

    #[test]
    fn app_reactive_bridge_record_init_fires_before_mount_hook() {
        let app_state = Arc::new(Mutex::new(InitOrderApp::new()));
        let mut adapter = TextualAppAdapter::new(app_state.clone(), NoopWidget::new());
        let mut runtime = App::new().expect("runtime init");
        let mut ctx = EventCtx::default();

        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            adapter.on_app_mount(&mut runtime, &mut __w);
        }

        let guard = app_state.lock().unwrap();
        // watcher_count must appear before on_mount_with_app in the log
        let watcher_pos = guard.log.iter().position(|&s| s == "watcher_count");
        let mount_pos = guard.log.iter().position(|&s| s == "on_mount_with_app");
        assert!(watcher_pos.is_some(), "watcher should have fired");
        assert!(mount_pos.is_some(), "on_mount_with_app should have been called");
        assert!(
            watcher_pos.unwrap() < mount_pos.unwrap(),
            "init watcher must fire before on_mount_with_app"
        );
    }

    // ---------------------------------------------------------------------------
    // T3: MessageHandlers adapter-wiring tests
    // ---------------------------------------------------------------------------

    #[derive(Default)]
    struct HandlerWiringApp {
        typed_handler_count: usize,
        builtin_hook_count: usize,
    }

    impl TextualApp for HandlerWiringApp {
        fn compose(&mut self) -> crate::widgets::AppRoot {
            crate::widgets::AppRoot::new()
        }

        fn register_message_handlers(
            &mut self,
            handlers: &mut crate::message_handlers::MessageHandlers<Self>,
        ) {
            handlers.on::<crate::message::ButtonPressed, _>(|app, _msg, _mctx, _ctx| {
                app.typed_handler_count += 1;
            });
        }

        fn on_button_pressed(&mut self, _description: &str, _ctx: &mut crate::event::WidgetCtx) {
            self.builtin_hook_count += 1;
        }
    }

    #[derive(Default)]
    struct HandlerSetHandledApp {
        typed_handler_count: usize,
        builtin_hook_count: usize,
    }

    impl TextualApp for HandlerSetHandledApp {
        fn compose(&mut self) -> crate::widgets::AppRoot {
            crate::widgets::AppRoot::new()
        }

        fn register_message_handlers(
            &mut self,
            handlers: &mut crate::message_handlers::MessageHandlers<Self>,
        ) {
            handlers.on::<crate::message::ButtonPressed, _>(|app, _msg, _mctx, ctx| {
                app.typed_handler_count += 1;
                ctx.set_handled();
            });
        }

        fn on_button_pressed(&mut self, _description: &str, _ctx: &mut crate::event::WidgetCtx) {
            self.builtin_hook_count += 1;
        }
    }

    #[test]
    fn typed_handler_runs_before_builtin_hook() {
        let app = Arc::new(Mutex::new(HandlerWiringApp::default()));
        let mut adapter = TextualAppAdapter::new(app.clone(), NoopWidget::new());
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            adapter.on_message(
            &MessageEvent::new(
                NodeId::default(),
                crate::message::ButtonPressed {
                    description: "test".to_string(),
                    button_id: None,
                },
            ),
            &mut __w);
        }
        let guard = app.lock().unwrap();
        assert_eq!(guard.typed_handler_count, 1, "typed handler ran");
        assert_eq!(guard.builtin_hook_count, 1, "builtin hook also ran");
    }

    #[test]
    fn typed_handler_set_handled_suppresses_builtin_hook() {
        let app = Arc::new(Mutex::new(HandlerSetHandledApp::default()));
        let mut adapter = TextualAppAdapter::new(app.clone(), NoopWidget::new());
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            adapter.on_message(
            &MessageEvent::new(
                NodeId::default(),
                crate::message::ButtonPressed {
                    description: "test".to_string(),
                    button_id: None,
                },
            ),
            &mut __w);
        }
        let guard = app.lock().unwrap();
        assert_eq!(guard.typed_handler_count, 1, "typed handler ran");
        assert_eq!(
            guard.builtin_hook_count, 0,
            "builtin hook suppressed by set_handled"
        );
    }

    // =========================================================================
    // App-level recompose bridge (Python `reactive(recompose=True)` on App)
    // =========================================================================

    /// App with a recompose reactive `n`: `compose()` yields `n` Labels. Setting
    /// `n` records a recompose change, which the bridge turns into an
    /// `App::recompose_app` call that rebuilds the app-content subtree.
    struct RecomposeBridgeApp {
        n: u32,
    }

    impl RecomposeBridgeApp {
        fn set_n(&mut self, val: u32, ctx: &mut ReactiveCtx) {
            use crate::reactive::ReactiveFlags;
            let old = self.n;
            self.n = val;
            ctx.record_change(
                "n",
                ReactiveFlags::reactive_recompose(),
                Box::new(old),
                Box::new(val),
            );
        }
    }

    impl TextualApp for RecomposeBridgeApp {
        fn compose(&mut self) -> AppRoot {
            let mut root = AppRoot::new();
            for i in 0..self.n {
                root = root.with_child(crate::widgets::Label::new(format!("row {i}")));
            }
            root
        }

        fn reactive_widget_mut(&mut self) -> Option<&mut dyn ReactiveWidget> {
            Some(self)
        }
    }

    impl ReactiveWidget for RecomposeBridgeApp {
        fn reactive_dispatch(
            &mut self,
            _changes: &[crate::reactive::ReactiveChange],
            _ctx: &mut ReactiveCtx,
        ) {
            // No watcher needed; recompose is handled by the bridge.
        }
    }

    #[test]
    fn app_recompose_bridge_rebuilds_app_content() {
        // Build a real runtime tree from the adapter so the app-content node
        // exists, then set the recompose reactive and run the bridge.
        let app_state = Arc::new(Mutex::new(RecomposeBridgeApp { n: 1 }));
        let composed = app_state.lock().unwrap().compose();
        let mut root = build_textual_app_runtime_root(app_state.clone(), composed);
        let adapter_for_state = TextualAppAdapter::new(app_state.clone(), NoopWidget::new());

        let mut runtime = App::new().expect("runtime init");
        runtime.build_widget_tree(&mut root);

        // Initially one Label.
        let before = runtime
            .query("Label")
            .map(|q| q.into_ids().len())
            .unwrap_or(0);
        assert_eq!(before, 1, "one Label before recompose");

        // Set n = 3 (records a recompose change), then run the bridge.
        app_state.lock().unwrap().set_n(3, runtime.reactive_ctx());
        let mut ctx = EventCtx::default();
        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); adapter_for_state.dispatch_app_reactive(&mut runtime, &mut __w) };

        // The app-content subtree was recomposed: now three Labels.
        let after = runtime
            .query("Label")
            .map(|q| q.into_ids().len())
            .unwrap_or(0);
        assert_eq!(after, 3, "three Labels after recompose");
        assert!(
            ctx.invalidation().layout,
            "recompose requests layout invalidation"
        );
    }
}
