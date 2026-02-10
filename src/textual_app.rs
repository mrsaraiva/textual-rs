use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rich_rs::{Console, ConsoleOptions, Segments};

use crate::demo_snapshot::{SnapshotArgs, snapshot_widget};
use crate::event::{Action, Event, EventCtx};
use crate::message::{Message, MessageEvent};
use crate::widgets::{AppRoot, Widget, WidgetId};
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

    /// Optional app output returned after the runtime exits.
    fn take_exit_output(&mut self) -> Option<String> {
        None
    }
}

struct TextualAppAdapter<T: TextualApp> {
    id: WidgetId,
    app: Arc<Mutex<T>>,
    child: Box<dyn Widget>,
}

impl<T: TextualApp> TextualAppAdapter<T> {
    fn new(app: Arc<Mutex<T>>, child: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            app,
            child: Box::new(child),
        }
    }
}

impl<T: TextualApp> Widget for TextualAppAdapter<T> {
    fn id(&self) -> WidgetId {
        self.id
    }

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
        if let Message::ButtonPressed { description } = &message.message {
            self.app
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .on_button_pressed(description, ctx);
            if ctx.handled() {
                return;
            }
        }
        self.app
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_message(message, ctx);
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(self.child.as_mut());
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

/// Optional helper for example/dev binaries that support both runtime and snapshot output.
///
/// This keeps snapshot wiring out of example `main()` bodies while remaining opt-in.
pub async fn run_snapshot<T: TextualApp>(definition: T) -> Result<()> {
    let _ = run_snapshot_with_output(definition).await?;
    Ok(())
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
