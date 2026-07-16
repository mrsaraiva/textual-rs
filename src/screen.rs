//! Screen system for full-page overlays with independent widget trees.
//!
//! Screens are stacked — only the topmost screen is active (receives events,
//! renders). Screens can return results when popped via `ScreenResult`.
//!
//! Lifecycle:
//! - `on_mount` — called when a screen is pushed and becomes active.
//! - `on_suspend` — called on the previously active screen when a new screen
//!   is pushed on top.
//! - `on_resume` — called when a screen becomes active again after the screen
//!   above it was popped.
//! - `on_unmount` — called when a screen is popped from the stack.

use crate::css::StyleSheet;
use crate::event::EventCtx;
use crate::message::{ButtonPressed, MessageEvent};
use crate::node_id::NodeId;
use crate::widget_tree::WidgetTree;
use crate::widgets::{BindingDecl, Widget};
use rich_rs::{Console, ConsoleOptions, Segments};
use std::fs;
use std::sync::{Arc, Mutex};

use crate::event::Event;

// ---------------------------------------------------------------------------
// Screen trait
// ---------------------------------------------------------------------------

/// A screen is a full-page container that manages its own widget tree.
/// Screens are stacked — only the topmost screen is active (receives events, renders).
pub trait Screen: Send + Sync {
    /// Human-readable name for this screen (used in debug/logging).
    fn name(&self) -> &str {
        "Screen"
    }

    /// CSS type name for this screen: the concrete type's short name.
    ///
    /// Python parity (`DOMNode._css_type_names`): a screen subclass participates
    /// in CSS type matching under its own class name, so a rule like
    /// `GotoScreen { align: center middle; }` matches a `GotoScreen` instance.
    /// The base names (`ModalScreen`/`Screen`) keep matching via the screen
    /// root's type aliases, mirroring the Python MRO. Override only if the CSS
    /// name must differ from the Rust type name.
    fn style_type(&self) -> &'static str {
        crate::widgets::short_type_name::<Self>()
    }

    /// The root widget for this screen. Called once when the screen is mounted.
    fn compose(&self) -> Box<dyn Widget>;

    /// CSS stylesheet for this screen (optional).
    fn css(&self) -> Option<&str> {
        None
    }

    /// Called when the screen becomes the active (topmost) screen.
    fn on_mount(&mut self) {}

    /// Called when the screen is no longer the active screen (another pushed on top).
    fn on_suspend(&mut self) {}

    /// Called when the screen becomes active again (screen above was popped).
    fn on_resume(&mut self) {}

    /// Called when this screen is popped from the stack.
    fn on_unmount(&mut self) {}

    /// Whether this screen is modal (blocks interaction with screens below).
    /// Default: true.
    fn is_modal(&self) -> bool {
        true
    }

    /// Screen title (overrides the app title in the Header widget).
    /// Return `None` to use the app's default title.
    fn title(&self) -> Option<&str> {
        None
    }

    /// Screen sub-title (overrides the app sub-title in the Header widget).
    /// Return `None` to use the app's default sub-title.
    fn sub_title(&self) -> Option<&str> {
        None
    }

    /// CSS selector for the widget to focus automatically when this screen
    /// becomes active (mirrors Python `Screen.AUTO_FOCUS`, e.g. the command
    /// palette's `"CommandInput"` at `command.py:535`).
    ///
    /// Return `Some(selector)` to focus the first matching focusable node when
    /// the screen is pushed; the runtime falls back to focusing the first
    /// focusable node in the screen tree when this is `None` (the Python `"*"`
    /// default) or when the selector matches nothing. The selector accepts the
    /// same forms as [`crate::runtime::App::query_one`] (`#id`, a type name,
    /// `.class`, …). Mirrors `Screen.AUTO_FOCUS` (`screen.py:152-156`).
    fn auto_focus(&self) -> Option<&str> {
        None
    }

    // -----------------------------------------------------------------------
    // Handler surface (Screen as a DOMNode — Python parity)
    // -----------------------------------------------------------------------
    //
    // A `Screen` is the root node of its own widget tree, so it owns the same
    // event/message/binding surface as a `Widget`. The runtime routes the active
    // screen's key bindings, events, and bubbled messages into these methods
    // (via the screen-tree root `ScreenHost`), mirroring Python's
    // `Screen(Widget)` with `BINDINGS` + `on_*` handlers.

    /// Declarative key bindings owned by this screen.
    ///
    /// These are matched along the focused→root chain of the active screen tree
    /// exactly like widget bindings (the screen root sits at the top of that
    /// chain), so a screen binding such as `("escape", "dismiss", "Close")`
    /// fires whenever the screen is active. Mirrors Python `Screen.BINDINGS`.
    fn bindings(&self) -> Vec<BindingDecl> {
        Vec::new()
    }

    /// Handle a raw event routed to the active screen.
    ///
    /// Called during the active screen tree's bubble phase (the screen root is
    /// the last node on the focused→root path). Mirrors Python `Screen.on_*`
    /// event handlers. Use [`ScreenMessageCtx::dismiss`] to close the screen
    /// with a result.
    fn on_event(&mut self, _event: &Event, _ctx: &mut ScreenMessageCtx) {}

    /// Handle a message bubbling up to the active screen.
    ///
    /// Called when a message reaches the screen-tree root. The default
    /// implementation forwards `Button.Pressed` messages to
    /// [`Screen::on_button_pressed`], mirroring Python's typed-handler dispatch
    /// (`on_button_pressed`). Override this for custom message handling.
    fn on_message(&mut self, message: &MessageEvent, ctx: &mut ScreenMessageCtx) {
        if let Some(pressed) = message.downcast_ref::<ButtonPressed>() {
            let control = message.control.unwrap_or(message.sender);
            self.on_button_pressed(pressed, control, ctx);
        }
    }

    /// Typed convenience handler for `Button.Pressed` messages reaching the
    /// screen. `control` is the `NodeId` of the button that was pressed; the
    /// pressed button's CSS id is available as `pressed.button_id`.
    ///
    /// Mirrors Python `Screen.on_button_pressed(self, event)` where
    /// `event.button.id` selects the action. The common modal pattern is to
    /// `ctx.dismiss(value)` here.
    fn on_button_pressed(
        &mut self,
        _pressed: &ButtonPressed,
        _control: NodeId,
        _ctx: &mut ScreenMessageCtx,
    ) {
    }
}

// ---------------------------------------------------------------------------
// ScreenMessageCtx
// ---------------------------------------------------------------------------

/// Context handed to a [`Screen`]'s event/message handlers.
///
/// Wraps the underlying [`EventCtx`] (so handlers can `post_message`, request
/// repaint, set-handled, etc.) and adds screen-scoped controls — most
/// importantly [`dismiss`](Self::dismiss), which records a result to be
/// delivered to the screen's result callback when the screen is popped.
///
/// `dismiss` does not pop the screen synchronously; instead it stages the
/// dismissal so the runtime can pop the active screen and invoke its callback
/// on the next loop pass. This keeps screen teardown on the single runtime
/// control path (the same way `Screen.dismiss()` in Python schedules an
/// `AwaitComplete` rather than tearing down mid-handler).
pub struct ScreenMessageCtx<'a> {
    ctx: &'a mut EventCtx,
    dismiss_slot: &'a Mutex<Option<ScreenResult>>,
}

impl<'a> ScreenMessageCtx<'a> {
    fn new(ctx: &'a mut EventCtx, dismiss_slot: &'a Mutex<Option<ScreenResult>>) -> Self {
        Self { ctx, dismiss_slot }
    }

    /// Construct a `ScreenMessageCtx` directly over an [`EventCtx`] and a
    /// caller-owned dismissal slot.
    ///
    /// Intended for unit tests of `Screen` handlers (e.g. demo `on_button_pressed`
    /// tests): pass a `&Mutex<Option<ScreenResult>>` and inspect it after the
    /// handler runs to assert the staged dismissal. The runtime itself uses the
    /// internal screen-tree wiring, not this constructor.
    pub fn for_test(
        ctx: &'a mut EventCtx,
        dismiss_slot: &'a Mutex<Option<ScreenResult>>,
    ) -> Self {
        Self::new(ctx, dismiss_slot)
    }

    /// Dismiss this screen with a typed result value.
    ///
    /// The boxed value is delivered to the callback registered via
    /// `App::push_screen_with_callback` (or to the awaiter once
    /// `push_screen_wait` lands), which downcasts it back to the expected type.
    /// Dismiss typing is dynamic ([`ScreenResult::Value`]), mirroring Python's
    /// `ModalScreen[T].dismiss(value)` without a generic `Screen<Result = T>`.
    ///
    /// For a plain dismissal with no value, use [`dismiss_none`](Self::dismiss_none).
    pub fn dismiss<T: std::any::Any + Send>(&mut self, value: T) {
        self.set_dismiss(ScreenResult::Value(Box::new(value)));
    }

    /// Dismiss this screen without a result value (Python `self.dismiss()`).
    ///
    /// The callback receives [`ScreenResult::Dismissed`].
    pub fn dismiss_none(&mut self) {
        self.set_dismiss(ScreenResult::Dismissed);
    }

    /// Dismiss this screen with a pre-built [`ScreenResult`].
    pub fn dismiss_result(&mut self, result: ScreenResult) {
        self.set_dismiss(result);
    }

    fn set_dismiss(&mut self, result: ScreenResult) {
        if let Ok(mut slot) = self.dismiss_slot.lock() {
            *slot = Some(result);
        }
        self.ctx.set_handled();
    }

    /// Mark the originating event/message as handled (stops propagation).
    pub fn set_handled(&mut self) {
        self.ctx.set_handled();
    }

    /// Request a repaint of the active screen.
    pub fn request_repaint(&mut self) {
        self.ctx.request_repaint();
    }

    /// Request the app to stop (quit). Equivalent to Python `self.app.exit()`.
    pub fn exit(&mut self) {
        self.ctx.request_stop();
    }

    /// Access the underlying [`EventCtx`] for posting messages / advanced use.
    pub fn event_ctx(&mut self) -> &mut EventCtx {
        self.ctx
    }
}


// ---------------------------------------------------------------------------
// ScreenResult
// ---------------------------------------------------------------------------

/// Result returned when a screen is popped.
pub enum ScreenResult {
    /// Screen was dismissed without a value.
    Dismissed,
    /// Screen returned a value (boxed for type erasure).
    Value(Box<dyn std::any::Any + Send>),
}

// ---------------------------------------------------------------------------
// Result callback type
// ---------------------------------------------------------------------------

/// Type-erased callback invoked when a screen is dismissed with a result.
pub type ScreenResultCallback = Box<dyn FnOnce(ScreenResult) + Send>;

// Base-type aliases for the screen root, mirroring the Python MRO: a concrete
// screen matches its own type name (carried by `ScreenHost::style_type`) plus
// every base name (`GotoScreen(ModalScreen)` -> {GotoScreen, ModalScreen,
// Screen, ...} in `DOMNode._css_type_names`).
const MODAL_SCREEN_ALIASES: &[&str] = &["ModalScreen", "Screen"];
const SCREEN_ALIASES: &[&str] = &["Screen"];

/// Shared, interior-mutable slot a screen handler writes its dismissal into.
///
/// `ScreenHost` (the screen-tree root) and the owning [`ScreenEntry`] both hold
/// a clone of this `Arc`. Handlers stage the result here; the runtime drains it
/// (`App::drain_screen_dismissals`) and pops the screen on the next loop pass.
type DismissSlot = Arc<Mutex<Option<ScreenResult>>>;

/// Handle to a [`Screen`]'s handler surface, shared between the screen-tree
/// root host and the owning [`ScreenEntry`].
///
/// The screen instance is the canonical owner of its BINDINGS and event/message
/// handlers; sharing it through an `Arc<Mutex<…>>` lets the tree root delegate
/// dispatch into it without a parallel runtime-only side path.
type SharedScreen = Arc<Mutex<Box<dyn Screen>>>;

/// Root host widget for pushed screens.
///
/// This preserves canonical Textual CSS typing (`Screen` / `ModalScreen`)
/// while keeping the screen body as a child subtree. It is also the node where
/// the active screen participates as a handler: its `bindings()`,
/// `on_event()`, and `on_message()` delegate to the owning [`Screen`] impl, so
/// the existing focused→root tree dispatch (capture/bubble + binding match)
/// drives screen handlers with no separate dispatch path.
struct ScreenHost {
    modal: bool,
    /// Concrete screen type short name (e.g. `GotoScreen`), captured from
    /// `Screen::style_type()` at push time so CSS rules keyed on the concrete
    /// screen type match the screen root (Python `_css_type_names` parity).
    screen_type: &'static str,
    child: Option<Box<dyn Widget>>,
    screen: SharedScreen,
    dismiss_slot: DismissSlot,
}

impl ScreenHost {
    fn new(
        modal: bool,
        screen_type: &'static str,
        child: Box<dyn Widget>,
        screen: SharedScreen,
        dismiss_slot: DismissSlot,
    ) -> Self {
        Self {
            modal,
            screen_type,
            child: Some(child),
            screen,
            dismiss_slot,
        }
    }
}

impl Widget for ScreenHost {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn compose(&mut self) -> crate::compose::ComposeResult {
        self.child
            .take()
            .into_iter()
            .map(crate::compose::ChildDecl::new)
            .collect()
    }

    fn style_type(&self) -> &'static str {
        self.screen_type
    }

    fn style_type_aliases(&self) -> &[&'static str] {
        if self.modal {
            MODAL_SCREEN_ALIASES
        } else {
            SCREEN_ALIASES
        }
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        self.screen
            .lock()
            .map(|screen| screen.bindings())
            .unwrap_or_default()
    }

    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        if let Ok(mut screen) = self.screen.lock() {
            let mut screen_ctx = ScreenMessageCtx::new(ctx.event_ctx_mut(), &self.dismiss_slot);
            screen.on_event(event, &mut screen_ctx);
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut crate::event::WidgetCtx) {
        if let Ok(mut screen) = self.screen.lock() {
            let mut screen_ctx = ScreenMessageCtx::new(ctx.event_ctx_mut(), &self.dismiss_slot);
            screen.on_message(message, &mut screen_ctx);
        }
    }
}

// ---------------------------------------------------------------------------
// ScreenEntry (internal)
// ---------------------------------------------------------------------------

/// Internal entry in the screen stack.
pub(crate) struct ScreenEntry {
    /// The screen instance, shared with the screen-tree root (`ScreenHost`) so
    /// the root can delegate bindings/event/message dispatch into it.
    pub screen: SharedScreen,
    /// Needs screen switching to swap active tree (no demo uses multiple screens yet).
    #[allow(dead_code)]
    pub widget_tree: WidgetTree,
    /// Per-screen stylesheet; requires screen switching infrastructure.
    #[allow(dead_code)]
    pub stylesheet: Option<StyleSheet>,
    /// Optional callback invoked when this screen is popped.
    result_callback: Option<ScreenResultCallback>,
    /// Pending result set by `dismiss(value)` before the screen is popped.
    pending_result: Option<ScreenResult>,
    /// Dismiss mailbox shared with the screen-tree root. A screen handler's
    /// `ctx.dismiss(value)` writes here; the runtime drains it and pops.
    dismiss_slot: DismissSlot,
    /// If this screen was pushed by `switch_mode`, this holds the mode name.
    /// Used to identify the correct screen when switching/removing modes.
    pub(crate) mode_name: Option<String>,
}

impl ScreenEntry {
    /// Take any pending dismissal staged by a screen handler's `ctx.dismiss(..)`.
    pub(crate) fn take_pending_dismissal(&self) -> Option<ScreenResult> {
        self.dismiss_slot.lock().ok().and_then(|mut s| s.take())
    }

    /// Run `f` against the locked screen instance (used for lifecycle hooks and
    /// title/name accessors).
    fn with_screen<R>(&self, f: impl FnOnce(&mut dyn Screen) -> R) -> Option<R> {
        self.screen.lock().ok().map(|mut s| f(&mut **s))
    }
}

// ---------------------------------------------------------------------------
// ScreenStack
// ---------------------------------------------------------------------------

/// Manages the stack of screens.
///
/// The bottom of the stack (index 0) is the first screen pushed; the top
/// (last element) is the currently active screen.
pub struct ScreenStack {
    screens: Vec<ScreenEntry>,
}

impl ScreenStack {
    /// Create an empty screen stack.
    pub fn new() -> Self {
        Self {
            screens: Vec::new(),
        }
    }

    /// Push a screen onto the stack.
    ///
    /// - Calls `on_suspend` on the previously active screen (if any).
    /// - Builds the widget tree from `screen.compose()`.
    /// - Parses the screen's CSS (if any).
    /// - Calls `on_mount` on the new screen.
    pub fn push(&mut self, screen: Box<dyn Screen>) {
        self.push_inner(screen, None, None);
    }

    /// Push a screen onto the stack with a result callback.
    ///
    /// The callback is invoked with the `ScreenResult` when the screen is
    /// popped (either via `pop()` or via `dismiss()`).
    pub fn push_with_callback(&mut self, screen: Box<dyn Screen>, callback: ScreenResultCallback) {
        self.push_inner(screen, Some(callback), None);
    }

    /// Push a mode screen onto the stack.
    ///
    /// The mode name is stored in the entry so that `pop_mode()` can identify
    /// and remove the correct screen even if transient screens are on top.
    pub fn push_mode(&mut self, screen: Box<dyn Screen>, mode_name: String) {
        self.push_inner(screen, None, Some(mode_name));
    }

    /// Pop the screen associated with the given mode name.
    ///
    /// If the mode screen is not on top (i.e. transient screens are above it),
    /// this pops the mode screen from its position in the stack and calls its
    /// lifecycle hooks. Returns the mode name if found and popped, `None` if
    /// no screen with that mode name exists.
    pub fn pop_mode(&mut self, mode_name: &str) -> Option<String> {
        // Find the entry with the matching mode name.
        let idx = self
            .screens
            .iter()
            .position(|e| e.mode_name.as_deref() == Some(mode_name))?;

        let entry = self.screens.remove(idx);
        entry.with_screen(|s| s.on_unmount());

        // If we removed the top screen and there's a new top, resume it.
        if idx == self.screens.len() {
            if let Some(new_top) = self.screens.last_mut() {
                new_top.with_screen(|s| s.on_resume());
            }
        }

        // Invoke the result callback if one was registered.
        let result = entry.pending_result.unwrap_or(ScreenResult::Dismissed);
        if let Some(callback) = entry.result_callback {
            callback(result);
        }

        entry.mode_name
    }

    /// Return the mode name of the topmost screen (if it has one).
    pub fn top_mode_name(&self) -> Option<&str> {
        self.screens.last().and_then(|e| e.mode_name.as_deref())
    }

    /// Return the `name()` of the topmost screen (if any).
    ///
    /// Used by the system-modal push hook to detect a re-entrant push of the
    /// same system screen (Python guards `ctrl+p` while the palette is already
    /// the top screen, `command.py:736-746`).
    // Consumed by `App::push_system_modal_screen` (Wave 0 hook); the live
    // `ctrl+p` consumer lands with the Wave 1 CommandPaletteScreen rebuild.
    #[allow(dead_code)]
    pub(crate) fn top_screen_name(&self) -> Option<String> {
        self.top().and_then(|e| e.with_screen(|s| s.name().to_string()))
    }

    /// Find the `WidgetTree::tree_id` of the topmost stacked screen whose
    /// [`Screen::name`] or mode name (from `switch_mode`) equals `name`.
    ///
    /// Top-down search: name collisions resolve to the topmost match, the
    /// Python `get_screen` semantic for installed screens. Consumed by the
    /// cross-screen surface (`ScreenRef::Name` resolution).
    pub(crate) fn find_tree_id_by_name(&self, name: &str) -> Option<u64> {
        self.screens.iter().rev().find_map(|entry| {
            let matches = entry.mode_name.as_deref() == Some(name)
                || entry.with_screen(|s| s.name() == name).unwrap_or(false);
            matches.then(|| entry.widget_tree.tree_id())
        })
    }

    fn push_inner(
        &mut self,
        screen: Box<dyn Screen>,
        callback: Option<ScreenResultCallback>,
        mode_name: Option<String>,
    ) {
        // Suspend the currently active screen.
        if let Some(top) = self.screens.last_mut() {
            top.with_screen(|s| s.on_suspend());
        }

        // Build the widget tree from the screen's compose output, extracting
        // composed children/declarations into the arena like the app root path.
        let modal = screen.is_modal();
        let screen_type = screen.style_type();
        let body = screen.compose();
        let shared: SharedScreen = Arc::new(Mutex::new(screen));
        let dismiss_slot: DismissSlot = Arc::new(Mutex::new(None));
        let root_widget = Box::new(ScreenHost::new(
            modal,
            screen_type,
            body,
            shared.clone(),
            dismiss_slot.clone(),
        ));
        let mut widget_tree = WidgetTree::new();
        let root_id = widget_tree.set_root(root_widget);
        // Materialize the screen host's children through the single ChildDecl
        // mount path (RA2.1). The host composes its screen body; the body's own
        // descendants recurse inside `mount_declarations`.
        let compose_decls = widget_tree
            .get_mut(root_id)
            .map(|node| node.widget.compose())
            .unwrap_or_default();
        crate::runtime::App::mount_declarations(&mut widget_tree, root_id, compose_decls);
        crate::runtime::App::mount_system_tooltip(&mut widget_tree, root_id);
        // Every screen carries its own system ToastRack (Python
        // `Screen._extend_compose`), so notifications posted while this screen
        // is active render above it, not on the occluded base tree.
        crate::runtime::App::mount_system_toast_rack(&mut widget_tree, root_id);
        // Drain initial lifecycle events (mount events from tree construction).
        let _ = widget_tree.drain_lifecycle();

        // Parse the screen's CSS stylesheet (if provided).
        // Accept either inline CSS text or a filesystem path.
        let stylesheet = {
            let guard = shared.lock().expect("screen lock");
            guard.css().map(|css| {
                let css_text = fs::read_to_string(css).unwrap_or_else(|_| css.to_string());
                StyleSheet::parse(&css_text)
            })
        };

        // Mount the new screen.
        if let Ok(mut guard) = shared.lock() {
            guard.on_mount();
        }

        self.screens.push(ScreenEntry {
            screen: shared,
            widget_tree,
            stylesheet,
            result_callback: callback,
            pending_result: None,
            dismiss_slot,
            mode_name,
        });
    }

    /// Set a pending dismiss result on the topmost screen.
    ///
    /// This is called by the screen itself (via runtime methods) to store a
    /// result value before the screen is popped. When `pop()` is called, the
    /// pending result takes precedence over the default `Dismissed` variant.
    pub fn dismiss(&mut self, result: ScreenResult) -> bool {
        if let Some(top) = self.screens.last_mut() {
            top.pending_result = Some(result);
            true
        } else {
            false
        }
    }

    /// Pop the topmost screen from the stack.
    ///
    /// - Calls `on_unmount` on the popped screen.
    /// - Calls `on_resume` on the new topmost screen (if any).
    /// - If a result callback was registered (via `push_with_callback`), it is
    ///   invoked with the result and the returned `ScreenResult` will be
    ///   `Dismissed` (the callback owns the real value).
    /// - If no callback was registered, the actual `ScreenResult` (pending
    ///   result from `dismiss()`, or `Dismissed` by default) is returned.
    /// - The third tuple element is the mode name of the popped screen (if it
    ///   was a mode screen). Callers should clear `current_mode` when this
    ///   is `Some`.
    ///
    /// Returns `None` if the stack is empty.
    pub fn pop(&mut self) -> Option<(SharedScreen, ScreenResult, Option<String>)> {
        let entry = self.screens.pop()?;
        entry.with_screen(|s| s.on_unmount());

        // Resume the screen that is now on top.
        if let Some(new_top) = self.screens.last_mut() {
            new_top.with_screen(|s| s.on_resume());
        }

        // Determine the result: use pending_result if set, otherwise Dismissed.
        let result = entry.pending_result.unwrap_or(ScreenResult::Dismissed);

        let mode_name = entry.mode_name;

        // Invoke the result callback if one was registered.
        if let Some(callback) = entry.result_callback {
            callback(result);
            // After callback consumed the result, return Dismissed to caller
            // since the callback already handled it.
            Some((entry.screen, ScreenResult::Dismissed, mode_name))
        } else {
            Some((entry.screen, result, mode_name))
        }
    }

    /// Reference to the topmost screen entry.
    pub(crate) fn top(&self) -> Option<&ScreenEntry> {
        self.screens.last()
    }

    /// Mutable reference to the topmost screen entry.
    ///
    /// Currently unused but part of the public screen API — needed when screen
    /// switching swaps the active widget tree (no demo exercises this yet).
    #[allow(dead_code)]
    pub(crate) fn top_mut(&mut self) -> Option<&mut ScreenEntry> {
        self.screens.last_mut()
    }

    /// Immutable access to a screen entry by stack index (`0` = bottom).
    pub(crate) fn get(&self, index: usize) -> Option<&ScreenEntry> {
        self.screens.get(index)
    }

    /// Mutable access to a screen entry by stack index (`0` = bottom).
    pub(crate) fn get_mut(&mut self, index: usize) -> Option<&mut ScreenEntry> {
        self.screens.get_mut(index)
    }

    /// Number of screens on the stack.
    pub fn len(&self) -> usize {
        self.screens.len()
    }

    /// Whether the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.screens.is_empty()
    }

    /// Get the title from the topmost screen (if it defines one).
    pub fn active_title(&self) -> Option<String> {
        self.top()
            .and_then(|e| e.with_screen(|s| s.title().map(str::to_string)))
            .flatten()
    }

    /// Get the sub-title from the topmost screen (if it defines one).
    pub fn active_sub_title(&self) -> Option<String> {
        self.top()
            .and_then(|e| e.with_screen(|s| s.sub_title().map(str::to_string)))
            .flatten()
    }

    /// Get the `auto_focus` selector from the topmost screen (if it defines one).
    ///
    /// Used by the runtime at push time to focus a specific node instead of the
    /// first focusable one (Python `Screen.AUTO_FOCUS`).
    pub(crate) fn active_auto_focus(&self) -> Option<String> {
        self.top()
            .and_then(|e| e.with_screen(|s| s.auto_focus().map(str::to_string)))
            .flatten()
    }

    /// Take any pending dismissal staged by the active screen's handler.
    ///
    /// Returns the [`ScreenResult`] a handler set via `ctx.dismiss(..)` since the
    /// last drain, or `None`. The runtime uses this to pop the active screen and
    /// deliver the result to its callback on the next loop pass.
    pub(crate) fn take_active_dismissal(&self) -> Option<ScreenResult> {
        self.top().and_then(|e| e.take_pending_dismissal())
    }
}

impl Default for ScreenStack {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rich_rs::{Console, ConsoleOptions, Segments};
    use std::sync::{Arc, Mutex};

    // -- Test helpers --------------------------------------------------------

    /// Tracks lifecycle calls in order for verification.
    #[derive(Debug, Clone, Default)]
    struct LifecycleLog {
        events: Arc<Mutex<Vec<String>>>,
    }

    impl LifecycleLog {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn log(&self, event: &str) {
            self.events.lock().unwrap().push(event.to_string());
        }

        fn events(&self) -> Vec<String> {
            self.events.lock().unwrap().clone()
        }
    }

    /// Minimal widget for screen compose output.
    struct StubWidget;

    impl Widget for StubWidget {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn style_type(&self) -> &'static str {
            "StubWidget"
        }
    }

    /// A test screen that logs lifecycle events.
    struct TestScreen {
        screen_name: String,
        log: LifecycleLog,
        modal: bool,
        css_text: Option<String>,
        screen_title: Option<String>,
        screen_sub_title: Option<String>,
    }

    impl TestScreen {
        fn new(name: &str, log: LifecycleLog) -> Self {
            Self {
                screen_name: name.to_string(),
                log,
                modal: true,
                css_text: None,
                screen_title: None,
                screen_sub_title: None,
            }
        }

        fn with_modal(mut self, modal: bool) -> Self {
            self.modal = modal;
            self
        }

        fn with_css(mut self, css: &str) -> Self {
            self.css_text = Some(css.to_string());
            self
        }

        fn with_title(mut self, title: &str) -> Self {
            self.screen_title = Some(title.to_string());
            self
        }

        fn with_sub_title(mut self, sub_title: &str) -> Self {
            self.screen_sub_title = Some(sub_title.to_string());
            self
        }

        fn boxed(name: &str, log: LifecycleLog) -> Box<dyn Screen> {
            Box::new(Self::new(name, log))
        }
    }

    impl Screen for TestScreen {
        fn name(&self) -> &str {
            &self.screen_name
        }

        fn compose(&self) -> Box<dyn Widget> {
            Box::new(StubWidget)
        }

        fn css(&self) -> Option<&str> {
            self.css_text.as_deref()
        }

        fn on_mount(&mut self) {
            self.log.log(&format!("{}:mount", self.screen_name));
        }

        fn on_suspend(&mut self) {
            self.log.log(&format!("{}:suspend", self.screen_name));
        }

        fn on_resume(&mut self) {
            self.log.log(&format!("{}:resume", self.screen_name));
        }

        fn on_unmount(&mut self) {
            self.log.log(&format!("{}:unmount", self.screen_name));
        }

        fn is_modal(&self) -> bool {
            self.modal
        }

        fn title(&self) -> Option<&str> {
            self.screen_title.as_deref()
        }

        fn sub_title(&self) -> Option<&str> {
            self.screen_sub_title.as_deref()
        }
    }

    // -- ScreenStack: new is empty -------------------------------------------

    #[test]
    fn new_stack_is_empty() {
        let stack = ScreenStack::new();
        assert!(stack.is_empty());
        assert_eq!(stack.len(), 0);
        assert!(stack.top().is_none());
    }

    // -- ScreenStack: push increases len -------------------------------------

    #[test]
    fn push_increases_len() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("A", log.clone()));
        assert_eq!(stack.len(), 1);
        assert!(!stack.is_empty());

        stack.push(TestScreen::boxed("B", log.clone()));
        assert_eq!(stack.len(), 2);

        stack.push(TestScreen::boxed("C", log.clone()));
        assert_eq!(stack.len(), 3);
    }

    // -- ScreenStack: pop returns screen + calls lifecycle -------------------

    #[test]
    fn pop_returns_screen_and_dismissed_result() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("Main", log.clone()));
        let result = stack.pop();
        assert!(result.is_some());

        let (screen, screen_result, _mode) = result.unwrap();
        assert_eq!(screen.lock().unwrap().name(), "Main");
        assert!(matches!(screen_result, ScreenResult::Dismissed));
        assert!(stack.is_empty());
    }

    // -- ScreenStack: push calls on_suspend on previous, on_mount on new ----

    #[test]
    fn push_calls_suspend_on_previous_and_mount_on_new() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("First", log.clone()));
        assert_eq!(log.events(), vec!["First:mount"]);

        stack.push(TestScreen::boxed("Second", log.clone()));
        assert_eq!(
            log.events(),
            vec!["First:mount", "First:suspend", "Second:mount"]
        );
    }

    // -- ScreenStack: pop calls on_unmount on popped, on_resume on new top --

    #[test]
    fn pop_calls_unmount_on_popped_and_resume_on_new_top() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("Base", log.clone()));
        stack.push(TestScreen::boxed("Overlay", log.clone()));

        // Clear log to focus on pop behavior.
        log.events.lock().unwrap().clear();

        stack.pop();
        assert_eq!(log.events(), vec!["Overlay:unmount", "Base:resume"]);
    }

    // -- ScreenStack: pop on empty returns None ------------------------------

    #[test]
    fn pop_on_empty_returns_none() {
        let mut stack = ScreenStack::new();
        assert!(stack.pop().is_none());
    }

    // -- ScreenStack: top returns topmost ------------------------------------

    #[test]
    fn top_returns_topmost() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("Bottom", log.clone()));
        stack.push(TestScreen::boxed("Top", log.clone()));

        assert_eq!(stack.top().unwrap().screen.lock().unwrap().name(), "Top");
    }

    #[test]
    fn top_mut_returns_topmost() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("Bottom", log.clone()));
        stack.push(TestScreen::boxed("Top", log.clone()));

        assert_eq!(
            stack.top_mut().unwrap().screen.lock().unwrap().name(),
            "Top"
        );
    }

    // -- ScreenResult: Dismissed and Value variants --------------------------

    #[test]
    fn screen_result_dismissed() {
        let result = ScreenResult::Dismissed;
        assert!(matches!(result, ScreenResult::Dismissed));
    }

    #[test]
    fn screen_result_value() {
        let result = ScreenResult::Value(Box::new(42i32));
        match result {
            ScreenResult::Value(val) => {
                let num = val.downcast_ref::<i32>().unwrap();
                assert_eq!(*num, 42);
            }
            _ => panic!("expected Value variant"),
        }
    }

    #[test]
    fn screen_result_value_string() {
        let result = ScreenResult::Value(Box::new("hello".to_string()));
        match result {
            ScreenResult::Value(val) => {
                let s = val.downcast_ref::<String>().unwrap();
                assert_eq!(s, "hello");
            }
            _ => panic!("expected Value variant"),
        }
    }

    // -- Modal default is true -----------------------------------------------

    #[test]
    fn modal_default_is_true() {
        let log = LifecycleLog::new();
        let screen = TestScreen::new("test", log);
        assert!(screen.is_modal());
    }

    #[test]
    fn modal_can_be_overridden() {
        let log = LifecycleLog::new();
        let screen = TestScreen::new("test", log).with_modal(false);
        assert!(!screen.is_modal());
    }

    // -- Screen lifecycle ordering -------------------------------------------

    #[test]
    fn full_lifecycle_ordering() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        // Push screen A.
        stack.push(TestScreen::boxed("A", log.clone()));
        // Push screen B (suspends A).
        stack.push(TestScreen::boxed("B", log.clone()));
        // Push screen C (suspends B).
        stack.push(TestScreen::boxed("C", log.clone()));

        // Pop C (unmounts C, resumes B).
        stack.pop();
        // Pop B (unmounts B, resumes A).
        stack.pop();
        // Pop A (unmounts A, no resume).
        stack.pop();

        assert_eq!(
            log.events(),
            vec![
                "A:mount",
                "A:suspend",
                "B:mount",
                "B:suspend",
                "C:mount",
                "C:unmount",
                "B:resume",
                "B:unmount",
                "A:resume",
                "A:unmount",
            ]
        );
    }

    // -- Widget tree is built from compose -----------------------------------

    #[test]
    fn push_builds_widget_tree() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("test", log));

        let entry = stack.top().unwrap();
        // The widget tree should have a root node (from compose).
        assert!(entry.widget_tree.root().is_some());
        // Root host + composed StubWidget + system tooltip + system ToastRack.
        assert_eq!(entry.widget_tree.len(), 4);
    }

    // -- CSS stylesheet is parsed from css() --------------------------------

    #[test]
    fn push_parses_css_stylesheet() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        let screen = TestScreen::new("styled", log).with_css("Button { color: red; }");
        stack.push(Box::new(screen));

        let entry = stack.top().unwrap();
        assert!(entry.stylesheet.is_some());
    }

    #[test]
    fn push_no_css_gives_none_stylesheet() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("plain", log));

        let entry = stack.top().unwrap();
        assert!(entry.stylesheet.is_none());
    }

    // -- Default trait method coverage ---------------------------------------

    #[test]
    fn screen_default_name() {
        /// Minimal screen impl that only provides compose.
        struct MinimalScreen;

        impl Screen for MinimalScreen {
            fn compose(&self) -> Box<dyn Widget> {
                Box::new(StubWidget)
            }
        }

        let screen = MinimalScreen;
        assert_eq!(screen.name(), "Screen");
        assert!(screen.css().is_none());
        assert!(screen.is_modal());
        assert!(screen.title().is_none());
        assert!(screen.sub_title().is_none());
    }

    // -- Pop last screen has no resume target --------------------------------

    #[test]
    fn pop_single_screen_no_resume_called() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("Only", log.clone()));
        log.events.lock().unwrap().clear();

        stack.pop();
        // Only unmount, no resume (nothing below).
        assert_eq!(log.events(), vec!["Only:unmount"]);
    }

    // -- Multiple pushes without pops ----------------------------------------

    #[test]
    fn multiple_pushes_suspend_chain() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("A", log.clone()));
        stack.push(TestScreen::boxed("B", log.clone()));
        stack.push(TestScreen::boxed("C", log.clone()));
        stack.push(TestScreen::boxed("D", log.clone()));

        assert_eq!(
            log.events(),
            vec![
                "A:mount",
                "A:suspend",
                "B:mount",
                "B:suspend",
                "C:mount",
                "C:suspend",
                "D:mount",
            ]
        );
        assert_eq!(stack.len(), 4);
    }

    // =========================================================================
    // P5-04: Screen results with callbacks
    // =========================================================================

    #[test]
    fn push_with_callback_invokes_on_pop() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();
        let callback_log = Arc::new(Mutex::new(Vec::<String>::new()));
        let cb_log = callback_log.clone();

        stack.push_with_callback(
            TestScreen::boxed("Dialog", log.clone()),
            Box::new(move |result| {
                let msg = match result {
                    ScreenResult::Dismissed => "dismissed".to_string(),
                    ScreenResult::Value(v) => {
                        format!("value:{}", v.downcast_ref::<i32>().unwrap())
                    }
                };
                cb_log.lock().unwrap().push(msg);
            }),
        );

        // Pop without dismiss — should get Dismissed.
        stack.pop();
        assert_eq!(callback_log.lock().unwrap().as_slice(), &["dismissed"]);
    }

    #[test]
    fn dismiss_with_value_invokes_callback_with_value() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();
        let callback_log = Arc::new(Mutex::new(Vec::<String>::new()));
        let cb_log = callback_log.clone();

        stack.push_with_callback(
            TestScreen::boxed("Dialog", log.clone()),
            Box::new(move |result| {
                let msg = match result {
                    ScreenResult::Dismissed => "dismissed".to_string(),
                    ScreenResult::Value(v) => {
                        format!("value:{}", v.downcast_ref::<String>().unwrap())
                    }
                };
                cb_log.lock().unwrap().push(msg);
            }),
        );

        // Dismiss with a value, then pop.
        stack.dismiss(ScreenResult::Value(Box::new("confirmed".to_string())));
        stack.pop();
        assert_eq!(
            callback_log.lock().unwrap().as_slice(),
            &["value:confirmed"]
        );
    }

    #[test]
    fn pop_without_callback_returns_pending_result() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("Dialog", log.clone()));
        stack.dismiss(ScreenResult::Value(Box::new(42i32)));

        let (_, result, _) = stack.pop().unwrap();
        match result {
            ScreenResult::Value(v) => assert_eq!(*v.downcast_ref::<i32>().unwrap(), 42),
            ScreenResult::Dismissed => panic!("expected Value"),
        }
    }

    #[test]
    fn dismiss_on_empty_stack_returns_false() {
        let mut stack = ScreenStack::new();
        assert!(!stack.dismiss(ScreenResult::Dismissed));
    }

    #[test]
    fn callback_receives_dismissed_when_no_pending_result() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();
        let received = Arc::new(Mutex::new(false));
        let received_clone = received.clone();

        stack.push_with_callback(
            TestScreen::boxed("X", log.clone()),
            Box::new(move |result| {
                *received_clone.lock().unwrap() = matches!(result, ScreenResult::Dismissed);
            }),
        );

        stack.pop();
        assert!(*received.lock().unwrap());
    }

    // =========================================================================
    // P5-14: Screen title/sub_title
    // =========================================================================

    #[test]
    fn screen_title_default_is_none() {
        let log = LifecycleLog::new();
        let screen = TestScreen::new("test", log);
        assert!(screen.title().is_none());
        assert!(screen.sub_title().is_none());
    }

    #[test]
    fn screen_title_can_be_set() {
        let log = LifecycleLog::new();
        let screen = TestScreen::new("test", log)
            .with_title("My App")
            .with_sub_title("v1.0");
        assert_eq!(screen.title(), Some("My App"));
        assert_eq!(screen.sub_title(), Some("v1.0"));
    }

    #[test]
    fn active_title_from_topmost_screen() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        // No screens — no title.
        assert!(stack.active_title().is_none());
        assert!(stack.active_sub_title().is_none());

        // Push screen without title.
        stack.push(TestScreen::boxed("Base", log.clone()));
        assert!(stack.active_title().is_none());

        // Push screen with title.
        let titled = TestScreen::new("Settings", log.clone())
            .with_title("Settings")
            .with_sub_title("General");
        stack.push(Box::new(titled));
        assert_eq!(stack.active_title().as_deref(), Some("Settings"));
        assert_eq!(stack.active_sub_title().as_deref(), Some("General"));

        // Pop titled screen — back to base with no title.
        stack.pop();
        assert!(stack.active_title().is_none());
    }

    #[test]
    fn pushed_modal_screen_root_carries_concrete_type_plus_base_aliases_and_preserves_body_widget()
    {
        struct ScreenBody;

        impl Widget for ScreenBody {
            fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
                Segments::new()
            }

            fn style_type(&self) -> &'static str {
                "QuitScreen"
            }
        }

        struct ModalBodyScreen;

        impl Screen for ModalBodyScreen {
            fn compose(&self) -> Box<dyn Widget> {
                Box::new(ScreenBody)
            }
        }

        let mut stack = ScreenStack::new();
        stack.push(Box::new(ModalBodyScreen));

        let entry = stack.top().expect("top screen should exist");
        let root_id = entry
            .widget_tree
            .root()
            .expect("screen tree root should exist");
        let root = entry
            .widget_tree
            .get(root_id)
            .expect("root node should exist");
        assert_eq!(
            root.widget.style_type(),
            "ModalBodyScreen",
            "screen root should carry the concrete screen type name"
        );
        assert!(
            root.widget.style_type_aliases().contains(&"ModalScreen"),
            "modal screen root should also match ModalScreen selectors"
        );
        assert!(
            root.widget.style_type_aliases().contains(&"Screen"),
            "modal screen root should also match Screen selectors"
        );

        let child_id = *entry
            .widget_tree
            .children(root_id)
            .first()
            .expect("modal root should host the composed body widget");
        let child = entry
            .widget_tree
            .get(child_id)
            .expect("body widget should exist");
        assert_eq!(
            child.widget.style_type(),
            "QuitScreen",
            "screen body widget type should remain available for screen-specific selectors"
        );
    }

    /// Regression: a CSS rule keyed on the CONCRETE screen type (e.g.
    /// `GotoScreen { ... }`) must match the pushed screen's root node, for both
    /// modal and non-modal screens, while the base `Screen`/`ModalScreen`
    /// selectors keep matching via aliases (Python `_css_type_names` MRO).
    #[test]
    fn concrete_screen_type_css_selector_matches_screen_root() {
        struct GotoScreen;

        impl Screen for GotoScreen {
            fn compose(&self) -> Box<dyn Widget> {
                Box::new(StubWidget)
            }
        }

        struct HelpScreen;

        impl Screen for HelpScreen {
            fn compose(&self) -> Box<dyn Widget> {
                Box::new(StubWidget)
            }

            fn is_modal(&self) -> bool {
                false
            }
        }

        let _guard = crate::css::set_style_context(StyleSheet::parse(
            "GotoScreen { fg: #ff00aa; } HelpScreen { fg: #00ff00; }",
        ));

        let mut stack = ScreenStack::new();
        stack.push(Box::new(GotoScreen));
        {
            let entry = stack.top().expect("top screen should exist");
            let root_id = entry.widget_tree.root().expect("screen tree root");
            let root = entry.widget_tree.get(root_id).expect("root node");
            assert_eq!(root.widget.style_type(), "GotoScreen");
            assert!(
                root.widget.style_type_aliases().contains(&"ModalScreen")
                    && root.widget.style_type_aliases().contains(&"Screen"),
                "modal screen root keeps the ModalScreen/Screen base aliases, got {:?}",
                root.widget.style_type_aliases()
            );
            let meta = crate::css::node_selector_meta(&entry.widget_tree, root_id);
            let style = crate::css::resolve_style_for_meta(&meta);
            assert_eq!(
                style.fg,
                crate::style::parse_color_like("#ff00aa"),
                "GotoScreen type selector should style the modal screen root"
            );
        }

        stack.push(Box::new(HelpScreen));
        {
            let entry = stack.top().expect("top screen should exist");
            let root_id = entry.widget_tree.root().expect("screen tree root");
            let root = entry.widget_tree.get(root_id).expect("root node");
            assert_eq!(root.widget.style_type(), "HelpScreen");
            assert!(
                root.widget.style_type_aliases().contains(&"Screen"),
                "non-modal screen root keeps the Screen base alias, got {:?}",
                root.widget.style_type_aliases()
            );
            let meta = crate::css::node_selector_meta(&entry.widget_tree, root_id);
            let style = crate::css::resolve_style_for_meta(&meta);
            assert_eq!(
                style.fg,
                crate::style::parse_color_like("#00ff00"),
                "HelpScreen type selector should style the non-modal screen root"
            );
        }
    }

    // =========================================================================
    // Keystone 1b: Screen-as-Widget handler surface (BINDINGS + on_message +
    // dismiss-with-result)
    // =========================================================================

    use crate::message::ButtonPressed;
    use crate::node_id::NodeId;
    use crate::runtime::dispatch_message_queue_tree;

    /// A modal QuitScreen mirroring `modal03.py`:
    /// - owns a `("escape", "dismiss", "Cancel")` binding,
    /// - dismisses with `true` when a `#quit` button is pressed and `false`
    ///   otherwise, via its own `on_button_pressed` handler.
    struct QuitScreen;

    impl Screen for QuitScreen {
        fn name(&self) -> &str {
            "QuitScreen"
        }

        fn compose(&self) -> Box<dyn Widget> {
            Box::new(StubWidget)
        }

        fn bindings(&self) -> Vec<BindingDecl> {
            vec![BindingDecl::new("escape", "dismiss", "Cancel")]
        }

        fn on_button_pressed(
            &mut self,
            pressed: &ButtonPressed,
            _control: NodeId,
            ctx: &mut ScreenMessageCtx,
        ) {
            if pressed.button_id.as_deref() == Some("quit") {
                ctx.dismiss(true);
            } else {
                ctx.dismiss(false);
            }
        }
    }

    // -- Screen BINDINGS are exposed on the screen-tree root host ------------

    #[test]
    fn screen_bindings_surface_on_screen_tree_root() {
        let mut stack = ScreenStack::new();
        stack.push(Box::new(QuitScreen));

        let entry = stack.top().expect("top screen");
        let root_id = entry.widget_tree.root().expect("screen tree root");
        let root = entry.widget_tree.get(root_id).expect("root node");

        let bindings = root.widget.bindings();
        assert_eq!(bindings.len(), 1, "screen binding should surface on root");
        assert_eq!(bindings[0].key, "escape");
        assert_eq!(bindings[0].action, "dismiss");
    }

    // -- on_button_pressed -> dismiss(value) reaches the callback -----------

    #[test]
    fn screen_button_press_dismisses_with_value_to_callback() {
        let mut stack = ScreenStack::new();
        let received = Arc::new(Mutex::new(Vec::<String>::new()));
        let cb = received.clone();

        // Push QuitScreen with a callback that records the dismiss value.
        stack.push_with_callback(
            Box::new(QuitScreen),
            Box::new(move |result| {
                let msg = match result {
                    ScreenResult::Dismissed => "dismissed".to_string(),
                    ScreenResult::Value(v) => match v.downcast::<bool>() {
                        Ok(b) => format!("value:{}", *b),
                        Err(_) => "value:?".to_string(),
                    },
                };
                cb.lock().unwrap().push(msg);
            }),
        );

        // Build a ButtonPressed message from a #quit button and bubble it
        // through the real screen tree to the screen-tree root, which delegates
        // to QuitScreen::on_message -> on_button_pressed -> ctx.dismiss(true).
        let (root_id, quit_button_id) = {
            let entry = stack.top().expect("top screen");
            let root_id = entry.widget_tree.root().expect("root");
            // Sender doesn't need to be in the tree for bubble-to-root; the
            // bubble path falls back to a depth-first walk that includes root.
            (root_id, crate::node_id::node_id_from_ffi(424242))
        };

        let message = MessageEvent::new(
            quit_button_id,
            ButtonPressed {
                description: "Quit".into(),
                button_id: Some("quit".into()),
            },
        )
        .with_control(quit_button_id);

        {
            let entry = stack.top_mut().expect("top screen");
            let _ = dispatch_message_queue_tree(&mut entry.widget_tree, vec![message]);
        }
        let _ = root_id;

        // The handler staged a dismissal; drain it and confirm the value.
        let staged = stack.take_active_dismissal().expect("dismissal staged");
        assert!(stack.dismiss(staged));
        stack.pop().expect("screen popped");

        assert_eq!(received.lock().unwrap().as_slice(), &["value:true"]);
    }

    #[test]
    fn screen_cancel_button_dismisses_with_false() {
        let mut stack = ScreenStack::new();
        let received = Arc::new(Mutex::new(Vec::<String>::new()));
        let cb = received.clone();

        stack.push_with_callback(
            Box::new(QuitScreen),
            Box::new(move |result| {
                if let ScreenResult::Value(v) = result {
                    if let Ok(b) = v.downcast::<bool>() {
                        cb.lock().unwrap().push(format!("value:{}", *b));
                    }
                }
            }),
        );

        let sender = crate::node_id::node_id_from_ffi(99);
        let message = MessageEvent::new(
            sender,
            ButtonPressed {
                description: "Cancel".into(),
                button_id: Some("cancel".into()),
            },
        )
        .with_control(sender);

        {
            let entry = stack.top_mut().expect("top screen");
            let _ = dispatch_message_queue_tree(&mut entry.widget_tree, vec![message]);
        }

        let staged = stack.take_active_dismissal().expect("dismissal staged");
        assert!(stack.dismiss(staged));
        stack.pop().expect("screen popped");

        assert_eq!(received.lock().unwrap().as_slice(), &["value:false"]);
    }

    // -- Screen::on_event handler runs via the tree bubble phase -------------

    #[test]
    fn screen_on_event_handler_runs_and_can_dismiss() {
        use crate::runtime::dispatch_event_tree;

        struct EventScreen;
        impl Screen for EventScreen {
            fn compose(&self) -> Box<dyn Widget> {
                Box::new(StubWidget)
            }
            fn on_event(&mut self, event: &Event, ctx: &mut ScreenMessageCtx) {
                if let Event::Key(key) = event {
                    if key.aliases().iter().any(|a| *a == "escape") {
                        ctx.dismiss_none();
                    }
                }
            }
        }

        let mut stack = ScreenStack::new();
        stack.push(Box::new(EventScreen));

        let esc = crate::keys::KeyEventData::from_crossterm(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        ));

        {
            let entry = stack.top_mut().expect("top screen");
            let _ = dispatch_event_tree(&mut entry.widget_tree, None, &Event::Key(esc));
        }

        assert!(
            stack.take_active_dismissal().is_some(),
            "Esc routed into Screen::on_event should stage a dismissal"
        );
    }
}
