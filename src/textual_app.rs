use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rich_rs::{Console, ConsoleOptions, Segments};

use crate::demo_snapshot::{SnapshotArgs, snapshot_widget};
use crate::event::{Action, Event, EventCtx};
use crate::message::{CommandPaletteCommand, Message, MessageEvent};
use crate::node_id::NodeId;
use crate::validation::ValidationResult;
use crate::widgets::{AppRoot, Widget};
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

    /// Called after widget mount, before entering the event loop.
    fn on_mount(&mut self) {}

    /// App-level action hook. Called after widget dispatch if the event was not handled.
    fn on_action(&mut self, _action: Action, _ctx: &mut EventCtx) {}

    /// App-level message hook. Called after widget message dispatch if not handled.
    fn on_message(&mut self, _message: &MessageEvent, _ctx: &mut EventCtx) {}

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
    child: Box<dyn Widget>,
    command_palette_providers: Vec<Box<dyn CommandPaletteProvider>>,
    command_palette_provider_index: HashMap<String, (usize, String)>,
}

impl<T: TextualApp> TextualAppAdapter<T> {
    fn new(app: Arc<Mutex<T>>, child: impl Widget + 'static) -> Self {
        Self {
            app,
            child: Box::new(child),
            command_palette_providers: Vec::new(),
            command_palette_provider_index: HashMap::new(),
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

        let mut commands = Vec::new();
        for (provider_index, provider) in self.command_palette_providers.iter_mut().enumerate() {
            provider.startup(ctx);
            for command in provider.commands() {
                self.command_palette_provider_index
                    .insert(command.id.clone(), (provider_index, command.id.clone()));
                commands.push(command);
            }
        }

        if !commands.is_empty() {
            ctx.post_message(Message::CommandPaletteSetCommands { commands });
        }
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
}

impl<T: TextualApp> Widget for TextualAppAdapter<T> {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.child.render_styled(console, options)
    }

    fn on_mount(&mut self) {
        self.child.on_mount();
        self.app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_mount();
    }

    fn on_unmount(&mut self) {
        let mut ctx = EventCtx::default();
        self.shutdown_command_palette_providers(&mut ctx);
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
        if ctx.handled() {
            return;
        }
        if let Event::Action(action) = event {
            self.app
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .on_action(*action, ctx);
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        self.child.on_message(message, ctx);
        if ctx.handled() {
            return;
        }
        match &message.message {
            Message::CommandPaletteOpened => {
                self.initialize_command_palette_providers(ctx);
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_command_palette_opened(ctx);
                if ctx.handled() {
                    return;
                }
            }
            Message::CommandPaletteClosed => {
                self.shutdown_command_palette_providers(ctx);
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_command_palette_closed(ctx);
                if ctx.handled() {
                    return;
                }
            }
            Message::CommandPaletteCommandSelected { id, title } => {
                self.handle_command_palette_selection(id, ctx);
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_command_palette_command_selected(id, title, ctx);
                if ctx.handled() {
                    return;
                }
            }
            _ => {}
        }
        match &message.message {
            Message::ButtonPressed { description } => {
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_button_pressed(description, ctx);
            }
            Message::InputChanged { value, validation } => {
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_input_changed(value, validation, ctx);
            }
            Message::InputSubmitted { value } => {
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_input_submitted(value, ctx);
            }
            Message::TextAreaChanged { value } => {
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_text_area_changed(value, ctx);
            }
            Message::CheckboxChanged { checked } => {
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_checkbox_changed(*checked, ctx);
            }
            Message::ListViewSelectionChanged { index, item } => {
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_list_view_selection_changed(*index, item, ctx);
            }
            Message::ListViewItemActivated { index, item } => {
                self.app
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_list_view_item_activated(*index, item, ctx);
            }
            Message::TabActivated { index, title, .. } => {
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
        self.app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_message(message, ctx);
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
    let mut root = TextualAppAdapter::new(state.clone(), composed);
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
    use crate::node_id::node_id_from_ffi;
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
                message: Message::CommandPaletteOpened,
            },
            &mut open_ctx,
        );
        assert_eq!(state.startup_count.load(Ordering::SeqCst), 1);
        let open_messages = open_ctx.take_messages();
        assert!(
            open_messages
                .iter()
                .any(|event| matches!(event.message, Message::CommandPaletteSetCommands { .. }))
        );

        let mut select_ctx = EventCtx::default();
        adapter.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::CommandPaletteCommandSelected {
                    id: "deploy".to_string(),
                    title: "Deploy".to_string(),
                },
            },
            &mut select_ctx,
        );
        assert_eq!(state.selected_count.load(Ordering::SeqCst), 1);

        let mut close_ctx = EventCtx::default();
        adapter.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::CommandPaletteClosed,
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
                message: Message::CommandPaletteOpened,
            },
            &mut first_open_ctx,
        );
        let mut first_close_ctx = EventCtx::default();
        adapter.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::CommandPaletteClosed,
            },
            &mut first_close_ctx,
        );
        let mut second_open_ctx = EventCtx::default();
        adapter.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::CommandPaletteOpened,
            },
            &mut second_open_ctx,
        );
        let mut second_close_ctx = EventCtx::default();
        adapter.on_message(
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::CommandPaletteClosed,
            },
            &mut second_close_ctx,
        );

        assert_eq!(state.startup_count.load(Ordering::SeqCst), 2);
        assert_eq!(state.shutdown_count.load(Ordering::SeqCst), 2);
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
            Message::ButtonPressed {
                description: "ok".to_string(),
            },
            Message::InputChanged {
                value: "42".to_string(),
                validation: ValidationResult::success(),
            },
            Message::InputSubmitted {
                value: "submit".to_string(),
            },
            Message::TextAreaChanged {
                value: "textarea".to_string(),
            },
            Message::CheckboxChanged { checked: true },
            Message::ListViewSelectionChanged {
                index: 2,
                item: "gamma".to_string(),
            },
            Message::ListViewItemActivated {
                index: 3,
                item: "delta".to_string(),
            },
            Message::TabActivated {
                id: "general".to_string(),
                index: 1,
                title: "General".to_string(),
            },
        ];
        for message in messages.drain(..) {
            let mut ctx = EventCtx::default();
            adapter.on_message(
                &MessageEvent {
                    sender: NodeId::default(),
                    message,
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
        assert_eq!(app.hooks.fallback_count, 8);
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
            Message::OverlaySetVisible {
                overlay,
                visible: true
            } if overlay == first
        ));
        assert!(matches!(
            messages[1].message,
            Message::OverlaySetVisible {
                overlay,
                visible: false
            } if overlay == first
        ));
        assert!(matches!(
            messages[2].message,
            Message::OverlaySetVisible {
                overlay,
                visible: true
            } if overlay == second
        ));
        assert!(matches!(
            messages[3].message,
            Message::OverlaySetVisible {
                overlay,
                visible: false
            } if overlay == second
        ));
        assert!(matches!(
            messages[4].message,
            Message::OverlaySetVisible {
                overlay,
                visible: true
            } if overlay == first
        ));
        assert!(matches!(
            messages[5].message,
            Message::OverlaySetVisible {
                overlay,
                visible: false
            } if overlay == first
        ));
    }
}
