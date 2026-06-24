mod devtools;
pub mod dispatch_ctx;
mod event_loop;
mod helpers;
mod render;
mod routing;
mod tasks;
mod timers;
mod types;

// Public re-exports for integration testing via `textual::runtime::*`.
pub use event_loop::resolve_transition_for_property;
pub use helpers::{call_on_mouse_move_tree, tree_content_local_coords, widget_at_tree_layout};
pub use render::{
    apply_text_overflow_to_line, constrain_overlay_position, render_tree_to_frame,
    render_tree_to_frame_with_debug, render_tree_to_frame_with_stylesheet, resolve_axis_constrain,
    run_layout_pass, text_overflow_mode,
};
pub use routing::{
    dispatch_event_to_target_tree, dispatch_event_tree, dispatch_message_queue_tree,
    focused_node_id_tree,
};
pub use types::DispatchOutcome;

use crate::animation::{Animator, animation_level_from_env};
use crate::compose::{ChildDecl, WidgetBuilder};
use crate::css::{StyleSheet, default_widget_stylesheet};
use crate::debug::{DebugLayout, debug_input, debug_render};
use crate::driver::{DriverOptions, KeyboardProtocol, PointerShape, TerminalDriver};
use crate::event::{ActionMap, BindingHint, Event, EventCtx, KeyBind};
use crate::message::MessageEvent;
use crate::node_id::NodeId;
use crate::node_id::node_id_from_ffi;
use crate::node_id::node_id_to_ffi;
use crate::render::FrameBuffer;
use crate::screen::ScreenStack;
use crate::style::{Color, Theme, Visibility};
use crate::widget_tree::{QueryError, WidgetTree};
use crate::widgets::{
    BindingDecl, HelpPanel, SYSTEM_TOOLTIP_STYLE_ID, ToastSeverity, Tooltip, Widget,
    WidgetSelectionAnchor, WidgetStyles,
};
use crate::{Error, Result};
use crossterm::event::{KeyCode, KeyModifiers};
use rich_rs::{Console, ConsoleOptions, MetaValue};
use std::any::Any;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::PathBuf;
#[cfg(unix)]
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tasks::AsyncTaskRuntime;
use timers::OneShotTimerRuntime;
use types::{
    AppNotification, BindingHintEntry, DEFAULT_NOTIFICATION_TIMEOUT, HitTestMap, StylesheetReload,
    StylesheetWatcher,
};

use helpers::{ClickTracker, apply_size, collect_focus_chain_tree, default_action_map};

type SuspendProcessFn = fn() -> io::Result<()>;
type DataBindApplyFn =
    dyn Fn(&mut dyn Widget, &(dyn Any + Send + Sync)) -> bool + Send + Sync + 'static;
const COMMAND_PALETTE_TOOLTIP_COOLDOWN: Duration = Duration::from_millis(500);

#[derive(Clone)]
struct DataBinding {
    key: String,
    selector: String,
    apply: Arc<DataBindApplyFn>,
}

/// Callback invoked when a watched reactive field changes. Receives the app
/// (so the callback can query/mutate widgets) and the new value, type-erased.
type DynamicWatcherFn = dyn Fn(&mut App, &(dyn Any + Send)) + Send + Sync + 'static;

/// A dynamic reactive watcher registered via [`App::watch_reactive`].
#[derive(Clone)]
struct DynamicWatcher {
    target: NodeId,
    field: String,
    callback: Arc<DynamicWatcherFn>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SelectionClickState {
    pub target: NodeId,
    pub button: u8,
    pub screen_x: u16,
    pub screen_y: u16,
    pub at: Instant,
    pub count: u8,
}

#[cfg(unix)]
fn suspend_process_default() -> io::Result<()> {
    let pid = std::process::id().to_string();
    let status = Command::new("kill").arg("-TSTP").arg(&pid).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "`kill -TSTP {pid}` exited with status {status}"
        )))
    }
}

#[cfg(not(unix))]
fn suspend_process_default() -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "app.suspend_process is not supported on this platform",
    ))
}

/// Snapshot-style query result over arena node ids.
#[derive(Debug, Clone)]
pub struct DomQuery {
    nodes: Vec<NodeId>,
}

impl DomQuery {
    fn from_nodes(nodes: Vec<NodeId>) -> Self {
        Self { nodes }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn ids(&self) -> &[NodeId] {
        &self.nodes
    }

    pub fn into_ids(self) -> Vec<NodeId> {
        self.nodes
    }

    pub fn first(&self) -> std::result::Result<NodeId, QueryError> {
        self.nodes.first().copied().ok_or(QueryError::NoMatch)
    }

    pub fn last(&self) -> std::result::Result<NodeId, QueryError> {
        self.nodes.last().copied().ok_or(QueryError::NoMatch)
    }

    pub fn only_one(&self) -> std::result::Result<NodeId, QueryError> {
        match self.nodes.len() {
            0 => Err(QueryError::NoMatch),
            1 => Ok(self.nodes[0]),
            n => Err(QueryError::TooManyMatches(n)),
        }
    }

    pub fn results(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes.iter().copied()
    }

    pub fn results_where(self, app: &App, mut predicate: impl FnMut(&dyn Widget) -> bool) -> Self {
        let Some(tree) = app.widget_tree.as_ref() else {
            return Self::from_nodes(Vec::new());
        };
        let filtered = self
            .nodes
            .into_iter()
            .filter(|id| {
                tree.get(*id)
                    .is_some_and(|node| predicate(node.widget.as_ref()))
            })
            .collect();
        Self::from_nodes(filtered)
    }

    pub fn filter(self, app: &App, selector: &str) -> std::result::Result<Self, QueryError> {
        let matched = app.query(selector)?;
        let matched_set: HashSet<NodeId> = matched.nodes.into_iter().collect();
        let filtered = self
            .nodes
            .into_iter()
            .filter(|id| matched_set.contains(id))
            .collect();
        Ok(Self::from_nodes(filtered))
    }

    pub fn exclude(self, app: &App, selector: &str) -> std::result::Result<Self, QueryError> {
        let matched = app.query(selector)?;
        let matched_set: HashSet<NodeId> = matched.nodes.into_iter().collect();
        let filtered = self
            .nodes
            .into_iter()
            .filter(|id| !matched_set.contains(id))
            .collect();
        Ok(Self::from_nodes(filtered))
    }
}

impl IntoIterator for DomQuery {
    type Item = NodeId;
    type IntoIter = std::vec::IntoIter<NodeId>;

    fn into_iter(self) -> Self::IntoIter {
        self.nodes.into_iter()
    }
}

/// Mutable query handle with chainable bulk mutation helpers.
pub struct DomQueryMut<'a> {
    app: &'a mut App,
    nodes: Vec<NodeId>,
}

impl<'a> DomQueryMut<'a> {
    fn new(app: &'a mut App, nodes: Vec<NodeId>) -> Self {
        Self { app, nodes }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn ids(&self) -> &[NodeId] {
        &self.nodes
    }

    pub fn set_class(self, add: bool, class_names: &[&str]) -> Self {
        if let Some(tree) = self.app.widget_tree.as_mut() {
            for &id in &self.nodes {
                for class in class_names {
                    if add {
                        tree.add_class(id, class);
                    } else {
                        tree.remove_class(id, class);
                    }
                }
            }
        }
        self
    }

    pub fn add_class(self, class: &str) -> Self {
        self.add_classes(&[class])
    }

    pub fn add_classes(self, class_names: &[&str]) -> Self {
        self.set_class(true, class_names)
    }

    pub fn remove_class(self, class: &str) -> Self {
        self.remove_classes(&[class])
    }

    pub fn remove_classes(self, class_names: &[&str]) -> Self {
        self.set_class(false, class_names)
    }

    pub fn toggle_class(self, class: &str) -> Self {
        self.toggle_classes(&[class])
    }

    pub fn toggle_classes(self, class_names: &[&str]) -> Self {
        if let Some(tree) = self.app.widget_tree.as_mut() {
            for &id in &self.nodes {
                for class in class_names {
                    tree.toggle_class(id, class);
                }
            }
        }
        self
    }

    pub fn set_classes(self, classes: &[&str]) -> Self {
        if let Some(tree) = self.app.widget_tree.as_mut() {
            for &id in &self.nodes {
                tree.set_classes(id, classes);
            }
        }
        self
    }

    pub fn update(self, mut f: impl FnMut(&mut dyn Widget)) -> Self {
        if let Some(tree) = self.app.widget_tree.as_mut() {
            for &id in &self.nodes {
                if let Some(node) = tree.get_mut(id) {
                    f(node.widget.as_mut());
                }
            }
        }
        self
    }

    pub fn set_styles(self, f: impl FnMut(&mut WidgetStyles)) -> Self {
        let mut f = f;
        if let Some(tree) = self.app.widget_tree.as_mut() {
            for &id in &self.nodes {
                tree.update_styles(id, |s| f(s));
            }
        }
        self
    }

    pub fn set_focus(self, focused: bool) -> Self {
        if let Some(tree) = self.app.widget_tree.as_mut() {
            for &id in &self.nodes {
                tree.set_focus_state(id, focused);
            }
        }
        self
    }

    pub fn focus(self) -> Self {
        for &id in &self.nodes {
            let eligible = self
                .app
                .with_widget_mut(id, |widget| widget.focusable())
                .unwrap_or(false);
            if eligible {
                let _ = self.app.set_focus_node(id);
                break;
            }
        }
        self
    }

    pub fn blur(self) -> Self {
        let focused = self
            .app
            .widget_tree
            .as_ref()
            .and_then(routing::focused_node_id_tree);
        if let Some(focused_id) = focused
            && self.nodes.contains(&focused_id)
            && let Some(tree) = self.app.widget_tree.as_mut()
        {
            tree.set_focus_state(focused_id, false);
        }
        self
    }

    pub fn set_display(self, display: bool) -> Self {
        if let Some(tree) = self.app.widget_tree.as_mut() {
            for &id in &self.nodes {
                tree.set_runtime_display(id, display);
            }
        }
        self
    }

    pub fn set_visible(self, visible: bool) -> Self {
        if let Some(tree) = self.app.widget_tree.as_mut() {
            let visibility = if visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
            for &id in &self.nodes {
                tree.set_visibility(id, visibility);
            }
        }
        self
    }

    pub fn set(
        self,
        display: Option<bool>,
        visible: Option<bool>,
        disabled: Option<bool>,
        loading: Option<bool>,
    ) -> Self {
        let query = if let Some(display) = display {
            self.set_display(display)
        } else {
            self
        };

        let query = if let Some(visible) = visible {
            query.set_visible(visible)
        } else {
            query
        };

        let query = if let Some(disabled) = disabled {
            if let Some(tree) = query.app.widget_tree.as_mut() {
                for &id in &query.nodes {
                    tree.set_disabled(id, disabled);
                }
            }
            query
        } else {
            query
        };

        if let Some(loading) = loading {
            if let Some(tree) = query.app.widget_tree.as_mut() {
                for &id in &query.nodes {
                    tree.set_loading(id, loading);
                }
            }
            query
        } else {
            query
        }
    }

    pub fn remove(self) -> Self {
        if let Some(tree) = self.app.widget_tree.as_mut() {
            for &id in &self.nodes {
                if tree.contains(id) {
                    tree.remove(id);
                }
            }
        }
        self
    }

    pub fn refresh(self) -> Self {
        self.app.request_query_refresh(&self.nodes);
        self
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
    dark_mode: bool,
    /// Name of the active named theme (default `textual-dark`).
    theme_name: String,
    /// Optional cycle list used by `action_cycle_theme` / `cycle_theme`.
    theme_cycle: Vec<String>,
    theme_cycle_index: usize,
    default_stylesheet: StyleSheet,
    stylesheet: StyleSheet,
    stylesheet_watch: Option<StylesheetWatcher>,
    running: bool,
    hovered: Option<NodeId>,
    tooltip_cooldown_until: Option<Instant>,
    click_tracker: ClickTracker,
    last_render_at: Instant,
    resized_since_last_render: bool,
    clear_on_next_render: bool,
    last_resize_at: Option<Instant>,
    resize_burst: u64,
    sync_output: bool,
    pointer_shape: PointerShape,
    app_active: bool,
    app_inline: bool,
    app_ansi: bool,
    app_nocolor: bool,
    /// Focused widget snapshot captured when app loses terminal focus.
    ///
    /// Mirrors Python Textual app-level blur/refocus behavior: on blur we clear
    /// widget focus and remember the node, then restore it (if still valid)
    /// when focus returns.
    last_focused_on_app_blur: Option<NodeId>,
    last_binding_hints: Vec<BindingHint>,
    last_binding_hint_sources: Vec<NodeId>,
    last_focused_help_source: Option<NodeId>,
    last_focused_help_markup: Option<String>,
    animator: Animator,
    animation_level: crate::event::AnimationLevel,
    notifications: Vec<AppNotification>,
    clipboard: Option<String>,
    active_selection_owner: Option<NodeId>,
    selection_anchor_start: Option<WidgetSelectionAnchor>,
    selection_anchor_end: Option<WidgetSelectionAnchor>,
    selection_drag_active: bool,
    selection_click_state: Option<SelectionClickState>,
    async_tasks: AsyncTaskRuntime,
    one_shot_timers: OneShotTimerRuntime,
    devtools: Option<devtools::DevtoolsRuntime>,
    /// Last resolved CSS style per node, used for automatic style-transition
    /// dispatch (P2-36).
    style_snapshot_cache: HashMap<NodeId, crate::style::Style>,
    /// Pending refresh targets requested via `DomQueryMut::refresh()`.
    pending_query_refresh_nodes: Vec<NodeId>,
    /// Pending subtree recomposition targets requested by widgets via `EventCtx`.
    pending_recompose_nodes: Vec<NodeId>,
    /// Force a full relayout + repaint on the next loop iteration. Set by
    /// runtime-driven structural mutations (dynamic mount/remove) that change
    /// the tree outside the normal dispatch-outcome invalidation flow.
    pending_force_relayout: bool,
    /// App-scoped typed values used by `data_bind`.
    data_values: HashMap<String, Arc<dyn Any + Send + Sync>>,
    /// Declarative data-binding registrations.
    data_bindings: Vec<DataBinding>,
    /// Dynamic reactive watchers registered via [`App::watch_reactive`].
    ///
    /// Mirrors Python `DOMNode.watch(obj, attribute, callback)`: each entry fires
    /// when the named reactive field on `target` changes. Invoked by the runtime
    /// reactive phase after a widget's `reactive_dispatch`.
    dynamic_watchers: Vec<DynamicWatcher>,
    /// Runtime hook used by `action_suspend_process()` (injectable in tests).
    suspend_process_impl: SuspendProcessFn,
    /// Pending highlight clear: (node_id, clear_at_instant).
    /// Set by HIGHLIGHT devtools command, cleared after timeout.
    pending_highlight_clear: Option<(NodeId, std::time::Instant)>,
    /// Callback for `check_action` — set by `TextualAppAdapter` to forward calls
    /// to the app's `TextualApp::check_action()` method. Used by
    /// `dispatch_binding_hints_changed` to set enabled/disabled state on each
    /// binding hint.
    check_action_fn: Option<Arc<dyn Fn(&str, &[String]) -> Option<bool> + Send + Sync>>,
    /// Reactive context for app-level reactive fields.
    ///
    /// `TextualApp` hooks call `app.reactive_ctx()` to record field changes via
    /// reactive setters generated by `#[derive(Reactive)]`. After each hook the
    /// adapter drains pending changes and dispatches them to the app's
    /// `ReactiveWidget` impl via `reactive_widget_mut()`.
    app_reactive_ctx: crate::reactive::ReactiveCtx,
    /// App-level title (mirrors Python `App.title`).
    ///
    /// Setting via `App::set_title()` enqueues a `ScreenTitleChanged` broadcast
    /// so the `Header` widget reflects the new value on the next event pass.
    app_title: String,
    /// App-level sub-title (mirrors Python `App.sub_title`).
    ///
    /// Setting via `App::set_sub_title()` / `App::clear_sub_title()` enqueues a
    /// `ScreenTitleChanged` broadcast, matching Python's reactive `sub_title`.
    app_sub_title: Option<String>,
    /// Messages enqueued by `App::set_title()` / `App::set_sub_title()` to be
    /// dispatched on the next event loop pass.
    ///
    /// Drained in `dispatch_background_runtime_messages()`.
    pending_app_messages: Vec<MessageEvent>,
    /// Arena-based widget tree built from `compose()` declarations.
    ///
    /// Populated during app startup by `build_widget_tree()`. Runtime dispatch,
    /// focus, and layout/render behavior are tree-driven.
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
                    .with_tooltip("Open command palette")
                    .with_namespace("app")
                    .with_group("command_palette")
                    .with_priority(false),
            }),
            theme: Theme::default(),
            dark_mode: true,
            theme_name: "textual-dark".to_string(),
            theme_cycle: Vec::new(),
            theme_cycle_index: 0,
            default_stylesheet: default_widget_stylesheet(),
            stylesheet: StyleSheet::default(),
            stylesheet_watch: None,
            running: true,
            hovered: None,
            tooltip_cooldown_until: None,
            click_tracker: ClickTracker::new(),
            last_render_at: Instant::now(),
            resized_since_last_render: false,
            clear_on_next_render: false,
            last_resize_at: None,
            resize_burst: 0,
            sync_output,
            pointer_shape: PointerShape::Default,
            app_active: true,
            app_inline: false,
            app_ansi: matches!(std::env::var("TEXTUAL_APP_ANSI").ok().as_deref(), Some("1")),
            app_nocolor: matches!(
                std::env::var("TEXTUAL_APP_NOCOLOR").ok().as_deref(),
                Some("1")
            ),
            last_focused_on_app_blur: None,
            last_binding_hints: Vec::new(),
            last_binding_hint_sources: Vec::new(),
            last_focused_help_source: None,
            last_focused_help_markup: None,
            animator: Animator::new(60),
            animation_level: animation_level_from_env(),
            notifications: Vec::new(),
            clipboard: None,
            active_selection_owner: None,
            selection_anchor_start: None,
            selection_anchor_end: None,
            selection_drag_active: false,
            selection_click_state: None,
            async_tasks: AsyncTaskRuntime::default(),
            one_shot_timers: OneShotTimerRuntime::default(),
            devtools: devtools::DevtoolsRuntime::from_env().ok().flatten(),
            style_snapshot_cache: HashMap::new(),
            pending_query_refresh_nodes: Vec::new(),
            pending_recompose_nodes: Vec::new(),
            pending_force_relayout: false,
            data_values: HashMap::new(),
            data_bindings: Vec::new(),
            dynamic_watchers: Vec::new(),
            suspend_process_impl: suspend_process_default,
            pending_highlight_clear: None,
            widget_tree: None,
            screen_stack: ScreenStack::new(),
            modes: HashMap::new(),
            current_mode: None,
            check_action_fn: None,
            app_reactive_ctx: crate::reactive::ReactiveCtx::new(NodeId::default()),
            app_title: String::new(),
            app_sub_title: None,
            pending_app_messages: Vec::new(),
        };
        Ok(app)
    }

    /// Reactive context for app-level `#[derive(Reactive)]` fields.
    ///
    /// `TextualApp` hooks receive `&mut App` and can call `app.reactive_ctx()`
    /// to get a context for reactive setters:
    /// ```ignore
    /// fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
    ///     self.set_count(self.count + 1, app.reactive_ctx());
    /// }
    /// ```
    /// After each hook call the `TextualAppAdapter` drains pending changes and
    /// dispatches them to `reactive_widget_mut()`.
    pub fn reactive_ctx(&mut self) -> &mut crate::reactive::ReactiveCtx {
        &mut self.app_reactive_ctx
    }

    // -----------------------------------------------------------------
    // App-level title / sub-title (mirrors Python App.title / App.sub_title)
    // -----------------------------------------------------------------

    /// Set the app-level title displayed in the `Header` widget.
    ///
    /// Mirrors Python `self.title = value`. Enqueues a `ScreenTitleChanged`
    /// broadcast that reaches the `Header` on the next event loop pass.
    ///
    /// # Example
    /// ```ignore
    /// fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
    ///     app.set_title("Code Browser");
    /// }
    /// ```
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.app_title = title.into();
        self.enqueue_screen_title_changed();
    }

    /// Set the app-level sub-title displayed in the `Header` widget.
    ///
    /// Mirrors Python `self.sub_title = value`. Enqueues a `ScreenTitleChanged`
    /// broadcast that reaches the `Header` on the next event loop pass.
    pub fn set_sub_title(&mut self, sub_title: impl Into<String>) {
        self.app_sub_title = Some(sub_title.into());
        self.enqueue_screen_title_changed();
    }

    /// Clear the app-level sub-title (revert to the Header's default).
    ///
    /// Mirrors Python `self.sub_title = ""`. Enqueues a `ScreenTitleChanged`
    /// broadcast so the Header reverts to its default subtitle.
    pub fn clear_sub_title(&mut self) {
        self.app_sub_title = None;
        self.enqueue_screen_title_changed();
    }

    /// Current app-level title (as last set via `set_title()`).
    pub fn title(&self) -> &str {
        &self.app_title
    }

    /// Current app-level sub-title (as last set via `set_sub_title()`).
    pub fn sub_title(&self) -> Option<&str> {
        self.app_sub_title.as_deref()
    }

    fn enqueue_screen_title_changed(&mut self) {
        use crate::message::ScreenTitleChanged;
        let title = if self.app_title.is_empty() {
            None
        } else {
            Some(self.app_title.clone())
        };
        self.pending_app_messages.push(MessageEvent::new(
            NodeId::default(),
            ScreenTitleChanged {
                title,
                sub_title: self.app_sub_title.clone(),
            },
        ));
    }

    /// Drain messages enqueued by `set_title()` / `set_sub_title()`.
    ///
    /// Called at the start of each background-message pass in the event loop.
    pub(super) fn drain_pending_app_messages(&mut self) -> Vec<MessageEvent> {
        std::mem::take(&mut self.pending_app_messages)
    }

    /// Configure runtime pseudo-class state used by CSS selectors.
    ///
    /// This controls `:inline`, `:ansi`, and `:nocolor` matching during style
    /// resolution. Defaults are `inline=false`, `ansi`/`nocolor` from
    /// `TEXTUAL_APP_ANSI`/`TEXTUAL_APP_NOCOLOR` at startup.
    pub fn set_css_runtime_pseudos(&mut self, inline: bool, ansi: bool, nocolor: bool) {
        self.app_inline = inline;
        self.app_ansi = ansi;
        self.app_nocolor = nocolor;
    }

    /// Return current runtime pseudo-class flags (`inline`, `ansi`, `nocolor`).
    pub fn css_runtime_pseudos(&self) -> (bool, bool, bool) {
        (self.app_inline, self.app_ansi, self.app_nocolor)
    }

    fn validate_selector(selector: &str) -> std::result::Result<(), QueryError> {
        if crate::css::parse_selector_list(selector).is_empty() {
            Err(QueryError::ParseError(format!(
                "invalid selector: {selector}"
            )))
        } else {
            Ok(())
        }
    }

    /// Return the active widget tree (top screen tree when a screen is pushed,
    /// otherwise the app root tree).
    pub(super) fn active_widget_tree(&self) -> Option<&WidgetTree> {
        self.screen_stack
            .top()
            .map(|entry| &entry.widget_tree)
            .or(self.widget_tree.as_ref())
    }

    /// Mutable variant of [`Self::active_widget_tree`].
    pub(super) fn active_widget_tree_mut(&mut self) -> Option<&mut WidgetTree> {
        if let Some(entry) = self.screen_stack.top_mut() {
            return Some(&mut entry.widget_tree);
        }
        self.widget_tree.as_mut()
    }

    /// Return active screen stylesheet override, when a screen is pushed.
    pub(super) fn active_screen_stylesheet(&self) -> Option<&StyleSheet> {
        self.screen_stack
            .top()
            .and_then(|entry| entry.stylesheet.as_ref())
    }

    /// Query nodes in the active arena tree using a CSS selector.
    ///
    /// Returns a snapshot query object in tree traversal order.
    pub fn query(&self, selector: &str) -> std::result::Result<DomQuery, QueryError> {
        match self.active_widget_tree() {
            Some(tree) => tree.query(selector).map(DomQuery::from_nodes),
            None => {
                Self::validate_selector(selector)?;
                Ok(DomQuery::from_nodes(Vec::new()))
            }
        }
    }

    /// Query first matching node (Python `query_one` semantics).
    pub fn query_one(&self, selector: &str) -> std::result::Result<NodeId, QueryError> {
        self.query(selector)?.first()
    }

    /// Query exactly one node; fails when more than one match exists.
    pub fn query_exactly_one(&self, selector: &str) -> std::result::Result<NodeId, QueryError> {
        self.query(selector)?.only_one()
    }

    /// Query one node optionally.
    pub fn query_one_optional(
        &self,
        selector: &str,
    ) -> std::result::Result<Option<NodeId>, QueryError> {
        match self.query(selector)?.first() {
            Ok(id) => Ok(Some(id)),
            Err(QueryError::NoMatch) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Query immediate children of the tree root.
    pub fn query_children(&self, selector: &str) -> std::result::Result<DomQuery, QueryError> {
        match self.active_widget_tree() {
            Some(tree) => match tree.root() {
                Some(root) => tree
                    .query_children(root, selector)
                    .map(DomQuery::from_nodes),
                None => Ok(DomQuery::from_nodes(Vec::new())),
            },
            None => {
                Self::validate_selector(selector)?;
                Ok(DomQuery::from_nodes(Vec::new()))
            }
        }
    }

    /// Query closest ancestor of a node matching a selector.
    pub fn query_ancestor(
        &self,
        node_id: NodeId,
        selector: &str,
    ) -> std::result::Result<NodeId, QueryError> {
        let Some(tree) = self.active_widget_tree() else {
            Self::validate_selector(selector)?;
            return Err(QueryError::NoMatch);
        };
        if !tree.contains(node_id) {
            return Err(QueryError::NoMatch);
        }
        let matched: HashSet<NodeId> = self.query(selector)?.into_ids().into_iter().collect();
        for ancestor in tree.ancestors(node_id) {
            if matched.contains(&ancestor) {
                return Ok(ancestor);
            }
        }
        Err(QueryError::NoMatch)
    }

    /// Find first descendant by CSS id (selector `#id`).
    pub fn get_widget_by_id(&self, id: &str) -> std::result::Result<NodeId, QueryError> {
        self.query_one(&format!("#{id}"))
    }

    /// Find immediate child of the tree root by CSS id.
    pub fn get_child_by_id(&self, id: &str) -> std::result::Result<NodeId, QueryError> {
        self.query_children(&format!("#{id}"))?.first()
    }

    /// Find immediate child of the tree root by widget type.
    pub fn get_child_by_type<T: Widget + 'static>(
        &self,
    ) -> std::result::Result<NodeId, QueryError> {
        let Some(tree) = self.active_widget_tree() else {
            return Err(QueryError::NoMatch);
        };
        let Some(root) = tree.root() else {
            return Err(QueryError::NoMatch);
        };
        for child in tree.children(root) {
            let Some(node) = tree.get(*child) else {
                continue;
            };
            let any_widget = node.widget.as_ref() as &dyn Any;
            if any_widget.is::<T>() {
                return Ok(*child);
            }
        }
        Err(QueryError::NoMatch)
    }

    /// Run multiple tree updates and request a single repaint at the end.
    pub fn batch_update<R>(&mut self, update: impl FnOnce(&mut Self) -> R) -> R {
        let out = update(self);
        self.clear_on_next_render = true;
        out
    }

    /// Mount a widget as a direct child of the active tree root.
    pub fn mount(
        &mut self,
        widget: impl Widget + 'static,
    ) -> std::result::Result<NodeId, QueryError> {
        self.mount_boxed(Box::new(widget))
    }

    /// Mount a boxed widget as a direct child of the active tree root.
    pub fn mount_boxed(
        &mut self,
        widget: Box<dyn Widget>,
    ) -> std::result::Result<NodeId, QueryError> {
        let Some(tree) = self.active_widget_tree_mut() else {
            return Err(QueryError::NoMatch);
        };
        let Some(root) = tree.root() else {
            return Err(QueryError::NoMatch);
        };
        let id = tree.mount(root, widget);
        self.clear_on_next_render = true;
        Ok(id)
    }

    /// Mount multiple widgets as direct children of the active tree root.
    pub fn mount_all(
        &mut self,
        widgets: Vec<Box<dyn Widget>>,
    ) -> std::result::Result<(), QueryError> {
        let Some(tree) = self.active_widget_tree_mut() else {
            return Err(QueryError::NoMatch);
        };
        let Some(root) = tree.root() else {
            return Err(QueryError::NoMatch);
        };
        tree.mount_all(root, widgets);
        self.clear_on_next_render = true;
        Ok(())
    }

    // -- Dynamic mount/remove under a live parent (#stopwatch06) -------------
    //
    // Python parity: `Widget.mount` / `Widget.mount_all` / `Widget.remove`
    // (`../textual/src/textual/widget.py`). These let an already-mounted app
    // insert or remove widgets at runtime (for example
    // `action_add_stopwatch` / `action_remove_stopwatch`).
    //
    // All mounts reuse the canonical arena path
    // ([`mount_extracted_recursive`], src/runtime/mod.rs) so composed
    // children, child-decl metadata (#44), and child handle-sinks fire exactly
    // as on initial build. The freshly inserted `Mount` lifecycle events are
    // drained by the main event loop (src/runtime/event_loop.rs), which then
    // dispatches `Mount` events and routes mount-time messages (#51) via
    // [`Self::drain_pending_mount_messages`]. A relayout + repaint is requested
    // through [`Self::request_query_refresh`] (the same path `Handle::update`
    // uses), so the new subtree lays out and paints.

    /// Mount a widget as the last child of the parent matched by `selector`.
    ///
    /// `selector` is a CSS selector resolved with [`query_one`](Self::query_one)
    /// (e.g. `"#timers"`, `"VerticalScroll"`). Returns the new node's `NodeId`.
    pub fn mount_under(
        &mut self,
        selector: &str,
        widget: impl Widget + 'static,
    ) -> std::result::Result<NodeId, QueryError> {
        let parent = self.query_one(selector)?;
        self.mount_under_node_boxed(parent, Box::new(widget))
    }

    /// Mount a boxed widget as the last child of `parent` (a live `NodeId`).
    pub fn mount_under_node(
        &mut self,
        parent: NodeId,
        widget: impl Widget + 'static,
    ) -> std::result::Result<NodeId, QueryError> {
        self.mount_under_node_boxed(parent, Box::new(widget))
    }

    /// Boxed twin of [`mount_under_node`](Self::mount_under_node).
    pub fn mount_under_node_boxed(
        &mut self,
        parent: NodeId,
        widget: Box<dyn Widget>,
    ) -> std::result::Result<NodeId, QueryError> {
        {
            let Some(tree) = self.active_widget_tree_mut() else {
                return Err(QueryError::NoMatch);
            };
            if !tree.contains(parent) {
                return Err(QueryError::Unmounted);
            }
        }
        let id = {
            let tree = self.active_widget_tree_mut().ok_or(QueryError::NoMatch)?;
            Self::mount_extracted_recursive(tree, parent, widget)
        };
        self.after_structural_mutation(parent);
        Ok(id)
    }

    /// Mount a widget immediately before the sibling matched by `selector`.
    ///
    /// Python parity: `mount(widget, before=...)`. The new node becomes a child
    /// of the sibling's parent, inserted at the sibling's index.
    pub fn mount_before(
        &mut self,
        selector: &str,
        widget: impl Widget + 'static,
    ) -> std::result::Result<NodeId, QueryError> {
        let sibling = self.query_one(selector)?;
        self.mount_relative_to(sibling, widget, false)
    }

    /// Mount a widget immediately after the sibling matched by `selector`.
    ///
    /// Python parity: `mount(widget, after=...)`.
    pub fn mount_after(
        &mut self,
        selector: &str,
        widget: impl Widget + 'static,
    ) -> std::result::Result<NodeId, QueryError> {
        let sibling = self.query_one(selector)?;
        self.mount_relative_to(sibling, widget, true)
    }

    fn mount_relative_to(
        &mut self,
        sibling: NodeId,
        widget: impl Widget + 'static,
        after: bool,
    ) -> std::result::Result<NodeId, QueryError> {
        let (parent, mut index) = {
            let tree = self.active_widget_tree().ok_or(QueryError::NoMatch)?;
            let parent = tree.parent(sibling).ok_or(QueryError::Unmounted)?;
            let index = tree.child_index(parent, sibling).ok_or(QueryError::Unmounted)?;
            (parent, index)
        };
        if after {
            index += 1;
        }
        // Mount at the end via the canonical recursive path (so composed
        // children + decl-meta + sinks fire), then reposition to `index`.
        let id = {
            let tree = self.active_widget_tree_mut().ok_or(QueryError::NoMatch)?;
            let new_id = Self::mount_extracted_recursive(tree, parent, Box::new(widget));
            tree.reorder_child(new_id, index);
            new_id
        };
        self.after_structural_mutation(parent);
        Ok(id)
    }

    /// Remove the node matched by `selector` (and its whole subtree).
    ///
    /// Python parity: `Widget.remove` / `query(...).remove()`. Returns
    /// `Err(QueryError::NoMatch)` if nothing matches, or
    /// `Err(QueryError::TooManyMatches)` if the selector is ambiguous; use
    /// [`remove_node`](Self::remove_node) to remove a specific `NodeId`.
    pub fn remove(&mut self, selector: &str) -> std::result::Result<(), QueryError> {
        let node_id = self.query_one(selector)?;
        self.remove_node(node_id)
    }

    /// Remove a specific node (and its subtree) by `NodeId`.
    ///
    /// Clears focus from any removed node, tears down the subtree (emitting
    /// `Unmount` lifecycle events drained by the event loop), and requests a
    /// relayout + repaint of the former parent.
    pub fn remove_node(&mut self, node_id: NodeId) -> std::result::Result<(), QueryError> {
        let parent = {
            let tree = self.active_widget_tree_mut().ok_or(QueryError::NoMatch)?;
            if !tree.contains(node_id) {
                return Err(QueryError::Unmounted);
            }
            let parent = tree.parent(node_id);
            // Drop focus from the subtree before removal so the loop's
            // focus-transition pass re-derives a valid focus target.
            for id in tree.walk_depth_first(node_id) {
                tree.set_focus_state(id, false);
            }
            tree.remove(node_id);
            parent
        };
        if let Some(parent) = parent {
            self.after_structural_mutation(parent);
        } else {
            self.clear_on_next_render = true;
        }
        Ok(())
    }

    /// Shared post-mount/remove bookkeeping: force a clear + relayout/repaint
    /// of the affected parent subtree. The structural mutation already queued
    /// the `Mount`/`Unmount` lifecycle events that the event loop drains.
    fn after_structural_mutation(&mut self, parent: NodeId) {
        // A structural change (new/removed subtree) can resize auto-sized
        // ancestors, so request a full relayout + full-content repaint on the
        // next loop iteration. `request_query_refresh` alone only dirties
        // content rects (no layout flag), which is insufficient for auto-height
        // parents that grow/shrink with the child count.
        //
        // NOTE: deliberately *not* setting `clear_on_next_render`. A terminal
        // clear diffs the new frame against the *previous* (stale) framebuffer,
        // so unchanged siblings (already painted) would be wiped by the clear
        // but not re-emitted by the diff. A full-content relayout without the
        // clear re-lays everything and the normal diff repaints exactly the
        // moved/added/removed cells while leaving correct cells on screen.
        self.pending_force_relayout = true;
        // Also queue the parent for content refresh so single-frame loops that
        // only consult query-refresh state still observe the change.
        self.request_query_refresh(&[parent]);
    }

    /// Whether a runtime-driven structural mutation requested a full relayout.
    /// Consumed by the event loop after draining query refreshes.
    pub(super) fn take_pending_force_relayout(&mut self) -> bool {
        std::mem::replace(&mut self.pending_force_relayout, false)
    }

    /// Mutable query handle for chainable bulk mutations.
    pub fn query_mut(
        &mut self,
        selector: &str,
    ) -> std::result::Result<DomQueryMut<'_>, QueryError> {
        let nodes = self.query(selector)?.into_ids();
        Ok(DomQueryMut::new(self, nodes))
    }

    /// Store an app-scoped typed value.
    ///
    /// Any registered `data_bind` callbacks for this key are re-applied
    /// immediately after update.
    pub fn set_data<T>(&mut self, key: impl Into<String>, value: T)
    where
        T: Any + Send + Sync + 'static,
    {
        let key = key.into();
        self.data_values.insert(key.clone(), Arc::new(value));
        self.apply_data_bindings_for_key(&key);
    }

    /// Read an app-scoped typed value by key.
    pub fn get_data<T>(&self, key: &str) -> Option<T>
    where
        T: Any + Clone + Send + Sync + 'static,
    {
        self.data_values
            .get(key)
            .and_then(|value| value.as_ref().downcast_ref::<T>())
            .cloned()
    }

    /// Bind a data key to widgets matched by `selector`.
    ///
    /// Whenever `set_data(key, ...)` is called, the binder runs for each
    /// matched widget with the latest typed value.
    pub fn data_bind<T>(
        &mut self,
        key: impl Into<String>,
        selector: impl Into<String>,
        apply: impl Fn(&mut dyn Widget, &T) -> bool + Send + Sync + 'static,
    ) -> std::result::Result<(), QueryError>
    where
        T: Any + Send + Sync + 'static,
    {
        let key = key.into();
        let selector = selector.into();
        Self::validate_selector(&selector)?;
        let wrapped: Arc<DataBindApplyFn> = Arc::new(move |widget, value| {
            value
                .downcast_ref::<T>()
                .map(|typed| apply(widget, typed))
                .unwrap_or(false)
        });
        self.data_bindings.push(DataBinding {
            key: key.clone(),
            selector,
            apply: wrapped,
        });
        self.apply_data_bindings_for_key(&key);
        Ok(())
    }

    /// Apply `add_class` to all nodes matching `selector`.
    ///
    /// Returns the number of matched nodes.
    pub fn action_add_class(
        &mut self,
        selector: &str,
        class_name: &str,
    ) -> std::result::Result<usize, QueryError> {
        let query = self.query_mut(selector)?;
        let matched = query.len();
        query.add_class(class_name);
        Ok(matched)
    }

    /// Apply `remove_class` to all nodes matching `selector`.
    ///
    /// Returns the number of matched nodes.
    pub fn action_remove_class(
        &mut self,
        selector: &str,
        class_name: &str,
    ) -> std::result::Result<usize, QueryError> {
        let query = self.query_mut(selector)?;
        let matched = query.len();
        query.remove_class(class_name);
        Ok(matched)
    }

    /// Apply `toggle_class` to all nodes matching `selector`.
    ///
    /// Returns the number of matched nodes.
    pub fn action_toggle_class(
        &mut self,
        selector: &str,
        class_name: &str,
    ) -> std::result::Result<usize, QueryError> {
        let query = self.query_mut(selector)?;
        let matched = query.len();
        query.toggle_class(class_name);
        Ok(matched)
    }

    fn set_focus_node(&mut self, node_id: NodeId) -> bool {
        let Some(tree) = self.active_widget_tree_mut() else {
            return false;
        };
        if !tree.contains(node_id) || !tree.is_displayed(node_id) {
            return false;
        }
        let current = routing::focused_node_id_tree(tree);
        if current == Some(node_id) {
            return false;
        }
        if let Some(current) = current {
            tree.set_focus_state(current, false);
        }
        if tree.contains(node_id) {
            tree.set_focus_state(node_id, true);
            return true;
        }
        false
    }

    fn focus_first_in_active_tree(&mut self) -> bool {
        let Some(tree) = self.active_widget_tree_mut() else {
            return false;
        };
        let mut focus_chain = collect_focus_chain_tree(tree);
        if focus_chain.is_empty()
            && let Some(root) = tree.root()
        {
            focus_chain = tree
                .walk_depth_first(root)
                .into_iter()
                .filter(|&id| {
                    tree.get(id)
                        .map(|node| node.widget.focusable())
                        .unwrap_or(false)
                })
                .collect();
        }
        let Some(&first) = focus_chain.first() else {
            return false;
        };
        let current = focused_node_id_tree(tree);
        if let Some(current) = current
            && current != first
        {
            tree.set_focus_state(current, false);
        }
        if tree.contains(first) {
            let was_focused = tree.node_state(first).focused;
            tree.set_focus_state(first, true);
            return !was_focused;
        }
        false
    }

    pub fn action_focus(&mut self, widget_id: &str) -> std::result::Result<bool, QueryError> {
        let selector = format!("#{widget_id}");
        let target = match self.query_one(&selector) {
            Ok(id) => id,
            Err(QueryError::NoMatch) => return Ok(false),
            Err(err) => return Err(err),
        };
        Ok(self.set_focus_node(target))
    }

    pub fn action_focus_next(&mut self) -> bool {
        let Some(tree) = self.active_widget_tree_mut() else {
            return false;
        };
        let focus_chain = collect_focus_chain_tree(tree);
        if focus_chain.is_empty() {
            return false;
        }
        let current = routing::focused_node_id_tree(tree);
        let current_index =
            current.and_then(|id| focus_chain.iter().position(|candidate| *candidate == id));
        let next_index = match current_index {
            Some(idx) => (idx + 1) % focus_chain.len(),
            None => 0,
        };
        self.set_focus_node(focus_chain[next_index])
    }

    pub fn action_focus_previous(&mut self) -> bool {
        let Some(tree) = self.active_widget_tree_mut() else {
            return false;
        };
        let focus_chain = collect_focus_chain_tree(tree);
        if focus_chain.is_empty() {
            return false;
        }
        let current = routing::focused_node_id_tree(tree);
        let current_index =
            current.and_then(|id| focus_chain.iter().position(|candidate| *candidate == id));
        let next_index = match current_index {
            Some(0) | None => focus_chain.len() - 1,
            Some(idx) => idx - 1,
        };
        self.set_focus_node(focus_chain[next_index])
    }

    pub fn action_help_quit(&mut self) {
        self.notify_help_quit();
    }

    pub fn action_copy_selected_text(&mut self) -> Option<String> {
        self.validate_active_selection_owner();
        self.selected_text()
    }

    pub fn action_notify(&mut self, message: &str, title: &str, severity: &str) {
        let severity = match severity.to_ascii_lowercase().as_str() {
            "warning" => ToastSeverity::Warning,
            "error" => ToastSeverity::Error,
            _ => ToastSeverity::Information,
        };
        self.notify(message.to_string(), title.to_string(), severity, None);
    }

    pub(super) fn selected_text(&mut self) -> Option<String> {
        if let Some(owner) = self.active_selection_owner {
            let selected = self.with_widget_mut(owner, |widget| widget.get_selection())?;
            let selected = selected?;
            if !selected.trim().is_empty() {
                return Some(selected);
            }
        }

        let focused = self.active_widget_tree().and_then(focused_node_id_tree)?;
        let selected = self
            .with_widget_mut(focused, |widget| widget.get_selection())
            .flatten()?;
        if selected.trim().is_empty() {
            None
        } else {
            Some(selected)
        }
    }

    pub(super) fn clear_active_selection(&mut self) -> bool {
        self.selection_drag_active = false;
        self.selection_anchor_start = None;
        self.selection_anchor_end = None;
        let Some(owner) = self.active_selection_owner.take() else {
            return false;
        };
        self.with_widget_mut(owner, |widget| widget.clear_selection())
            .unwrap_or(false)
    }

    pub(super) fn begin_selection_drag(&mut self, target: NodeId, x: u16, y: u16) -> Option<bool> {
        let anchor = self.with_widget_mut(target, |widget| {
            if !widget.allow_select() {
                return None;
            }
            widget.selection_at(x, y)
        })??;

        let _ = self.clear_active_selection();
        self.active_selection_owner = Some(target);
        self.selection_drag_active = true;
        self.selection_anchor_start = Some(anchor);
        self.selection_anchor_end = Some(anchor);

        self.with_widget_mut(target, |widget| {
            let changed = widget.update_selection(anchor, anchor);
            if changed {
                let mut selection_ctx = EventCtx::default();
                selection_ctx.set_node_id(target);
                widget.selection_updated(&mut selection_ctx);
            }
            changed
        })
    }

    pub(super) fn update_selection_drag(&mut self, target: NodeId, x: u16, y: u16) -> Option<bool> {
        let owner = self.active_selection_owner?;
        if owner != target || !self.selection_drag_active {
            return None;
        }
        let from = self.selection_anchor_start?;
        let anchor = self
            .with_widget_mut(target, |widget| widget.selection_at(x, y))
            .flatten()?;
        self.selection_anchor_end = Some(anchor);
        Some(self.with_widget_mut(target, |widget| {
            let changed = widget.update_selection(from, anchor);
            if changed {
                let mut selection_ctx = EventCtx::default();
                selection_ctx.set_node_id(target);
                widget.selection_updated(&mut selection_ctx);
            }
            changed
        })?)
    }

    pub(super) fn end_selection_drag(&mut self) {
        self.selection_drag_active = false;
    }

    pub(super) fn validate_active_selection_owner(&mut self) {
        let Some(owner) = self.active_selection_owner else {
            return;
        };
        let still_valid = self
            .active_widget_tree()
            .and_then(|tree| tree.get(owner))
            .is_some_and(|node| {
                node.display && node.visibility == Visibility::Visible && node.widget.allow_select()
            });
        if !still_valid {
            self.active_selection_owner = None;
            self.selection_anchor_start = None;
            self.selection_anchor_end = None;
            self.selection_drag_active = false;
        }
    }

    pub(super) fn register_selection_click(
        &mut self,
        target: NodeId,
        button: u8,
        screen_x: u16,
        screen_y: u16,
    ) -> u8 {
        const MULTI_CLICK_MAX_DELAY: Duration = Duration::from_millis(500);
        const MULTI_CLICK_MAX_DISTANCE: u16 = 1;

        let now = Instant::now();
        let mut next_count = 1u8;
        if let Some(prev) = self.selection_click_state
            && prev.target == target
            && prev.button == button
            && now.saturating_duration_since(prev.at) <= MULTI_CLICK_MAX_DELAY
            && prev.screen_x.abs_diff(screen_x) <= MULTI_CLICK_MAX_DISTANCE
            && prev.screen_y.abs_diff(screen_y) <= MULTI_CLICK_MAX_DISTANCE
        {
            next_count = prev.count.saturating_add(1).min(3);
        }
        self.selection_click_state = Some(SelectionClickState {
            target,
            button,
            screen_x,
            screen_y,
            at: now,
            count: next_count,
        });
        next_count
    }

    pub(super) fn clear_selection_click_streak(&mut self) {
        self.selection_click_state = None;
    }

    pub(super) fn select_word_at(&mut self, target: NodeId, x: u16, y: u16) -> Option<bool> {
        let (from, to) =
            self.with_widget_mut(target, |widget| widget.selection_word_range_at(x, y))??;
        let _ = self.clear_active_selection();
        self.active_selection_owner = Some(target);
        self.selection_anchor_start = Some(from);
        self.selection_anchor_end = Some(to);
        self.selection_drag_active = false;
        self.with_widget_mut(target, |widget| {
            let changed = widget.update_selection(from, to);
            if changed {
                let mut selection_ctx = EventCtx::default();
                selection_ctx.set_node_id(target);
                widget.selection_updated(&mut selection_ctx);
            }
            changed
        })
    }

    pub(super) fn select_all_at_target(&mut self, target: NodeId) -> Option<bool> {
        let (from, to) = self.with_widget_mut(target, |widget| widget.selection_all_range())??;
        let _ = self.clear_active_selection();
        self.active_selection_owner = Some(target);
        self.selection_anchor_start = Some(from);
        self.selection_anchor_end = Some(to);
        self.selection_drag_active = false;
        self.with_widget_mut(target, |widget| {
            let changed = widget.update_selection(from, to);
            if changed {
                let mut selection_ctx = EventCtx::default();
                selection_ctx.set_node_id(target);
                widget.selection_updated(&mut selection_ctx);
            }
            changed
        })
    }

    pub fn action_back(&mut self) -> bool {
        self.pop_screen().is_some()
    }

    pub fn action_push_screen(&mut self, screen: &str) -> bool {
        let Some(factory) = self.modes.get(screen) else {
            return false;
        };
        self.push_screen(factory());
        true
    }

    pub fn action_pop_screen(&mut self) -> bool {
        self.pop_screen().is_some()
    }

    pub fn action_switch_screen(&mut self, screen: &str) -> bool {
        let Some(factory) = self.modes.get(screen) else {
            return false;
        };
        let screen = factory();
        let _ = self.pop_screen();
        self.push_screen(screen);
        true
    }

    pub fn action_hide_help_panel(&mut self) -> std::result::Result<bool, QueryError> {
        let ids = self.query("HelpPanel")?.into_ids();
        if ids.is_empty() {
            return Ok(false);
        }
        if let Some(tree) = self.active_widget_tree_mut() {
            for id in ids {
                tree.remove(id);
            }
            return Ok(true);
        }
        Ok(false)
    }

    pub fn action_show_help_panel(&mut self) -> std::result::Result<bool, QueryError> {
        if !self.query("HelpPanel")?.is_empty() {
            return Ok(false);
        }
        let mut mount_parent = match self.active_widget_tree().and_then(|tree| tree.root()) {
            Some(root) => root,
            None => return Ok(false),
        };

        if let Ok(command_palette_ids) = self.query("CommandPalette")
            && let Some(command_palette_id) = command_palette_ids.into_ids().first().copied()
            && let Some(tree) = self.active_widget_tree()
            && let Some(adapter_id) = tree.get(command_palette_id).and_then(|node| node.parent)
            && let Some(app_content_id) = tree.children(adapter_id).first().copied()
        {
            // In TextualApp runtime roots, CommandPalette is hosted as the second child
            // of the adapter (first child = normal app subtree). Mount HelpPanel into
            // that app subtree so CommandPalette remains the top-most overlay.
            mount_parent = app_content_id;
        }

        if let Some(tree) = self.active_widget_tree_mut() {
            tree.mount(mount_parent, Box::new(HelpPanel::new()));
            // A newly mounted HelpPanel needs a fresh broadcast of binding hints and
            // focused-help payload, even when those values are unchanged.
            self.last_binding_hints.clear();
            self.last_binding_hint_sources.clear();
            self.last_focused_help_source = None;
            self.last_focused_help_markup = None;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn action_toggle_dark(&mut self) -> bool {
        self.dark_mode = !self.dark_mode;
        let mut base = crate::style::Style::new();
        if self.dark_mode {
            if let Some(bg) = crate::style::parse_color_like("$background") {
                base = base.bg(bg);
            }
            if let Some(fg) = crate::style::parse_color_like("$foreground") {
                base = base.fg(fg);
            }
        } else {
            base = base
                .bg(Color::rgb(245, 245, 245))
                .fg(Color::rgb(20, 20, 20));
        }
        self.theme = Theme::new().base(base);
        true
    }

    pub fn action_change_theme(&mut self) -> bool {
        self.action_toggle_dark()
    }

    pub fn action_bell(&mut self) -> bool {
        self.console.write_str("\x07").is_ok()
    }

    pub fn action_suspend_process(&mut self) -> bool {
        let was_started = self.driver.started();
        if was_started && self.driver.stop().is_err() {
            self.action_notify(
                "Failed to suspend process: could not stop terminal driver cleanly",
                "Suspend process",
                "error",
            );
            return true;
        }

        let suspend = (self.suspend_process_impl)();

        if was_started {
            if let Err(err) = self.driver.start() {
                self.action_notify(
                    &format!("Failed to resume terminal after suspend: {err}"),
                    "Suspend process",
                    "error",
                );
                return true;
            }
            if let Err(err) = self.refresh_size() {
                self.action_notify(
                    &format!("Failed to refresh terminal size after resume: {err}"),
                    "Suspend process",
                    "error",
                );
                return true;
            }
            let _ = self.set_pointer_shape(PointerShape::Default);
            self.clear_on_next_render = true;
        }

        match suspend {
            Ok(()) => true,
            Err(err) => {
                let severity = if err.kind() == io::ErrorKind::Unsupported {
                    "warning"
                } else {
                    "error"
                };
                self.action_notify(
                    &format!("Process suspend is unavailable: {err}"),
                    "Suspend process",
                    severity,
                );
                true
            }
        }
    }

    pub fn action_screenshot(&mut self, filename: Option<&str>, path: Option<&str>) -> bool {
        let file_name = filename
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("screenshot.svg");
        let output = if let Some(path) = path {
            PathBuf::from(path).join(file_name)
        } else {
            PathBuf::from(file_name)
        };
        let result = self.console.save_svg(
            output.to_string_lossy().as_ref(),
            "textual-rs screenshot",
            None,
            true,
            0.61,
            None,
        );
        match result {
            Ok(()) => {
                self.action_notify(
                    &format!("Saved screenshot to {}", output.display()),
                    "Screenshot",
                    "information",
                );
                true
            }
            Err(err) => {
                self.action_notify(
                    &format!("Failed to save screenshot: {err}"),
                    "Screenshot",
                    "error",
                );
                true
            }
        }
    }

    fn apply_data_bindings_for_key(&mut self, key: &str) {
        let Some(value) = self.data_values.get(key).cloned() else {
            return;
        };
        let bindings: Vec<DataBinding> = self
            .data_bindings
            .iter()
            .filter(|binding| binding.key == key)
            .cloned()
            .collect();
        if bindings.is_empty() {
            return;
        }

        let mut refresh_nodes: HashSet<NodeId> = HashSet::new();
        for binding in bindings {
            let node_ids = match self.query(&binding.selector) {
                Ok(query) => query.into_ids(),
                Err(_) => continue,
            };
            for node_id in node_ids {
                let changed = self
                    .with_widget_mut(node_id, |widget| (binding.apply)(widget, value.as_ref()))
                    .unwrap_or(false);
                if changed {
                    refresh_nodes.insert(node_id);
                }
            }
        }

        if !refresh_nodes.is_empty() {
            let refresh_nodes = refresh_nodes.into_iter().collect::<Vec<_>>();
            self.request_query_refresh(&refresh_nodes);
        }
    }

    pub(crate) fn request_query_refresh(&mut self, nodes: &[NodeId]) {
        let queued: Vec<NodeId> = {
            let Some(tree) = self.active_widget_tree() else {
                self.clear_on_next_render = true;
                return;
            };

            let mut queued = Vec::new();
            for &id in nodes {
                if !tree.contains(id) {
                    continue;
                }
                queued.extend(tree.walk_depth_first(id));
            }
            queued
        };

        if queued.is_empty() {
            self.clear_on_next_render = true;
            return;
        }

        for id in queued {
            if !self.pending_query_refresh_nodes.contains(&id) {
                self.pending_query_refresh_nodes.push(id);
            }
        }
    }

    pub(super) fn take_pending_query_refresh_nodes(&mut self) -> Vec<NodeId> {
        std::mem::take(&mut self.pending_query_refresh_nodes)
    }

    pub(super) fn request_widget_recompose_nodes(&mut self, nodes: &[NodeId]) {
        let queued: Vec<NodeId> = {
            let Some(tree) = self.active_widget_tree() else {
                self.clear_on_next_render = true;
                return;
            };
            nodes
                .iter()
                .copied()
                .filter(|node_id| tree.contains(*node_id))
                .collect()
        };
        if queued.is_empty() {
            self.clear_on_next_render = true;
            return;
        }
        for node_id in queued {
            if !self.pending_recompose_nodes.contains(&node_id) {
                self.pending_recompose_nodes.push(node_id);
            }
        }
    }

    pub(super) fn take_pending_recompose_nodes(&mut self) -> Vec<NodeId> {
        std::mem::take(&mut self.pending_recompose_nodes)
    }

    /// Recompose the application root from a freshly composed [`AppRoot`].
    ///
    /// Mirrors Python Textual's `reactive(recompose=True)` at the `App`/`Screen`
    /// level (e.g. `recompose01.py`, `set_reactive03.py`): when an app-level
    /// reactive changes, `compose()` is re-invoked and the screen's children are
    /// rebuilt. The `TextualAppAdapter` produces the fresh `AppRoot` (by calling
    /// the app's `compose()`), and this method swaps it into the app-content node
    /// and remounts its subtree.
    ///
    /// The app-content node is the first child of the adapter root. Its widget is
    /// replaced with the new `AppRoot` (children intact), then the standard
    /// subtree-recompose path runs — which removes the old children (including
    /// scrollbars) and remounts from the fresh widget, exactly as on first mount.
    ///
    /// Returns `true` if the recompose was applied.
    pub fn recompose_app(&mut self, fresh_root: crate::widgets::AppRoot) -> bool {
        let app_content_id = match self.app_content_node_id() {
            Some(id) => id,
            None => return false,
        };
        let Some(tree) = self.active_widget_tree_mut() else {
            return false;
        };
        let Some(node) = tree.get_mut(app_content_id) else {
            return false;
        };
        node.widget = Box::new(fresh_root);
        crate::runtime::event_loop::recompose_node_subtree(tree, app_content_id);
        true
    }

    /// The node id of the app-content root (the `AppRoot` mounted as the first
    /// child of the runtime adapter root). Returns `None` if the tree is not
    /// built or has no children.
    pub(crate) fn app_content_node_id(&self) -> Option<NodeId> {
        let tree = self.active_widget_tree()?;
        let root = tree.root()?;
        tree.children(root).first().copied()
    }

    /// Register a dynamic watcher on a reactive field of another node.
    ///
    /// Mirrors Python `DOMNode.watch(obj, attribute, callback)`: `callback` is
    /// invoked with the new value each time the `field` reactive on `target`
    /// changes (during the runtime reactive phase, after the target's
    /// `reactive_dispatch`). The callback receives `&mut App`, so it can query and
    /// mutate other widgets — matching Python watchers that call `self.query_one`.
    ///
    /// The value passed to `callback` is type-erased; downcast it to the field's
    /// type with `value.downcast_ref::<T>()`.
    pub fn watch_reactive<F>(&mut self, target: NodeId, field: impl Into<String>, callback: F)
    where
        F: Fn(&mut App, &(dyn Any + Send)) + Send + Sync + 'static,
    {
        self.dynamic_watchers.push(DynamicWatcher {
            target,
            field: field.into(),
            callback: Arc::new(callback),
        });
    }

    /// Whether any dynamic watcher is registered for `target`+`field`.
    pub(crate) fn has_dynamic_watcher(&self, target: NodeId, field: &str) -> bool {
        self.dynamic_watchers
            .iter()
            .any(|w| w.target == target && w.field == field)
    }

    /// Fire all dynamic watchers registered for `target`+`field` with `value`.
    ///
    /// Callbacks are cloned out (Arc clone) before invocation so the borrow on
    /// `self.dynamic_watchers` is released while `&mut self` is handed to each
    /// callback (same pattern as `data_bind`).
    pub(crate) fn notify_dynamic_watchers(
        &mut self,
        target: NodeId,
        field: &str,
        value: &(dyn Any + Send),
    ) {
        let callbacks: Vec<Arc<DynamicWatcherFn>> = self
            .dynamic_watchers
            .iter()
            .filter(|w| w.target == target && w.field == field)
            .map(|w| Arc::clone(&w.callback))
            .collect();
        for callback in callbacks {
            callback(self, value);
        }
    }

    #[cfg(test)]
    pub(crate) fn set_suspend_process_impl_for_test(&mut self, f: SuspendProcessFn) {
        self.suspend_process_impl = f;
    }

    /// Borrow a widget mutably by node id for a scoped update.
    pub fn with_widget_mut<R>(
        &mut self,
        node_id: NodeId,
        f: impl FnOnce(&mut dyn Widget) -> R,
    ) -> Option<R> {
        let tree = self.active_widget_tree_mut()?;
        let node = tree.get_mut(node_id)?;
        let result = f(node.widget.as_mut());
        // Drain any pending class ops that widget methods may have staged on the
        // widget struct (e.g. MarkdownViewer.toc_class_pending). These are ops the
        // widget cannot apply itself because it has no EventCtx reference.
        let pending = node.widget.drain_pending_class_ops();
        if !pending.is_empty() {
            for (class, add) in pending {
                if add {
                    tree.add_class(node_id, &class);
                } else {
                    tree.remove_class(node_id, &class);
                }
            }
        }
        Some(result)
    }

    /// Borrow a widget mutably by node id and downcast to `T`.
    pub fn with_widget_mut_as<T: Widget + 'static, R>(
        &mut self,
        node_id: NodeId,
        f: impl FnOnce(&mut T) -> R,
    ) -> Option<R> {
        self.with_widget_mut(node_id, |widget| {
            let any_widget = widget as &mut dyn Any;
            any_widget.downcast_mut::<T>().map(f)
        })
        .flatten()
    }

    /// Query one widget by selector and borrow it mutably for a scoped update.
    ///
    /// Escape hatch; prefer `query_one_typed` + `Handle` for typed single-widget access.
    pub fn with_query_one_mut<R>(
        &mut self,
        selector: &str,
        f: impl FnOnce(&mut dyn Widget) -> R,
    ) -> std::result::Result<R, QueryError> {
        let node_id = self.query_one(selector)?;
        self.with_widget_mut(node_id, f).ok_or(QueryError::NoMatch)
    }

    /// Query one widget by selector and mutably downcast it to `T`.
    ///
    /// Escape hatch; prefer `query_one_typed` + `Handle` for typed single-widget access.
    pub fn with_query_one_mut_as<T: Widget + 'static, R>(
        &mut self,
        selector: &str,
        f: impl FnOnce(&mut T) -> R,
    ) -> std::result::Result<R, QueryError> {
        let node_id = self.query_one(selector)?;
        self.with_widget_mut_as(node_id, f)
            .ok_or(QueryError::NoMatch)
    }

    /// Typed `query_one` upgrade: selector must match exactly one node whose
    /// concrete type is `W`.
    ///
    /// Typed wrapper over the same arena access as `with_widget_mut_as`;
    /// for imperative widget APIs. Application state belongs in reactive
    /// fields/signals (RA-3).
    pub fn query_one_typed<W: Widget>(&self, selector: &str) -> std::result::Result<crate::handle::Handle<W>, QueryError> {
        let node_id = self.query_one(selector)?;
        self.typed_handle::<W>(node_id)
    }

    /// Checked typed upgrade of a NodeId in the active tree.
    ///
    /// Typed wrapper over the same arena access as `with_widget_mut_as`;
    /// for one-off access to a `NodeId` from a message (e.g. `MessageEvent.sender`).
    pub fn typed_handle<W: Widget>(&self, node_id: NodeId) -> std::result::Result<crate::handle::Handle<W>, QueryError> {
        let tree = self.active_widget_tree().ok_or(QueryError::Unmounted)?;
        crate::handle::Handle::<W>::resolve(tree, node_id)
    }

    /// Mount a widget as a direct child of the active tree root and return a
    /// typed handle to it (typed twin of `App::mount`, src/runtime/mod.rs:882).
    pub fn mount_typed<W: Widget>(&mut self, widget: W) -> std::result::Result<crate::handle::Handle<W>, QueryError> {
        let node_id = self.mount(widget).map_err(|_| QueryError::Unmounted)?;
        let tree = self.active_widget_tree().ok_or(QueryError::Unmounted)?;
        Ok(crate::handle::Handle::new(node_id, tree.tree_id()))
    }

    /// Plumbing for `Handle::read` (active_widget_tree is pub(super)).
    pub(crate) fn handle_read<W: Widget, R>(
        &self,
        handle: crate::handle::Handle<W>,
        f: impl FnOnce(&W) -> R,
    ) -> std::result::Result<R, QueryError> {
        let tree = self.active_widget_tree().ok_or(QueryError::Unmounted)?;
        handle.read_in(tree, f)
    }

    /// Plumbing for `Handle::update`: tree-level update (enqueues the reactive
    /// entry) + automatic subtree repaint via the same path as
    /// `DomQueryMut::refresh` (src/runtime/mod.rs:415).
    ///
    /// Also drains any pending class ops staged by widget methods (e.g.
    /// `MarkdownViewer.toc_class_pending`) — the same post-mutation step
    /// that `with_widget_mut` performs (src/runtime/mod.rs:1623).
    pub(crate) fn handle_update<W: Widget, R>(
        &mut self,
        handle: crate::handle::Handle<W>,
        f: impl FnOnce(&mut W, &mut crate::reactive::ReactiveCtx) -> R,
    ) -> std::result::Result<R, QueryError> {
        let out = {
            let tree = self.active_widget_tree_mut().ok_or(QueryError::Unmounted)?;
            let out = handle.update_in(tree, f)?;
            // Drain pending class ops (same as with_widget_mut, src/runtime/mod.rs:1623).
            let node_id = handle.node_id();
            if let Some(node) = tree.get_mut(node_id) {
                let pending = node.widget.drain_pending_class_ops();
                if !pending.is_empty() {
                    for (class, add) in pending {
                        if add {
                            tree.add_class(node_id, &class);
                        } else {
                            tree.remove_class(node_id, &class);
                        }
                    }
                }
            }
            out
        };
        self.request_query_refresh(&[handle.node_id()]);
        Ok(out)
    }

    /// Plumbing for `Handle::is_mounted`.
    pub(crate) fn handle_is_mounted<W: Widget>(&self, handle: crate::handle::Handle<W>) -> bool {
        self.active_widget_tree()
            .map(|tree| handle.is_mounted_in(tree))
            .unwrap_or(false)
    }

    /// Build the arena-based widget tree by extracting children from the root widget.
    ///
    /// Uses `take_composed_children()` to recursively move children out of
    /// containers and into the arena tree. After building, the tree is stored
    /// in `self.widget_tree` and tree mode becomes active.
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

        Self::mount_system_tooltip(&mut tree, root_node_id);

        // Drain lifecycle events from initial build (mount events) — the
        // runtime will call on_mount separately via the existing path.
        let _ = tree.drain_lifecycle();
        self.widget_tree = Some(tree);
    }

    /// Recursively extract children from a widget via `take_composed_children()`
    /// and mount them into the tree. Fires handle sinks for any bound children.
    fn extract_children_to_tree(tree: &mut WidgetTree, parent: NodeId, widget: &mut dyn Widget) {
        let children = widget.take_composed_children();
        let mut sinks: std::collections::HashMap<usize, crate::handle::HandleSink> =
            widget.take_child_handle_sinks().into_iter().collect();
        let mut decl_meta: std::collections::HashMap<usize, (Option<String>, Vec<String>)> = widget
            .take_child_decl_meta()
            .into_iter()
            .map(|(index, id, classes)| (index, (id, classes)))
            .collect();
        for (index, mut child) in children.into_iter().enumerate() {
            // Recursively extract grandchildren before mounting the child.
            // We must do this while we still have &mut access to the child.
            let grandchildren = child.take_composed_children();
            let mut grandchild_sinks: std::collections::HashMap<usize, crate::handle::HandleSink> =
                child.take_child_handle_sinks().into_iter().collect();
            let mut grandchild_meta: std::collections::HashMap<usize, (Option<String>, Vec<String>)> =
                child
                    .take_child_decl_meta()
                    .into_iter()
                    .map(|(g_index, id, classes)| (g_index, (id, classes)))
                    .collect();
            // Also collect compose() declarations from the child.
            let child_compose = child.compose();

            let node_id = tree.mount(parent, child);

            // Apply compose-time CSS id/classes recorded on this declaration.
            if let Some((id, classes)) = decl_meta.remove(&index) {
                crate::widgets::apply_child_decl_meta(tree, node_id, id, &classes);
            }

            // Fire this child's sink if one was recorded.
            if let Some(sink) = sinks.remove(&index) {
                sink(node_id, tree.tree_id());
            }

            // Recursively mount grandchildren under this node.
            for (g_index, grandchild) in grandchildren.into_iter().enumerate() {
                let g_id = Self::mount_extracted_recursive(tree, node_id, grandchild);
                if let Some((id, classes)) = grandchild_meta.remove(&g_index) {
                    crate::widgets::apply_child_decl_meta(tree, g_id, id, &classes);
                }
                if let Some(sink) = grandchild_sinks.remove(&g_index) {
                    sink(g_id, tree.tree_id());
                }
            }

            // Mount compose() declarations from the child.
            if !child_compose.is_empty() {
                Self::mount_declarations(tree, node_id, child_compose);
            }
        }
    }

    /// Recursively mount an already-extracted child widget and its descendants.
    /// Returns the `NodeId` of the newly mounted node.
    pub(crate) fn mount_extracted_recursive(
        tree: &mut WidgetTree,
        parent: NodeId,
        mut widget: Box<dyn Widget>,
    ) -> NodeId {
        let grandchildren = widget.take_composed_children();
        let mut grandchild_sinks: std::collections::HashMap<usize, crate::handle::HandleSink> =
            widget.take_child_handle_sinks().into_iter().collect();
        let mut grandchild_meta: std::collections::HashMap<usize, (Option<String>, Vec<String>)> =
            widget
                .take_child_decl_meta()
                .into_iter()
                .map(|(g_index, id, classes)| (g_index, (id, classes)))
                .collect();
        let compose_decls = widget.compose();

        let node_id = tree.mount(parent, widget);

        for (g_index, grandchild) in grandchildren.into_iter().enumerate() {
            let g_id = Self::mount_extracted_recursive(tree, node_id, grandchild);
            if let Some((id, classes)) = grandchild_meta.remove(&g_index) {
                crate::widgets::apply_child_decl_meta(tree, g_id, id, &classes);
            }
            if let Some(sink) = grandchild_sinks.remove(&g_index) {
                sink(g_id, tree.tree_id());
            }
        }

        if !compose_decls.is_empty() {
            Self::mount_declarations(tree, node_id, compose_decls);
        }

        node_id
    }

    /// Recursively mount `ChildDecl` declarations into the tree under `parent`.
    /// Fires handle sinks recorded on declarations via `HandleSlot::bind`.
    pub(crate) fn mount_declarations(
        tree: &mut WidgetTree,
        parent: NodeId,
        declarations: Vec<ChildDecl>,
    ) {
        for decl in declarations {
            let ChildDecl {
                builder,
                children: decl_children,
                id,
                classes,
                handle_sink,
            } = decl;
            let WidgetBuilder::Ready(mut widget) = builder;
            // Extract children from declared widgets too.
            let extracted = widget.take_composed_children();
            let mut extracted_sinks: std::collections::HashMap<usize, crate::handle::HandleSink> =
                widget.take_child_handle_sinks().into_iter().collect();
            let mut extracted_meta: std::collections::HashMap<usize, (Option<String>, Vec<String>)> =
                widget
                    .take_child_decl_meta()
                    .into_iter()
                    .map(|(index, id, classes)| (index, (id, classes)))
                    .collect();
            let child_compose = widget.compose();
            let node_id = tree.mount(parent, widget);
            // Fire the decl's handle sink if one was set via HandleSlot::bind.
            if let Some(sink) = handle_sink {
                sink(node_id, tree.tree_id());
            }
            // Apply CSS id from the declaration via the tree after mount.
            if let Some(id_str) = &id {
                tree.set_css_id(node_id, Some(id_str.clone()));
            }
            for class in &classes {
                tree.add_class(node_id, class);
            }
            // Mount extracted children first.
            for (index, child) in extracted.into_iter().enumerate() {
                let c_id = Self::mount_extracted_recursive(tree, node_id, child);
                if let Some((id, classes)) = extracted_meta.remove(&index) {
                    crate::widgets::apply_child_decl_meta(tree, c_id, id, &classes);
                }
                if let Some(sink) = extracted_sinks.remove(&index) {
                    sink(c_id, tree.tree_id());
                }
            }
            // Mount explicit child declarations.
            if !decl_children.is_empty() {
                Self::mount_declarations(tree, node_id, decl_children);
            }
            // Then mount compose() declarations from the widget itself.
            if !child_compose.is_empty() {
                Self::mount_declarations(tree, node_id, child_compose);
            }
        }
    }

    pub(crate) fn mount_system_tooltip(tree: &mut WidgetTree, root: NodeId) -> NodeId {
        let tooltip = Tooltip::system();
        let tooltip_id = tree.mount(root, Box::new(tooltip));
        tree.set_css_id(tooltip_id, Some(SYSTEM_TOOLTIP_STYLE_ID.to_string()));
        tree.set_runtime_display(tooltip_id, false);
        tooltip_id
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
        MessageEvent::new(
            sender,
            crate::message::TextEditClipboardPaste { target, text },
        )
        .with_control(sender)
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

    /// Register (or replace) a named theme in the global catalog.
    ///
    /// Mirrors Python `App.register_theme`.
    pub fn register_theme(&mut self, theme: crate::theme::NamedTheme) {
        crate::theme::register_theme(theme);
    }

    /// Names of all registered themes, sorted (Python `App.available_themes`).
    pub fn available_themes(&self) -> Vec<String> {
        crate::theme::available_theme_names()
    }

    /// The currently active theme name.
    pub fn theme_name(&self) -> &str {
        &self.theme_name
    }

    /// Activate a named theme by name (Python `App.theme = name`).
    ///
    /// Re-colors the UI by swapping the active design-token map and rebuilding
    /// the base surface Style. Returns `false` if no such theme is registered.
    pub fn set_theme_by_name(&mut self, name: &str) -> bool {
        if !crate::theme::set_active_theme(name) {
            return false;
        }
        self.theme_name = name.to_string();
        self.dark_mode = crate::theme::get_theme(name).map(|t| t.dark).unwrap_or(true);
        self.rebuild_base_from_active_theme();
        true
    }

    /// Rebuild the runtime base Style (`$background`/`$foreground`) from the
    /// active theme's tokens.
    fn rebuild_base_from_active_theme(&mut self) {
        let mut base = crate::style::Style::new();
        if let Some(bg) = crate::style::parse_color_like("$background") {
            base = base.bg(bg);
        }
        if let Some(fg) = crate::style::parse_color_like("$foreground") {
            base = base.fg(fg);
        }
        self.theme = Theme::new().base(base);
    }

    /// Set the ordered list of theme names cycled by `action_cycle_theme`.
    ///
    /// Mirrors the Python pattern `THEMES = cycle([...])` + `action_cycle_theme`.
    pub fn set_theme_cycle<I, S>(&mut self, names: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.theme_cycle = names.into_iter().map(Into::into).collect();
        self.theme_cycle_index = 0;
    }

    /// Advance to the next theme in the configured cycle (Python
    /// `action_cycle_theme` over `cycle([...])`). Returns `true` if the theme
    /// changed.
    pub fn cycle_theme(&mut self) -> bool {
        if self.theme_cycle.is_empty() {
            return false;
        }
        let name = self.theme_cycle[self.theme_cycle_index % self.theme_cycle.len()].clone();
        self.theme_cycle_index = (self.theme_cycle_index + 1) % self.theme_cycle.len();
        self.set_theme_by_name(&name)
    }

    /// Action handler bound to e.g. `ctrl+t` for theme cycling.
    pub fn action_cycle_theme(&mut self) -> bool {
        self.cycle_theme()
    }

    pub fn binding_hints(&self) -> Vec<BindingHint> {
        let mut out = Vec::new();
        for quit in &self.quit_keys {
            out.push(
                BindingHint::new(quit.display_key(), "Quit application")
                    .hidden(true)
                    .with_namespace("app")
                    .with_priority(true)
                    .with_system(true),
            );
        }
        for (bind, action) in self.action_map.entries() {
            out.push(
                BindingHint::new(bind.display_key(), action.description())
                    .hidden(true)
                    .with_namespace("screen")
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

        self.normalize_binding_hints(out)
    }

    pub(super) fn normalize_binding_hints(&self, out: Vec<BindingHint>) -> Vec<BindingHint> {
        let mut unique = BTreeSet::new();
        let mut deduped = Vec::new();
        for entry in out {
            let key = (
                entry.key.clone(),
                entry.description.clone(),
                entry.tooltip.clone(),
                entry.namespace.clone(),
                entry.show,
                entry.key_display.clone(),
                entry.group.clone(),
                entry.priority,
                entry.system,
                entry.action_name.clone(),
                entry.action_parameters.clone(),
                entry.enabled,
            );
            if unique.insert(key) {
                deduped.push(entry);
            }
        }

        deduped
    }

    pub fn set_command_palette_hint(&mut self, enabled: bool) {
        if enabled {
            if self.command_palette_hint.is_none() {
                self.command_palette_hint = Some(BindingHintEntry {
                    key: KeyBind::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
                    hint: BindingHint::new("ctrl+p", "palette")
                        .with_key_display("^p")
                        .with_tooltip("Open command palette")
                        .with_namespace("app")
                        .with_group("command_palette")
                        .with_priority(false),
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
                .with_namespace("app")
                .with_group("command_palette")
                .with_priority(false),
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

    /// Register the `check_action` callback. Called by `TextualAppAdapter` during
    /// initialization to forward `check_action` calls to the app's trait method.
    pub fn set_check_action_fn(
        &mut self,
        f: Arc<dyn Fn(&str, &[String]) -> Option<bool> + Send + Sync>,
    ) {
        self.check_action_fn = Some(f);
    }

    /// Force re-evaluation of `check_action` for all current bindings.
    ///
    /// Clears the cached binding hints so the next `dispatch_binding_hints_changed()`
    /// call detects a change and re-broadcasts to the Footer. Mirrors Python
    /// Textual's `App.refresh_bindings()`.
    pub fn refresh_bindings(&mut self) {
        self.last_binding_hints.clear();
        self.last_binding_hint_sources.clear();
    }

    /// Apply `check_action` results to a set of binding hints.
    ///
    /// For each hint with an `action` field, calls the registered `check_action_fn`
    /// and updates the `enabled` field accordingly.
    pub(super) fn apply_check_action(&self, hints: &mut [BindingHint]) {
        let Some(check_fn) = &self.check_action_fn else {
            return;
        };
        for hint in hints.iter_mut() {
            if let Some(action_name) = &hint.action_name {
                hint.enabled = check_fn(action_name, &hint.action_parameters);
            } else if let Some(action) = &hint.action {
                hint.enabled = check_fn(action, &[]);
            }
        }
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

    fn dispatch_screen_lifecycle_event(&mut self, event: Event) {
        // App-level lifecycle messages target the runtime root tree.
        // ScreenStack handles per-screen suspend/resume through Screen hooks.
        let Some(tree) = self.widget_tree.as_mut() else {
            return;
        };
        if tree.root().is_none() {
            return;
        }
        let focused = routing::focused_node_id_tree(tree);
        let _ = routing::dispatch_event_tree(tree, focused, &event);
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
        self.last_focused_on_app_blur = None;
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
        self.dispatch_screen_lifecycle_event(Event::ScreenSuspend);
        self.screen_stack.push(screen);
        let _ = self.focus_first_in_active_tree();
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
        self.dispatch_screen_lifecycle_event(Event::ScreenSuspend);
        self.screen_stack.push_with_callback(screen, callback);
        let _ = self.focus_first_in_active_tree();
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
                self.dispatch_screen_lifecycle_event(Event::ScreenResume);
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
            self.dispatch_screen_lifecycle_event(Event::ScreenResume);
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

        self.dispatch_screen_lifecycle_event(Event::ScreenSuspend);

        // Remove the current mode screen by its mode tag (safe even if
        // transient screens are on top).
        if let Some(mode) = self.current_mode.take() {
            self.screen_stack.pop_mode(&mode);
        }

        // Push the new mode screen with its mode tag.
        self.screen_stack.push_mode(new_screen, name.to_string());
        let _ = self.focus_first_in_active_tree();
        self.current_mode = Some(name.to_string());
        self.dispatch_screen_lifecycle_event(Event::ScreenResume);
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
        let hovered = self.widget_at_auto(x as u16, y as u16);

        let hovered_changed = hovered != self.hovered;
        if hovered_changed {
            debug_input(&format!(
                "[hover] screen=({}, {}) hovered {:?} -> {:?}",
                x,
                y,
                self.hovered.map(node_id_to_ffi),
                hovered.map(node_id_to_ffi)
            ));
            // Update hover state through the tree.
            if let Some(old_id) = self.hovered {
                if let Some(tree) = self.active_widget_tree_mut() {
                    tree.set_hover_state(old_id, false);
                }
            }
            if let Some(new_id) = hovered {
                if let Some(tree) = self.active_widget_tree_mut() {
                    tree.set_hover_state(new_id, true);
                }
            }
            self.hovered = hovered;
            let shape = self.pointer_shape_for_hover_auto(root, self.hovered);
            let _ = self.set_pointer_shape(shape);
        }

        // Forward updated coordinates so widgets can track intra-widget mouse position.
        let moved_changed = if let Some(id) = self.hovered {
            let (lx, ly) = self.content_local_coords_auto(id, x as u16, y as u16);
            self.call_on_mouse_move_auto(root, id, lx, ly, false)
        } else {
            // No hover target: forward through the real root widget so app
            // wrappers can still observe pointer movement outside widget hits.
            debug_input(&format!(
                "[hover] fallback root-move via real-root screen=({}, {})",
                x, y
            ));
            root.on_mouse_move(x as u16, y as u16)
        };

        hovered_changed || moved_changed
    }

    fn system_tooltip_node_id(&self) -> Option<NodeId> {
        let tree = self.active_widget_tree()?;
        let root = tree.root()?;
        tree.walk_depth_first(root).into_iter().find(|node_id| {
            tree.css_id(*node_id)
                .is_some_and(|id| id == SYSTEM_TOOLTIP_STYLE_ID)
        })
    }

    fn set_runtime_display_for_node(&mut self, node_id: NodeId, visible: bool) -> bool {
        let Some(tree) = self.active_widget_tree_mut() else {
            return false;
        };
        let before = tree.get(node_id).map(|node| node.runtime_display);
        if before == Some(visible) {
            return false;
        }
        tree.set_runtime_display(node_id, visible);
        true
    }

    fn hover_tooltip_owner_candidates(&self) -> Vec<NodeId> {
        let Some(hovered) = self.hovered else {
            return Vec::new();
        };
        let Some(tree) = self.active_widget_tree() else {
            return Vec::new();
        };
        if !tree.contains(hovered) {
            return Vec::new();
        }
        let mut owners = vec![hovered];
        owners.extend(tree.ancestors(hovered));
        owners
    }

    fn tooltip_anchor_for_owner(
        &mut self,
        owner: NodeId,
        screen_x: u16,
        screen_y: u16,
    ) -> Option<(u16, u16)> {
        if let Some((anchor_local_x, anchor_local_y)) = self
            .with_widget_mut(owner, |widget| widget.tooltip_anchor())
            .flatten()
        {
            let (cursor_local_x, cursor_local_y) =
                self.content_local_coords_auto(owner, screen_x, screen_y);
            let origin_x = i32::from(screen_x) - i32::from(cursor_local_x);
            let origin_y = i32::from(screen_y) - i32::from(cursor_local_y);
            let anchor_x =
                (origin_x + i32::from(anchor_local_x)).clamp(0, i32::from(u16::MAX)) as u16;
            let anchor_y =
                (origin_y + i32::from(anchor_local_y)).clamp(0, i32::from(u16::MAX)) as u16;
            return Some((anchor_x, anchor_y));
        }

        let tree = self.active_widget_tree()?;
        let node = tree.get(owner)?;
        let rect = node.layout_rect;
        let width = rect.x1.saturating_sub(rect.x0).max(1);
        let height = rect.y1.saturating_sub(rect.y0).max(1);
        Some((
            rect.x0.saturating_add(width / 2),
            rect.y0.saturating_add(height / 2),
        ))
    }

    fn tooltip_viewport_for_owner(&mut self, owner: NodeId) -> (u16, u16, usize, usize) {
        let screen_width = self.options.size.0.max(1);
        let screen_height = self.options.size.1.max(1);

        let mut owners = vec![owner];
        if let Some(tree) = self.active_widget_tree() {
            if !tree.contains(owner) {
                return (0, 0, screen_width, screen_height);
            }
            owners.extend(tree.ancestors(owner));
        } else {
            return (0, 0, screen_width, screen_height);
        }

        for node_id in owners {
            let has_viewport = self
                .with_widget_mut(node_id, |widget| widget.scroll_viewport_size())
                .flatten()
                .is_some();
            if !has_viewport {
                continue;
            }
            let Some((x0, y0, viewport_width, viewport_height)) = self
                .active_widget_tree()
                .and_then(|tree| tree.get(node_id))
                .map(|node| {
                    let rect = node.content_rect;
                    (
                        rect.x0,
                        rect.y0,
                        rect.x1.saturating_sub(rect.x0) as usize,
                        rect.y1.saturating_sub(rect.y0) as usize,
                    )
                })
            else {
                continue;
            };

            let x = usize::from(x0).min(screen_width.saturating_sub(1));
            let y = usize::from(y0).min(screen_height.saturating_sub(1));
            let max_width = screen_width.saturating_sub(x).max(1);
            let max_height = screen_height.saturating_sub(y).max(1);
            return (
                x.min(u16::MAX as usize) as u16,
                y.min(u16::MAX as usize) as u16,
                viewport_width.max(1).min(max_width),
                viewport_height.max(1).min(max_height),
            );
        }

        (0, 0, screen_width, screen_height)
    }

    pub(super) fn update_hover_tooltip(&mut self, screen_x: u16, screen_y: u16) -> bool {
        let Some(tooltip_id) = self.system_tooltip_node_id() else {
            return false;
        };
        if self.hover_tooltips_suppressed() {
            return self.clear_hover_tooltip();
        }

        let owners = self.hover_tooltip_owner_candidates();
        let mut next: Option<(NodeId, String)> = None;
        for owner in owners {
            let text = self
                .with_widget_mut(owner, |widget| widget.tooltip())
                .flatten()
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty());
            if let Some(text) = text {
                next = Some((owner, text));
                break;
            }
        }

        let mut changed = false;
        match next {
            Some((owner, text)) => {
                let (anchor_x, anchor_y) = self
                    .tooltip_anchor_for_owner(owner, screen_x, screen_y)
                    .unwrap_or((0, 0));
                let (viewport_x, viewport_y, viewport_width, viewport_height) =
                    self.tooltip_viewport_for_owner(owner);
                changed |= self
                    .with_widget_mut_as::<Tooltip, _>(tooltip_id, |tooltip| {
                        tooltip.apply_system_state(
                            owner,
                            text,
                            anchor_x as usize,
                            anchor_y as usize,
                            viewport_x as usize,
                            viewport_y as usize,
                            viewport_width,
                            viewport_height,
                        )
                    })
                    .unwrap_or(false);
                changed |= self.set_runtime_display_for_node(tooltip_id, true);
            }
            None => {
                changed |= self
                    .with_widget_mut_as::<Tooltip, _>(tooltip_id, Tooltip::hide_system)
                    .unwrap_or(false);
                changed |= self.set_runtime_display_for_node(tooltip_id, false);
            }
        }

        changed
    }

    pub(super) fn clear_hover_tooltip(&mut self) -> bool {
        let Some(tooltip_id) = self.system_tooltip_node_id() else {
            return false;
        };
        let mut changed = false;
        changed |= self
            .with_widget_mut_as::<Tooltip, _>(tooltip_id, Tooltip::hide_system)
            .unwrap_or(false);
        changed |= self.set_runtime_display_for_node(tooltip_id, false);
        changed
    }

    fn hover_tooltips_suppressed(&mut self) -> bool {
        match self.tooltip_cooldown_until {
            Some(until) if Instant::now() < until => true,
            Some(_) => {
                self.tooltip_cooldown_until = None;
                false
            }
            None => false,
        }
    }

    pub(super) fn suppress_hover_tooltips_for(&mut self, duration: Duration) -> bool {
        self.tooltip_cooldown_until = Some(Instant::now() + duration);
        self.clear_hover_tooltip()
    }

    pub(super) fn start_command_palette_tooltip_cooldown(&mut self) -> bool {
        self.suppress_hover_tooltips_for(COMMAND_PALETTE_TOOLTIP_COOLDOWN)
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
        let target = cell
            .meta
            .as_ref()
            .and_then(|m| m.meta.as_ref())
            .and_then(|map| map.get("textual:widget_id"))
            .and_then(|value| match value {
                MetaValue::Int(n) if *n >= 0 => Some(node_id_from_ffi(*n as u64)),
                _ => None,
            });

        let Some(target) = target else {
            return None;
        };

        if target == NodeId::default() {
            return None;
        }

        if let Some(tree) = self.active_widget_tree() {
            if !tree.contains(target) {
                return None;
            }
            if tree
                .css_id(target)
                .is_some_and(|id| id == SYSTEM_TOOLTIP_STYLE_ID)
            {
                return None;
            }
        }

        Some(target)
    }

    fn widget_at_auto(&self, x: u16, y: u16) -> Option<NodeId> {
        let frame_target = self.widget_at(x, y);
        if let Some(tree) = self.active_widget_tree() {
            let tree_target = widget_at_tree_layout(tree, x, y);
            let chosen_raw = choose_deeper_target(tree, frame_target, tree_target);
            // Guard tree-only fallback with frame geometry presence to avoid
            // startup false positives from coarse layout-only hits.
            let chosen = match (frame_target, chosen_raw) {
                (None, Some(tree_hit))
                    if !hit_test_contains_point(&self.hit_test, tree_hit, x, y) =>
                {
                    None
                }
                _ => chosen_raw,
            };
            if frame_target != tree_target {
                debug_input(&format!(
                    "[hover] widget_at mismatch x={} y={} frame={} tree={} chosen={}",
                    x,
                    y,
                    debug_target_label(tree, frame_target),
                    debug_target_label(tree, tree_target),
                    debug_target_label(tree, chosen)
                ));
            } else if let Some(target) = chosen {
                debug_input(&format!(
                    "[hover] widget_at source=frame+tree x={} y={} target={}",
                    x,
                    y,
                    node_id_to_ffi(target)
                ));
            } else {
                debug_input(&format!(
                    "[hover] widget_at source=frame+tree x={} y={} target=None",
                    x, y
                ));
            }
            chosen
        } else {
            if let Some(target) = frame_target {
                debug_input(&format!(
                    "[hover] widget_at source=frame x={} y={} target={}",
                    x,
                    y,
                    node_id_to_ffi(target)
                ));
                Some(target)
            } else {
                debug_input(&format!(
                    "[hover] widget_at source=none x={} y={} target=None (tree-missing)",
                    x, y
                ));
                None
            }
        }
    }

    fn content_local_coords_auto(
        &self,
        target: NodeId,
        screen_x: u16,
        screen_y: u16,
    ) -> (u16, u16) {
        if let Some(tree) = self.active_widget_tree() {
            if tree.contains(target) {
                let coords = tree_content_local_coords(tree, target, screen_x, screen_y);
                debug_input(&format!(
                    "[hover] local source=tree target={} screen=({}, {}) local=({}, {})",
                    node_id_to_ffi(target),
                    screen_x,
                    screen_y,
                    coords.0,
                    coords.1
                ));
                return coords;
            }
        }
        let coords = self
            .hit_test
            .content_local_coords(target, screen_x, screen_y);
        debug_input(&format!(
            "[hover] local source=frame target={} screen=({}, {}) local=({}, {})",
            node_id_to_ffi(target),
            screen_x,
            screen_y,
            coords.0,
            coords.1
        ));
        coords
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

fn is_ancestor_or_self(tree: &WidgetTree, ancestor: NodeId, node: NodeId) -> bool {
    let mut cursor = Some(node);
    while let Some(id) = cursor {
        if id == ancestor {
            return true;
        }
        cursor = tree.parent(id);
    }
    false
}

fn choose_deeper_target(
    tree: &WidgetTree,
    frame_target: Option<NodeId>,
    tree_target: Option<NodeId>,
) -> Option<NodeId> {
    match (frame_target, tree_target) {
        (Some(frame), Some(tree_hit)) if frame != tree_hit => {
            if is_ancestor_or_self(tree, frame, tree_hit) {
                Some(tree_hit)
            } else if is_ancestor_or_self(tree, tree_hit, frame) {
                Some(frame)
            } else {
                // If frame/tree disagree and neither is ancestor of the other,
                // trust the frame hit: it reflects actual painted cell metadata.
                Some(frame)
            }
        }
        (Some(frame), Some(_)) => Some(frame),
        (Some(frame), None) => Some(frame),
        (None, Some(tree_hit)) => Some(tree_hit),
        (None, None) => None,
    }
}

fn hit_test_contains_point(hit_test: &HitTestMap, target: NodeId, x: u16, y: u16) -> bool {
    hit_test
        .rect(target)
        .is_some_and(|r| x >= r.x0 && x <= r.x1 && y >= r.y0 && y <= r.y1)
}

fn debug_target_label(tree: &WidgetTree, id: Option<NodeId>) -> String {
    match id {
        Some(node_id) => {
            if let Some(node) = tree.get(node_id) {
                let parent = tree.parent(node_id).map(node_id_to_ffi).unwrap_or(0);
                format!(
                    "Some(id={},type={},parent={},children={})",
                    node_id_to_ffi(node_id),
                    node.widget.style_type(),
                    parent,
                    node.children.len()
                )
            } else {
                format!("Some(id={},missing)", node_id_to_ffi(node_id))
            }
        }
        None => "None".to_string(),
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
// Standalone tree-builder for integration tests
// ---------------------------------------------------------------------------

/// Build an arena-based [`WidgetTree`] from a root widget without requiring
/// a full [`App`] instance.
///
/// Replicates the extraction logic of [`App::build_widget_tree()`] for
/// user-declared children (excluding runtime-injected system widgets):
/// 1. Creates a `TreeStubWidget` root node.
/// 2. Recursively extracts children via `take_composed_children()`.
/// 3. Processes `compose()` declarations.
/// 4. Returns `None` if the root has no children (no tree to build).
pub fn build_widget_tree_from_root(root: &mut dyn Widget) -> Option<WidgetTree> {
    let mut tree = WidgetTree::new();
    let root_node_id = tree.set_root(Box::new(TreeStubWidget::from_widget(root)));

    // Propagate the real root widget's CSS identity (id/classes) to the tree
    // root node so node_selector_meta() and CSS combinator rules (.panel > ...)
    // see the correct ancestor metadata during layout and rendering.
    {
        let root_classes: Vec<String> = root.style_classes().to_vec();
        let root_css_id: Option<String> = root.style_id().map(|s| s.to_string());
        for class in &root_classes {
            tree.add_class(root_node_id, class);
        }
        tree.set_css_id(root_node_id, root_css_id);
    }

    App::extract_children_to_tree(&mut tree, root_node_id, root);

    let declarations = root.compose();
    if !declarations.is_empty() {
        App::mount_declarations(&mut tree, root_node_id, declarations);
    }

    if tree.len() <= 1 {
        return None;
    }

    let _ = tree.drain_lifecycle();

    // Propagate the real root widget's initial focused state to the tree so
    // that CSS `:focus` rules and `node.state.focused` are correct from the
    // very first render pass (mirrors how `is_initially_disabled` works for
    // `:disabled`).
    //
    // If the root is directly focusable, focus it. If the root is not
    // focusable (e.g. TabbedContent delegates focus to a ContentTabs child),
    // focus the first focusable descendant instead — mirroring what the real
    // runtime does when navigating focus to a non-focusable container.
    if root.is_initially_focused() {
        if root.focusable() {
            tree.set_focus_state(root_node_id, true);
        } else {
            let first_focusable = crate::runtime::helpers::collect_focus_chain_tree(&tree)
                .into_iter()
                .next();
            if let Some(focus_id) = first_focusable {
                tree.set_focus_state(focus_id, true);
            }
        }
    }

    Some(tree)
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
    bindings: Vec<BindingDecl>,
    binding_hints: Vec<BindingHint>,
}

impl TreeStubWidget {
    fn from_widget(w: &dyn Widget) -> Self {
        Self {
            type_name: w.style_type(),
            bindings: w.bindings(),
            binding_hints: w.binding_hints(),
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

    fn bindings(&self) -> Vec<BindingDecl> {
        self.bindings.clone()
    }

    fn binding_hints(&self) -> Vec<BindingHint> {
        self.binding_hints.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{BindingHint, EventCtx};
    use crate::widget_tree::QueryError;
    use crate::widgets::{AppRoot, BindingDecl, Button, Label, Node};
    use rich_rs::Segments;
    use rich_rs::{Console, ConsoleOptions};
    use std::sync::Arc;
    use std::time::Duration;

    struct StatusProbe {
        text: String,
    }

    impl StatusProbe {
        fn new() -> Self {
            Self {
                text: String::new(),
            }
        }
    }

    impl Widget for StatusProbe {
        fn style_type(&self) -> &'static str {
            "StatusLine"
        }

        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }
    }

    struct TooltipProbe {
        text: String,
        anchor: Option<(u16, u16)>,
        viewport_size: Option<(usize, usize)>,
    }

    impl TooltipProbe {
        fn new(text: &str) -> Self {
            Self {
                text: text.to_string(),
                anchor: None,
                viewport_size: None,
            }
        }

        fn with_anchor(mut self, x: u16, y: u16) -> Self {
            self.anchor = Some((x, y));
            self
        }

        fn with_viewport_size(mut self, width: usize, height: usize) -> Self {
            self.viewport_size = Some((width, height));
            self
        }
    }

    impl Widget for TooltipProbe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn tooltip(&self) -> Option<String> {
            Some(self.text.clone())
        }

        fn tooltip_anchor(&self) -> Option<(u16, u16)> {
            self.anchor
        }

        fn scroll_viewport_size(&self) -> Option<(usize, usize)> {
            self.viewport_size
        }
    }

    #[derive(Default)]
    struct RootBindingsProbe {
        extracted: bool,
    }

    impl Widget for RootBindingsProbe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn bindings(&self) -> Vec<BindingDecl> {
            vec![BindingDecl::new("l", "show_tab('leto')", "Leto")]
        }

        fn binding_hints(&self) -> Vec<BindingHint> {
            vec![BindingHint::new("x", "extra").with_key_display("x")]
        }

        fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
            if self.extracted {
                Vec::new()
            } else {
                self.extracted = true;
                vec![Box::new(Label::new("child"))]
            }
        }
    }

    #[test]
    fn choose_deeper_target_prefers_tree_descendant_over_frame_ancestor() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let frame_target = tree.mount(root, Box::new(Label::new("row")));
        let tree_target = tree.mount(frame_target, Box::new(Label::new("button")));

        let chosen = choose_deeper_target(&tree, Some(frame_target), Some(tree_target));
        assert_eq!(chosen, Some(tree_target));
    }

    #[test]
    fn choose_deeper_target_keeps_frame_when_tree_hit_is_ancestor() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let tree_target = tree.mount(root, Box::new(Label::new("row")));
        let frame_target = tree.mount(tree_target, Box::new(Label::new("button")));

        let chosen = choose_deeper_target(&tree, Some(frame_target), Some(tree_target));
        assert_eq!(chosen, Some(frame_target));
    }

    #[test]
    fn choose_deeper_target_prefers_frame_when_targets_are_unrelated() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let frame_target = tree.mount(root, Box::new(Label::new("left")));
        let tree_target = tree.mount(root, Box::new(Label::new("right")));

        let chosen = choose_deeper_target(&tree, Some(frame_target), Some(tree_target));
        assert_eq!(chosen, Some(frame_target));
    }

    #[test]
    fn hit_test_contains_point_requires_frame_rect_for_tree_fallback() {
        let mut hit_test = HitTestMap::default();
        let target = node_id_from_ffi(42);

        assert!(!hit_test_contains_point(&hit_test, target, 5, 5));

        hit_test.bounds.insert(
            target,
            crate::runtime::types::Rect {
                x0: 4,
                y0: 4,
                x1: 10,
                y1: 10,
            },
        );

        assert!(hit_test_contains_point(&hit_test, target, 5, 5));
        assert!(!hit_test_contains_point(&hit_test, target, 11, 5));
    }

    #[test]
    fn app_query_and_query_one_delegate_to_tree_selectors() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let primary = tree.mount(root, Box::new(Button::new("primary")));
        let secondary = tree.mount(root, Box::new(Button::new("secondary")));
        tree.add_class(primary, "primary");

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);

        let all_buttons = app.query("Button").expect("selector should parse");
        assert_eq!(all_buttons.len(), 2);
        let selected = app.query_one(".primary").expect("selector match");
        assert_eq!(selected, primary);

        let first_button = app.query_one("Button").expect("first match should exist");
        assert_eq!(
            first_button, primary,
            "query_one should return first match in traversal order"
        );
        let strict = app.query_exactly_one("Button");
        assert_eq!(strict, Err(QueryError::TooManyMatches(2)));
        let first_via_last = app.query("Button").expect("selector should parse").last();
        assert_eq!(first_via_last, Ok(secondary));
    }

    #[test]
    fn app_query_methods_validate_selectors_without_tree() {
        let app = App::new().expect("app should initialize");

        assert!(matches!(app.query(""), Err(QueryError::ParseError(_))));
        assert!(matches!(app.query_one(""), Err(QueryError::ParseError(_))));
        assert!(matches!(
            app.query_exactly_one(""),
            Err(QueryError::ParseError(_))
        ));
        assert!(matches!(
            app.query_one_optional(""),
            Err(QueryError::ParseError(_))
        ));
        assert!(matches!(app.query_one("Button"), Err(QueryError::NoMatch)));
        assert_eq!(app.query_one_optional("Button"), Ok(None));
    }

    #[test]
    fn tree_stub_preserves_root_bindings_and_binding_hints() {
        let mut app = App::new().expect("app should initialize");
        let mut root = RootBindingsProbe::default();
        app.build_widget_tree(&mut root);

        let tree = app.widget_tree.as_ref().expect("tree should be built");
        let root_id = tree.root().expect("root node should exist");
        let root_node = tree.get(root_id).expect("root node should exist");
        let bindings = root_node.widget.bindings();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].key, "l");
        assert_eq!(bindings[0].description, "Leto");

        let hints = root_node.widget.binding_hints();
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].description, "extra");
        assert_eq!(hints[0].key_display.as_deref(), Some("x"));
    }

    #[test]
    fn build_widget_tree_mounts_hidden_system_tooltip() {
        let mut app = App::new().expect("app should initialize");
        let mut root = RootBindingsProbe::default();
        app.build_widget_tree(&mut root);

        let tooltip_id = app
            .get_widget_by_id(SYSTEM_TOOLTIP_STYLE_ID)
            .expect("system tooltip should be mounted");
        let hidden = app
            .active_widget_tree()
            .and_then(|tree| tree.get(tooltip_id))
            .map(|node| !node.runtime_display)
            .unwrap_or(false);
        assert!(hidden, "system tooltip should start hidden");
    }

    #[test]
    fn app_with_query_one_mut_updates_widget_state() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let target = tree.mount(root, Box::new(Button::new("focus me")));

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);

        app.query_mut("Button")
            .expect("query should succeed")
            .set_focus(true);

        if let Some(tree) = app.widget_tree.as_mut() {
            tree.set_hover_state(target, true);
        }

        let (focused, hovered) = app
            .widget_tree
            .as_ref()
            .map(|tree| {
                (
                    tree.node_state(target).focused,
                    tree.node_state(target).hovered,
                )
            })
            .unwrap_or((false, false));
        assert!(focused);
        assert!(hovered);
    }

    #[test]
    fn recompose_app_rebuilds_app_content_subtree() {
        // Simulate the runtime shape: adapter root with the AppRoot mounted as
        // its first child (children[0]). `recompose_app` swaps in a freshly
        // composed AppRoot and remounts the subtree, mirroring Python
        // `reactive(recompose=True)` at the App level.
        let mut tree = WidgetTree::new();
        // Adapter-like root wrapper.
        let adapter_root = tree.set_root(Box::new(AppRoot::new()));
        // App-content AppRoot mounted as first child, carrying one initial Label.
        let initial = AppRoot::new().with_child(Label::new("before"));
        let app_content = App::mount_extracted_recursive(&mut tree, adapter_root, Box::new(initial));

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);

        // Sanity: app_content_node_id resolves to the first child of the root.
        assert_eq!(app.app_content_node_id(), Some(app_content));

        // Before: exactly one Label with text "before".
        let before_labels = app
            .query("Label")
            .map(|q| q.into_ids())
            .unwrap_or_default();
        assert_eq!(before_labels.len(), 1, "expected one Label before recompose");

        // Recompose with a fresh AppRoot carrying two different Labels.
        let fresh = AppRoot::new()
            .with_child(Label::new("after-1"))
            .with_child(Label::new("after-2"));
        assert!(app.recompose_app(fresh), "recompose_app should apply");

        // The app-content node is preserved (same id), but its subtree is rebuilt.
        assert_eq!(
            app.app_content_node_id(),
            Some(app_content),
            "app-content node id is stable across recompose"
        );
        let after_labels = app
            .query("Label")
            .map(|q| q.into_ids())
            .unwrap_or_default();
        assert_eq!(after_labels.len(), 2, "expected two Labels after recompose");
        assert!(
            after_labels.iter().all(|id| !before_labels.contains(id)),
            "old child nodes should be replaced, not reused"
        );
    }

    #[test]
    fn recompose_app_without_tree_is_noop() {
        let mut app = App::new().expect("app should initialize");
        // No widget tree built yet.
        assert!(
            !app.recompose_app(AppRoot::new()),
            "recompose_app should return false with no tree"
        );
    }

    #[test]
    fn dynamic_watcher_fires_with_value_and_app() {
        use std::sync::atomic::{AtomicI64, Ordering};
        let mut app = App::new().expect("app should initialize");
        let target = node_id_from_ffi(7);

        // No watcher yet.
        assert!(!app.has_dynamic_watcher(target, "counter"));

        let seen = Arc::new(AtomicI64::new(-1));
        let seen_cb = Arc::clone(&seen);
        app.watch_reactive(target, "counter", move |_app, value| {
            if let Some(v) = value.downcast_ref::<i64>() {
                seen_cb.store(*v, Ordering::SeqCst);
            }
        });
        assert!(app.has_dynamic_watcher(target, "counter"));
        // A different field on the same node has no watcher.
        assert!(!app.has_dynamic_watcher(target, "other"));

        // Fire it.
        let value: i64 = 42;
        app.notify_dynamic_watchers(target, "counter", &value);
        assert_eq!(seen.load(Ordering::SeqCst), 42, "watcher saw the new value");
    }

    #[test]
    fn dynamic_watcher_scoped_to_target_and_field() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let mut app = App::new().expect("app should initialize");
        let target_a = node_id_from_ffi(1);
        let target_b = node_id_from_ffi(2);

        let fires = Arc::new(AtomicUsize::new(0));
        let fires_cb = Arc::clone(&fires);
        app.watch_reactive(target_a, "counter", move |_app, _value| {
            fires_cb.fetch_add(1, Ordering::SeqCst);
        });

        let v: i64 = 1;
        // Wrong target → no fire.
        app.notify_dynamic_watchers(target_b, "counter", &v);
        assert_eq!(fires.load(Ordering::SeqCst), 0);
        // Wrong field → no fire.
        app.notify_dynamic_watchers(target_a, "other", &v);
        assert_eq!(fires.load(Ordering::SeqCst), 0);
        // Right target + field → fire.
        app.notify_dynamic_watchers(target_a, "counter", &v);
        assert_eq!(fires.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn recompose_app_preserves_child_decl_ids() {
        // `with_compose` children carry CSS-id decl metadata that must survive
        // the recompose remount (so `#id` selectors keep resolving after a
        // recompose, as in set_reactive03-style dynamic lists).
        use crate::compose::ChildDecl;

        let mut tree = WidgetTree::new();
        let adapter_root = tree.set_root(Box::new(AppRoot::new()));
        let initial = AppRoot::new().with_compose(vec![
            ChildDecl::from(Label::new("first")).with_id("row-0"),
        ]);
        let app_content =
            App::mount_extracted_recursive(&mut tree, adapter_root, Box::new(initial));

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);
        assert_eq!(app.app_content_node_id(), Some(app_content));

        // Recompose with two id-bearing rows.
        let fresh = AppRoot::new().with_compose(vec![
            ChildDecl::from(Label::new("a")).with_id("row-0"),
            ChildDecl::from(Label::new("b")).with_id("row-1"),
        ]);
        assert!(app.recompose_app(fresh));

        // Both id selectors must resolve after recompose.
        assert!(
            app.query_one("#row-0").is_ok(),
            "#row-0 should resolve after recompose"
        );
        assert!(
            app.query_one("#row-1").is_ok(),
            "#row-1 should resolve after recompose"
        );
    }

    #[test]
    fn selection_click_streak_counts_double_and_triple_clicks() {
        let mut app = App::new().expect("app should initialize");
        let target = node_id_from_ffi(42);
        assert_eq!(app.register_selection_click(target, 0, 10, 10), 1);
        assert_eq!(app.register_selection_click(target, 0, 10, 10), 2);
        assert_eq!(app.register_selection_click(target, 0, 10, 10), 3);
    }

    #[test]
    fn selection_click_streak_resets_when_target_or_button_changes() {
        let mut app = App::new().expect("app should initialize");
        let a = node_id_from_ffi(1);
        let b = node_id_from_ffi(2);
        assert_eq!(app.register_selection_click(a, 0, 5, 5), 1);
        assert_eq!(app.register_selection_click(a, 0, 5, 5), 2);
        assert_eq!(app.register_selection_click(b, 0, 5, 5), 1);
        assert_eq!(app.register_selection_click(a, 2, 5, 5), 1);
    }

    #[test]
    fn app_with_query_one_mut_as_updates_typed_widget_by_selector() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        tree.mount(root, Box::new(StatusProbe::new()));

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);

        let value = app
            .with_query_one_mut_as::<StatusProbe, _>("StatusLine", |status| {
                status.text = "updated".to_string();
                status.text.clone()
            })
            .expect("typed selector mutation should succeed");
        assert_eq!(value, "updated");
    }

    #[test]
    fn app_with_query_one_mut_as_returns_no_match_for_type_mismatch() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        tree.mount(root, Box::new(StatusProbe::new()));

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);

        let result = app.with_query_one_mut_as::<Button, _>("StatusLine", |_| ());
        assert_eq!(result, Err(QueryError::NoMatch));
    }

    #[test]
    fn update_hover_tooltip_updates_shared_system_widget_and_keeps_anchor_stable() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let probe = tree.mount(
            root,
            Box::new(TooltipProbe::new("Open the command palette")),
        );
        App::mount_system_tooltip(&mut tree, root);
        if let Some(node) = tree.get_mut(probe) {
            node.layout_rect = crate::widget_tree::Rect {
                x0: 0,
                y0: 0,
                x1: 8,
                y1: 1,
            };
        }

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);
        app.hovered = Some(probe);

        assert!(app.update_hover_tooltip(2, 0));
        let tooltip_id = app
            .get_widget_by_id(SYSTEM_TOOLTIP_STYLE_ID)
            .expect("system tooltip should exist");
        let first_offset = app
            .active_widget_tree()
            .and_then(|tree| tree.get(tooltip_id))
            .and_then(|node| node.widget.style())
            .and_then(|style| style.offset)
            .expect("tooltip offset should be set");
        let first_text = app
            .with_widget_mut_as::<Tooltip, _>(tooltip_id, |tooltip| tooltip.text().to_string())
            .expect("tooltip widget should downcast");
        assert_eq!(first_text, "Open the command palette");
        let first_visible = app
            .active_widget_tree()
            .and_then(|tree| tree.get(tooltip_id))
            .map(|node| node.runtime_display)
            .unwrap_or(false);
        assert!(first_visible);
        assert!(!app.update_hover_tooltip(6, 0));
        let second_offset = app
            .active_widget_tree()
            .and_then(|tree| tree.get(tooltip_id))
            .and_then(|node| node.widget.style())
            .and_then(|style| style.offset)
            .expect("tooltip offset should remain set");
        assert_eq!(first_offset, second_offset);
    }

    #[test]
    fn update_hover_tooltip_reanchors_when_owner_changes_even_with_same_text() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let first = tree.mount(
            root,
            Box::new(TooltipProbe::new("Open the command palette")),
        );
        let second = tree.mount(
            root,
            Box::new(TooltipProbe::new("Open the command palette")),
        );
        App::mount_system_tooltip(&mut tree, root);
        if let Some(node) = tree.get_mut(first) {
            node.layout_rect = crate::widget_tree::Rect {
                x0: 0,
                y0: 0,
                x1: 8,
                y1: 1,
            };
        }
        if let Some(node) = tree.get_mut(second) {
            node.layout_rect = crate::widget_tree::Rect {
                x0: 60,
                y0: 0,
                x1: 68,
                y1: 1,
            };
        }

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);
        let tooltip_id = app
            .get_widget_by_id(SYSTEM_TOOLTIP_STYLE_ID)
            .expect("system tooltip should exist");

        app.hovered = Some(first);
        assert!(app.update_hover_tooltip(2, 0));
        let first_offset = app
            .active_widget_tree()
            .and_then(|tree| tree.get(tooltip_id))
            .and_then(|node| node.widget.style())
            .and_then(|style| style.offset)
            .expect("first tooltip offset");

        app.hovered = Some(second);
        assert!(app.update_hover_tooltip(68, 0));
        let second_offset = app
            .active_widget_tree()
            .and_then(|tree| tree.get(tooltip_id))
            .and_then(|node| node.widget.style())
            .and_then(|style| style.offset)
            .expect("second tooltip offset");
        assert_ne!(first_offset, second_offset);
    }

    #[test]
    fn update_hover_tooltip_prefers_widget_tooltip_anchor_over_owner_center() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let probe = tree.mount(
            root,
            Box::new(TooltipProbe::new("Open command palette").with_anchor(0, 0)),
        );
        App::mount_system_tooltip(&mut tree, root);
        if let Some(node) = tree.get_mut(probe) {
            node.layout_rect = crate::widget_tree::Rect {
                x0: 40,
                y0: 5,
                x1: 50,
                y1: 6,
            };
            node.content_rect = node.layout_rect;
        }

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);
        app.hovered = Some(probe);
        app.options.size = (200, 40);
        app.options.max_width = 200;
        app.options.max_height = 40;

        assert!(app.update_hover_tooltip(44, 5));
        let tooltip_id = app
            .get_widget_by_id(SYSTEM_TOOLTIP_STYLE_ID)
            .expect("system tooltip should exist");
        let (offset_x, width) = app
            .active_widget_tree()
            .and_then(|tree| tree.get(tooltip_id))
            .and_then(|node| {
                let style = node.widget.style()?;
                let width = match style.width {
                    Some(crate::style::Scalar::Cells(value)) => Some(value as usize),
                    _ => None,
                }?;
                let offset_x = match style.offset?.x {
                    crate::style::OffsetValue::Cells(value) => Some(value as i32),
                    _ => None,
                }?;
                Some((offset_x, width))
            })
            .expect("tooltip geometry should be set");
        let expected_anchor_screen_x = 40i32;
        let expected_x = expected_anchor_screen_x.saturating_sub((width / 2) as i32);
        assert_eq!(
            offset_x, expected_x,
            "tooltip offset should be computed from widget-provided anchor, not owner center"
        );
    }

    #[test]
    fn update_hover_tooltip_constrains_to_widget_viewport_region() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let probe = tree.mount(
            root,
            Box::new(
                TooltipProbe::new("Open command palette")
                    .with_anchor(18, 0)
                    .with_viewport_size(20, 10),
            ),
        );
        App::mount_system_tooltip(&mut tree, root);
        if let Some(node) = tree.get_mut(probe) {
            node.layout_rect = crate::widget_tree::Rect {
                x0: 0,
                y0: 0,
                x1: 80,
                y1: 1,
            };
            node.content_rect = crate::widget_tree::Rect {
                x0: 0,
                y0: 0,
                x1: 20,
                y1: 1,
            };
        }

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);
        app.hovered = Some(probe);
        app.options.size = (80, 24);
        app.options.max_width = 80;
        app.options.max_height = 24;

        assert!(app.update_hover_tooltip(5, 0));
        let tooltip_id = app
            .get_widget_by_id(SYSTEM_TOOLTIP_STYLE_ID)
            .expect("system tooltip should exist");
        let (offset_x, width) = app
            .active_widget_tree()
            .and_then(|tree| tree.get(tooltip_id))
            .and_then(|node| {
                let style = node.widget.style()?;
                let width = match style.width {
                    Some(crate::style::Scalar::Cells(value)) => Some(value as usize),
                    _ => None,
                }?;
                let offset_x = match style.offset?.x {
                    crate::style::OffsetValue::Cells(value) => Some(value as i32),
                    _ => None,
                }?;
                Some((offset_x, width))
            })
            .expect("tooltip geometry should be set");
        let viewport_width = 20usize;
        let anchor_x = 18usize;
        let expected_x = anchor_x
            .saturating_sub(width / 2)
            .min(viewport_width.saturating_sub(width)) as i32;
        assert_eq!(
            offset_x, expected_x,
            "tooltip x-offset should be constrained to the widget viewport width"
        );
    }

    #[test]
    fn update_hover_tooltip_uses_content_rect_for_scroll_viewport_bounds() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let probe = tree.mount(
            root,
            Box::new(
                TooltipProbe::new("Open command palette")
                    .with_anchor(18, 0)
                    .with_viewport_size(80, 10),
            ),
        );
        App::mount_system_tooltip(&mut tree, root);
        if let Some(node) = tree.get_mut(probe) {
            node.layout_rect = crate::widget_tree::Rect {
                x0: 0,
                y0: 0,
                x1: 80,
                y1: 1,
            };
            node.content_rect = crate::widget_tree::Rect {
                x0: 0,
                y0: 0,
                x1: 20,
                y1: 1,
            };
        }

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);
        app.hovered = Some(probe);
        app.options.size = (80, 24);
        app.options.max_width = 80;
        app.options.max_height = 24;

        assert!(app.update_hover_tooltip(5, 0));
        let tooltip_id = app
            .get_widget_by_id(SYSTEM_TOOLTIP_STYLE_ID)
            .expect("system tooltip should exist");
        let (offset_x, width) = app
            .active_widget_tree()
            .and_then(|tree| tree.get(tooltip_id))
            .and_then(|node| {
                let style = node.widget.style()?;
                let width = match style.width {
                    Some(crate::style::Scalar::Cells(value)) => Some(value as usize),
                    _ => None,
                }?;
                let offset_x = match style.offset?.x {
                    crate::style::OffsetValue::Cells(value) => Some(value as i32),
                    _ => None,
                }?;
                Some((offset_x, width))
            })
            .expect("tooltip geometry should be set");
        let viewport_width = 20usize;
        let anchor_x = 18usize;
        let expected_x = anchor_x
            .saturating_sub(width / 2)
            .min(viewport_width.saturating_sub(width)) as i32;
        assert_eq!(
            offset_x, expected_x,
            "content_rect viewport bounds must take precedence over stale widget-reported viewport size"
        );
    }

    #[test]
    fn update_hover_tooltip_remains_hidden_while_cooldown_active() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let probe = tree.mount(root, Box::new(TooltipProbe::new("Open command palette")));
        App::mount_system_tooltip(&mut tree, root);
        if let Some(node) = tree.get_mut(probe) {
            node.layout_rect = crate::widget_tree::Rect {
                x0: 0,
                y0: 0,
                x1: 8,
                y1: 1,
            };
            node.content_rect = node.layout_rect;
        }

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);
        app.hovered = Some(probe);
        app.options.size = (80, 24);
        app.options.max_width = 80;
        app.options.max_height = 24;
        assert!(app.update_hover_tooltip(1, 0));
        assert!(app.suppress_hover_tooltips_for(Duration::from_secs(1)));
        assert!(
            !app.update_hover_tooltip(1, 0),
            "suppressed tooltips should stay hidden until cooldown expires"
        );

        let tooltip_id = app
            .get_widget_by_id(SYSTEM_TOOLTIP_STYLE_ID)
            .expect("system tooltip should exist");
        let visible = app
            .active_widget_tree()
            .and_then(|tree| tree.get(tooltip_id))
            .map(|node| node.runtime_display)
            .unwrap_or(true);
        assert!(!visible, "tooltip should remain hidden during cooldown");
    }

    #[test]
    fn update_hover_tooltip_inflect_above_anchor_compensates_margin_top() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let probe = tree.mount(
            root,
            Box::new(TooltipProbe::new("Open command palette").with_anchor(10, 0)),
        );
        App::mount_system_tooltip(&mut tree, root);
        if let Some(node) = tree.get_mut(probe) {
            node.layout_rect = crate::widget_tree::Rect {
                x0: 0,
                y0: 23,
                x1: 80,
                y1: 24,
            };
            node.content_rect = node.layout_rect;
        }

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);
        app.hovered = Some(probe);
        app.options.size = (80, 24);
        app.options.max_width = 80;
        app.options.max_height = 24;
        assert!(app.update_hover_tooltip(10, 23));

        let tooltip_id = app
            .get_widget_by_id(SYSTEM_TOOLTIP_STYLE_ID)
            .expect("system tooltip should exist");
        let offset_y = app
            .active_widget_tree()
            .and_then(|tree| tree.get(tooltip_id))
            .and_then(|node| node.widget.style())
            .and_then(|style| style.offset)
            .and_then(|offset| match offset.y {
                crate::style::OffsetValue::Cells(value) => Some(value as i32),
                _ => None,
            })
            .expect("tooltip y-offset should be set");
        assert_eq!(
            offset_y, 19,
            "inflected tooltips near the footer should compensate margin-top and sit above footer row"
        );
    }

    #[test]
    fn app_query_children_and_id_helpers_follow_root_children_scope() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let first = tree.mount(root, Box::new(Button::new("first")));
        let second = tree.mount(root, Box::new(Button::new("second")));
        let nested_parent = tree.mount(root, Box::new(Label::new("parent")));
        let nested = tree.mount(nested_parent, Box::new(Button::new("nested")));
        app_assign_style_id(&mut tree, first, "first");
        app_assign_style_id(&mut tree, second, "second");
        app_assign_style_id(&mut tree, nested, "nested");

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);

        let root_children = app.query_children("Button").expect("selector parses");
        assert_eq!(root_children.len(), 2);
        assert_eq!(app.get_child_by_id("first"), Ok(first));
        assert_eq!(app.get_widget_by_id("nested"), Ok(nested));
    }

    #[test]
    fn app_convenience_mount_mount_all_and_get_child_by_type_work() {
        struct TypeProbe;

        impl Widget for TypeProbe {
            fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
                Segments::new()
            }
        }

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let _existing = tree.mount(root, Box::new(Label::new("existing")));

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);

        let mounted = app.mount(TypeProbe).expect("mount should work");
        app.mount_all(vec![
            Box::new(Button::new("one")),
            Box::new(Button::new("two")),
        ])
        .expect("mount_all should work");

        assert!(app.clear_on_next_render);
        let by_type = app
            .get_child_by_type::<TypeProbe>()
            .expect("child by type should resolve");
        assert_eq!(mounted, by_type);
        let button_children = app.query_children("Button").expect("selector parses");
        assert_eq!(button_children.len(), 2);
    }

    #[test]
    fn app_batch_update_marks_repaint_once() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let _ = tree.mount(root, Box::new(Label::new("root")));
        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);
        app.clear_on_next_render = false;

        app.batch_update(|app| {
            let _ = app.mount(Label::new("first"));
            let _ = app.mount(Label::new("second"));
        });
        assert!(app.clear_on_next_render);
        let labels = app.query_children("Label").expect("selector parses");
        assert_eq!(labels.len(), 3);
    }

    #[test]
    fn app_data_bind_applies_latest_typed_value_to_query_matches() {
        #[derive(Default)]
        struct DataBindProbe {
            value: i32,
        }

        impl Widget for DataBindProbe {
            fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
                Segments::new()
            }
        }

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let probe_id = tree.mount(root, Box::new(DataBindProbe::default()));
        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);

        app.set_data("counter", 10_i32);
        app.data_bind::<i32>("counter", "DataBindProbe", |widget, value| {
            let Some(probe) = (widget as &mut dyn Any).downcast_mut::<DataBindProbe>() else {
                return false;
            };
            let changed = probe.value != *value;
            probe.value = *value;
            changed
        })
        .expect("binding should register");

        let tree = app.widget_tree.as_ref().expect("tree exists");
        let probe = tree.get(probe_id).expect("probe exists").widget.as_ref() as &dyn Any;
        let probe = probe.downcast_ref::<DataBindProbe>().expect("typed probe");
        assert_eq!(probe.value, 10);

        app.pending_query_refresh_nodes.clear();
        app.set_data("counter", 33_i32);
        let tree = app.widget_tree.as_ref().expect("tree exists");
        let probe = tree.get(probe_id).expect("probe exists").widget.as_ref() as &dyn Any;
        let probe = probe.downcast_ref::<DataBindProbe>().expect("typed probe");
        assert_eq!(probe.value, 33);
        assert_eq!(app.get_data::<i32>("counter"), Some(33));
        assert!(app.pending_query_refresh_nodes.contains(&probe_id));
    }

    #[test]
    fn app_query_ancestor_finds_closest_match() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let container = tree.mount(root, Box::new(Label::new("container")));
        let button = tree.mount(container, Box::new(Button::new("child")));
        tree.add_class(container, "panel");

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);

        let ancestor = app
            .query_ancestor(button, ".panel")
            .expect("ancestor exists");
        assert_eq!(ancestor, container);
    }

    #[test]
    fn dom_query_filter_exclude_and_bulk_class_mutation_are_centralized() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let first = tree.mount(root, Box::new(Button::new("first")));
        let second = tree.mount(root, Box::new(Button::new("second")));
        tree.add_class(first, "left");
        tree.add_class(second, "right");

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);

        let left = app
            .query("Button")
            .expect("query")
            .filter(&app, ".left")
            .expect("filter");
        assert_eq!(left.len(), 1);
        assert_eq!(left.first(), Ok(first));
        let not_left = app
            .query("Button")
            .expect("query")
            .exclude(&app, ".left")
            .expect("exclude");
        assert_eq!(not_left.first(), Ok(second));
        if let Some(tree) = app.widget_tree.as_mut() {
            tree.set_focus_state(second, true);
        }
        // After RA-2: focus state lives in the tree node record, not the widget.
        // results_where with has_focus() would always return false for migrated widgets.
        // Instead, check the tree node_state directly.
        {
            let button_query = app.query("Button").expect("query");
            let button_ids: Vec<NodeId> = button_query.ids().to_vec();
            let focused: Vec<NodeId> = button_ids
                .into_iter()
                .filter(|&id| {
                    app.widget_tree
                        .as_ref()
                        .is_some_and(|t| t.node_state(id).focused)
                })
                .collect();
            assert_eq!(focused.len(), 1);
            assert_eq!(focused[0], second);
        }

        app.query_mut("Button")
            .expect("query mut")
            .add_class("all")
            .remove_class("left")
            .toggle_class("toggled")
            .set_classes(&["normalized"]);
        let all_normalized = app.query(".normalized").expect("selector");
        assert_eq!(all_normalized.len(), 2);
    }

    #[test]
    fn dom_query_mut_supports_state_style_and_refresh_helpers() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let first = tree.mount(root, Box::new(Button::new("first")));
        let second = tree.mount(root, Box::new(Button::new("second")));

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);

        app.query_mut("Button")
            .expect("query mut")
            .set_styles(|styles| {
                styles.set_bold(true);
                styles.set_min_width(9);
            })
            .focus()
            .set(Some(false), Some(false), None, None)
            .refresh();

        let tree = app.widget_tree.as_ref().expect("tree should exist");
        assert!(
            tree.styles(first)
                .is_some_and(|styles| styles.style.bold == Some(true))
        );
        assert!(
            tree.styles(second)
                .is_some_and(|styles| styles.style.bold == Some(true))
        );
        assert!(!tree.is_displayed(first));
        assert!(!tree.is_displayed(second));
        assert_eq!(tree.visibility(first), Visibility::Hidden);
        assert_eq!(tree.visibility(second), Visibility::Hidden);
        assert!(tree.node_state(first).focused);
        assert!(!tree.node_state(second).focused);
        assert!(!app.clear_on_next_render);
        assert_eq!(app.pending_query_refresh_nodes.len(), 2);
        assert!(app.pending_query_refresh_nodes.contains(&first));
        assert!(app.pending_query_refresh_nodes.contains(&second));
    }

    #[test]
    fn dom_query_refresh_targets_subtrees_and_falls_back_without_tree() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let parent = tree.mount(root, Box::new(Label::new("parent")));
        let child = tree.mount(parent, Box::new(Button::new("child")));

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);
        app.query_mut("Label").expect("query mut").refresh();
        assert!(!app.clear_on_next_render);
        assert!(app.pending_query_refresh_nodes.contains(&parent));
        assert!(app.pending_query_refresh_nodes.contains(&child));

        let mut no_tree = App::new().expect("app should initialize");
        DomQueryMut::new(&mut no_tree, vec![node_id_from_ffi(1)]).refresh();
        assert!(no_tree.clear_on_next_render);
    }

    #[test]
    fn action_suspend_process_uses_runtime_hook_and_reports_errors() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static SUSPEND_CALLS: AtomicUsize = AtomicUsize::new(0);

        fn suspend_ok() -> io::Result<()> {
            SUSPEND_CALLS.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        fn suspend_unsupported() -> io::Result<()> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "unsupported in test",
            ))
        }

        let mut app = App::new().expect("app should initialize");
        app.set_suspend_process_impl_for_test(suspend_ok);
        assert!(app.action_suspend_process());
        assert_eq!(SUSPEND_CALLS.load(Ordering::Relaxed), 1);
        assert!(app.notifications.is_empty());

        app.set_suspend_process_impl_for_test(suspend_unsupported);
        assert!(app.action_suspend_process());
        let last = app.notifications.last().expect("warning notification");
        assert_eq!(last.title, "Suspend process");
        assert_eq!(last.severity, ToastSeverity::Warning);
    }

    // -- Dynamic mount/remove under a live parent (#stopwatch06) -------------

    /// Leaf widget that stages a message at mount time (#51) and reports it as
    /// a composed child so the canonical mount path is exercised.
    #[derive(Debug, Clone)]
    struct MountPing;
    crate::impl_message!(MountPing);

    struct MountProbe {
        staged: Vec<Box<dyn crate::message::Message>>,
    }
    impl MountProbe {
        fn new() -> Self {
            Self {
                staged: vec![Box::new(MountPing)],
            }
        }
    }
    impl Widget for MountProbe {
        fn style_type(&self) -> &'static str {
            "MountProbe"
        }
        fn render(&self, _c: &Console, _o: &ConsoleOptions) -> Segments {
            Segments::new()
        }
        fn focusable(&self) -> bool {
            true
        }
        fn take_pending_mount_messages(&mut self) -> Vec<Box<dyn crate::message::Message>> {
            std::mem::take(&mut self.staged)
        }
    }

    fn app_with_root() -> (App, NodeId) {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let parent = tree.mount(root, Box::new(crate::widgets::Vertical::new()));
        tree.set_css_id(parent, Some("timers".to_string()));
        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);
        (app, parent)
    }

    #[test]
    fn mount_under_inserts_child_via_arena_path() {
        let (mut app, parent) = app_with_root();
        let id = app
            .mount_under("#timers", Button::new("dynamic"))
            .expect("mount_under should succeed");

        let tree = app.widget_tree.as_ref().expect("tree exists");
        assert!(tree.contains(id));
        assert_eq!(tree.parent(id), Some(parent));
        assert_eq!(tree.children(parent), &[id]);
        // Mount lifecycle event was queued for the event loop to drain.
        assert!(tree.has_pending_lifecycle());
        // Structural mutation requested a full relayout + repaint (without the
        // terminal clear, which would diff against a stale frame).
        assert!(app.pending_force_relayout);
    }

    #[test]
    fn mount_under_runs_composed_children_and_decl_meta() {
        let (mut app, _parent) = app_with_root();
        // A Container declared via with_compose carries a child with decl-meta
        // (#44); mounting it dynamically must extract + tag the child through
        // the same arena path the initial build uses. (Container is used here
        // rather than a delegated wrapper like Vertical because the shared
        // `delegate_widget_to!` macro does not forward `take_child_decl_meta` —
        // a pre-existing gap orthogonal to this dynamic-mount work.)
        let container = crate::widgets::Container::new().with_compose(vec![
            ChildDecl::from(Button::new("inner")).with_id("inner-btn"),
        ]);
        let cid = app
            .mount_under("#timers", container)
            .expect("mount container");

        let tree = app.widget_tree.as_ref().expect("tree exists");
        let kids = tree.children(cid);
        assert_eq!(kids.len(), 1, "composed child should have been extracted");
        // Decl-meta id landed on the dynamically mounted grandchild.
        assert_eq!(tree.css_id(kids[0]), Some("inner-btn"));
    }

    #[test]
    fn mount_under_stages_mount_messages_for_drain() {
        let (mut app, _parent) = app_with_root();
        let id = app
            .mount_under("#timers", MountProbe::new())
            .expect("mount probe");
        // The node went through the canonical mount path; its mount-time message
        // (#51) is now drainable from the node exactly as the event loop does.
        let drained = app
            .active_widget_tree_mut()
            .and_then(|t| t.get_mut(id))
            .map(|n| n.widget.take_pending_mount_messages())
            .unwrap_or_default();
        assert_eq!(drained.len(), 1);
        assert!(drained[0].as_any().is::<MountPing>());
    }

    #[test]
    fn mount_before_and_after_position_siblings() {
        let (mut app, parent) = app_with_root();
        let _a = app.mount_under("#timers", Button::new("a")).unwrap();
        let b = app.mount_under("#timers", Button::new("b")).unwrap();

        // Insert "before-b" before b, and "after-a" after a.
        let before = app.mount_before("#timers > Button", Button::new("first")).unwrap();
        let after_id = app.mount_after("#timers > Button", Button::new("second")).unwrap();

        let tree = app.widget_tree.as_ref().expect("tree exists");
        let kids: Vec<NodeId> = tree.children(parent).to_vec();
        // "before" must sit at index 0 (before the first existing Button).
        assert_eq!(kids[0], before);
        // "after" sits right after that first sibling (index 1).
        assert_eq!(kids[1], after_id);
        assert!(kids.contains(&b));
    }

    #[test]
    fn remove_node_tears_down_subtree_and_relayouts() {
        let (mut app, parent) = app_with_root();
        let container = crate::widgets::Vertical::new()
            .with_compose(vec![ChildDecl::from(Button::new("inner"))]);
        let cid = app.mount_under("#timers", container).unwrap();
        let inner = app.widget_tree.as_ref().unwrap().children(cid)[0];

        app.pending_force_relayout = false;
        app.remove_node(cid).expect("remove should succeed");

        let tree = app.widget_tree.as_ref().expect("tree exists");
        assert!(!tree.contains(cid), "removed node gone");
        assert!(!tree.contains(inner), "subtree torn down");
        assert!(tree.children(parent).is_empty());
        assert!(app.pending_force_relayout);
    }

    #[test]
    fn remove_selector_resolves_and_removes() {
        let (mut app, parent) = app_with_root();
        app.mount_under("#timers", Button::new("only")).unwrap();
        app.remove("#timers > Button").expect("remove by selector");
        let tree = app.widget_tree.as_ref().expect("tree exists");
        assert!(tree.children(parent).is_empty());
    }

    #[test]
    fn mount_under_missing_parent_errors() {
        let (mut app, _parent) = app_with_root();
        let err = app.mount_under("#nope", Button::new("x")).unwrap_err();
        assert_eq!(err, QueryError::NoMatch);
    }

    #[test]
    fn dom_query_mut_supports_remove_and_multi_class_ops() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let first = tree.mount(root, Box::new(Button::new("first")));
        let second = tree.mount(root, Box::new(Button::new("second")));

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);

        app.query_mut("Button")
            .expect("query mut")
            .add_classes(&["alpha", "beta"])
            .remove_classes(&["alpha"])
            .toggle_classes(&["beta", "gamma"]);

        let gamma = app.query(".gamma").expect("selector should parse");
        assert_eq!(gamma.len(), 2);
        let beta = app.query(".beta").expect("selector should parse");
        assert!(beta.is_empty());

        app.query_mut("Button").expect("query mut").remove();
        let tree = app.widget_tree.as_ref().expect("tree exists");
        assert!(!tree.contains(first));
        assert!(!tree.contains(second));
    }

    #[test]
    fn dom_query_mut_set_supports_disabled_and_loading() {
        #[derive(Default)]
        struct StateProbe;

        impl Widget for StateProbe {
            fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
                Segments::new()
            }
        }

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let probe = tree.mount(root, Box::new(StateProbe::default()));
        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);

        app.query_mut("StateProbe")
            .expect("query mut")
            .set(None, None, Some(true), Some(true));

        let tree = app.widget_tree.as_ref().expect("tree exists");
        let state = tree.node_state(probe);
        assert!(state.disabled);
        assert!(state.loading);
    }

    #[test]
    fn dom_query_focus_and_blur_follow_first_match_semantics() {
        struct FocusProbe;

        impl Widget for FocusProbe {
            fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
                Segments::new()
            }

            fn focusable(&self) -> bool {
                true
            }
        }

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let first = tree.mount(root, Box::new(FocusProbe));
        let second = tree.mount(root, Box::new(FocusProbe));
        tree.set_focus_state(second, true);
        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);

        app.query_mut("FocusProbe").expect("query").focus();
        let tree = app.widget_tree.as_ref().expect("tree exists");
        assert!(tree.node_state(first).focused);
        assert!(!tree.node_state(second).focused);
        app.query_mut("FocusProbe").expect("query").blur();
        let tree = app.widget_tree.as_ref().expect("tree exists");
        assert!(!tree.node_state(first).focused);
        assert!(!tree.node_state(second).focused);
    }

    #[test]
    fn app_selector_class_actions_mutate_matching_nodes() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let _first = tree.mount(root, Box::new(Button::new("first")));
        let second = tree.mount(root, Box::new(Button::new("second")));
        tree.add_class(second, "selected");

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);

        let added = app
            .action_add_class("Button", "bulk")
            .expect("selector should parse");
        assert_eq!(added, 2);
        let removed = app
            .action_remove_class(".selected", "selected")
            .expect("selector should parse");
        assert_eq!(removed, 1);
        let toggled = app
            .action_toggle_class("#missing", "ignored")
            .expect("selector should parse");
        assert_eq!(toggled, 0);

        let bulk = app.query(".bulk").expect("selector should parse");
        assert_eq!(bulk.len(), 2);
        let selected = app.query(".selected").expect("selector should parse");
        assert!(selected.is_empty());
    }

    #[test]
    fn app_action_helpers_cover_representative_direct_paths() {
        struct FocusProbe;

        impl Widget for FocusProbe {
            fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
                Segments::new()
            }

            fn focusable(&self) -> bool {
                true
            }
        }

        struct RuntimeModeScreen;
        impl crate::screen::Screen for RuntimeModeScreen {
            fn compose(&self) -> Box<dyn Widget> {
                Box::new(AppRoot::new())
            }
        }

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let first = tree.mount(root, Box::new(FocusProbe));
        let second = tree.mount(root, Box::new(FocusProbe));
        tree.set_css_id(first, Some("first".to_string()));
        tree.set_css_id(second, Some("second".to_string()));
        tree.set_focus_state(first, true);

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);

        assert_eq!(app.action_add_class("FocusProbe", "hot"), Ok(2));
        assert_eq!(app.action_remove_class("#missing", "hot"), Ok(0));
        assert_eq!(app.action_toggle_class("#first", "flip"), Ok(1));
        assert_eq!(app.query(".flip").expect("selector").len(), 1);
        assert_eq!(app.action_focus("second"), Ok(true));
        assert!(app.action_focus_next());
        assert!(app.action_focus_previous());
        app.action_notify("hello", "title", "information");
        assert!(!app.notifications.is_empty());
        assert_eq!(app.action_show_help_panel(), Ok(true));
        assert_eq!(app.action_hide_help_panel(), Ok(true));
        assert!(app.action_change_theme());
        assert!(app.action_toggle_dark());
        let screenshot_filename = format!(
            "textual-rs-test-{}.svg",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time after epoch")
                .as_nanos()
        );
        let screenshot_dir = std::env::temp_dir();
        let screenshot_path = screenshot_dir.join(&screenshot_filename);
        let screenshot_dir_str = screenshot_dir.to_string_lossy().to_string();
        assert!(app.action_screenshot(Some(&screenshot_filename), Some(&screenshot_dir_str)));
        assert!(screenshot_path.exists());
        let _ = std::fs::remove_file(&screenshot_path);

        app.add_mode("one", || Box::new(RuntimeModeScreen));
        app.add_mode("two", || Box::new(RuntimeModeScreen));
        assert!(app.action_push_screen("one"));
        assert!(app.switch_mode("two"));
        assert!(app.action_switch_screen("one"));
        assert!(app.action_back());
        assert!(app.action_pop_screen() || app.action_pop_screen());

        let tree = app.widget_tree.as_ref().expect("tree exists");
        assert!(tree.node_state(second).focused);
    }

    #[test]
    fn action_show_help_panel_mounts_under_app_content_when_command_palette_is_present() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let app_content = tree.mount(root, Box::new(AppRoot::new()));
        tree.mount(
            root,
            Box::new(crate::widgets::CommandPalette::new(
                crate::widgets::Label::new("body"),
            )),
        );

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);

        assert_eq!(app.action_show_help_panel(), Ok(true));

        let help_ids = app
            .query("HelpPanel")
            .expect("selector should resolve")
            .into_ids();
        assert_eq!(
            help_ids.len(),
            1,
            "exactly one help panel should be mounted"
        );

        let tree = app.widget_tree.as_ref().expect("tree exists");
        let parent = tree
            .get(help_ids[0])
            .and_then(|node| node.parent)
            .expect("help panel should have a parent");
        assert_eq!(
            parent, app_content,
            "help panel should mount under app content so command palette remains topmost"
        );
    }

    #[test]
    fn action_show_help_panel_invalidates_binding_and_help_caches() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        tree.mount(root, Box::new(AppRoot::new()));

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);
        app.last_binding_hints = vec![BindingHint::new("x", "stale")];
        app.last_binding_hint_sources = vec![node_id_from_ffi(42)];
        app.last_focused_help_source = Some(node_id_from_ffi(99));
        app.last_focused_help_markup = Some("stale help".to_string());

        assert_eq!(app.action_show_help_panel(), Ok(true));
        assert!(app.last_binding_hints.is_empty());
        assert!(app.last_binding_hint_sources.is_empty());
        assert_eq!(app.last_focused_help_source, None);
        assert_eq!(app.last_focused_help_markup, None);
    }

    #[test]
    fn apply_check_action_uses_parsed_action_name_and_parameters() {
        let mut app = App::new().expect("app should initialize");
        app.set_check_action_fn(Arc::new(|action, parameters| {
            if action == "push_screen" && parameters == ["settings"] {
                Some(false)
            } else {
                Some(true)
            }
        }));

        let mut hints = vec![
            BindingHint::new("s", "Settings").with_action("app.push_screen('settings')"),
            BindingHint::new("q", "Quit").with_action("app.quit"),
        ];
        app.apply_check_action(&mut hints);

        assert_eq!(hints[0].action_name.as_deref(), Some("push_screen"));
        assert_eq!(hints[0].action_parameters, vec!["settings".to_string()]);
        assert_eq!(hints[0].enabled, Some(false));
        assert_eq!(hints[1].enabled, Some(true));
    }

    #[test]
    fn screen_suspend_resume_are_dispatched_for_push_pop_and_switch_mode() {
        use std::sync::{Arc, Mutex};

        struct EventLogRoot {
            log: Arc<Mutex<Vec<&'static str>>>,
        }

        impl Widget for EventLogRoot {
            fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
                Segments::new()
            }

            fn on_event(&mut self, event: &Event, _ctx: &mut EventCtx) {
                let mut log = self.log.lock().expect("log lock");
                match event {
                    Event::ScreenSuspend => log.push("suspend"),
                    Event::ScreenResume => log.push("resume"),
                    _ => {}
                }
            }
        }

        struct RuntimeModeScreen;
        impl crate::screen::Screen for RuntimeModeScreen {
            fn compose(&self) -> Box<dyn Widget> {
                Box::new(AppRoot::new())
            }
        }

        let log = Arc::new(Mutex::new(Vec::new()));
        let mut tree = WidgetTree::new();
        tree.set_root(Box::new(EventLogRoot {
            log: Arc::clone(&log),
        }));

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);
        app.add_mode("mode-a", || Box::new(RuntimeModeScreen));
        app.add_mode("mode-b", || Box::new(RuntimeModeScreen));

        app.push_screen(Box::new(RuntimeModeScreen));
        app.pop_screen();
        assert_eq!(
            log.lock().expect("log lock").as_slice(),
            &["suspend", "resume"]
        );

        log.lock().expect("log lock").clear();
        assert!(app.switch_mode("mode-a"));
        log.lock().expect("log lock").clear();
        assert!(app.switch_mode("mode-b"));
        assert_eq!(
            log.lock().expect("log lock").as_slice(),
            &["suspend", "resume"]
        );
    }

    #[test]
    fn queries_resolve_against_active_screen_tree_when_screen_is_pushed() {
        struct BaseMarker;
        impl Widget for BaseMarker {
            fn style_type(&self) -> &'static str {
                "BaseMarker"
            }

            fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
                Segments::new()
            }
        }

        struct ScreenMarker;
        impl Widget for ScreenMarker {
            fn style_type(&self) -> &'static str {
                "ScreenMarker"
            }

            fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
                Segments::new()
            }
        }

        struct QueryScreen;
        impl crate::screen::Screen for QueryScreen {
            fn compose(&self) -> Box<dyn Widget> {
                Box::new(AppRoot::new().with_child(ScreenMarker))
            }
        }

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        tree.mount(root, Box::new(BaseMarker));

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);
        app.add_mode("overlay", || Box::new(QueryScreen));

        assert!(app.query_one("BaseMarker").is_ok());
        assert!(matches!(
            app.query_one("ScreenMarker"),
            Err(QueryError::NoMatch)
        ));

        assert!(app.action_push_screen("overlay"));
        assert!(app.query_one("ScreenMarker").is_ok());
        assert!(matches!(
            app.query_one("BaseMarker"),
            Err(QueryError::NoMatch)
        ));

        assert!(app.action_pop_screen());
        assert!(app.query_one("BaseMarker").is_ok());
        assert!(matches!(
            app.query_one("ScreenMarker"),
            Err(QueryError::NoMatch)
        ));
    }

    #[test]
    fn push_screen_focuses_first_focusable_widget_in_screen_tree() {
        struct FocusScreen;
        impl crate::screen::Screen for FocusScreen {
            fn compose(&self) -> Box<dyn Widget> {
                Box::new(
                    AppRoot::new()
                        .with_child(Node::new(Button::new("First")).id("first"))
                        .with_child(Node::new(Button::new("Second")).id("second")),
                )
            }
        }

        let mut tree = WidgetTree::new();
        tree.set_root(Box::new(AppRoot::new()));

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);
        app.add_mode("focus", || Box::new(FocusScreen));

        assert!(app.action_push_screen("focus"));

        let buttons = app.query("Button").expect("button query should resolve");
        assert_eq!(buttons.len(), 2);
        let first = buttons.ids()[0];
        let second = buttons.ids()[1];
        assert_eq!(
            app.active_widget_tree()
                .map(|t| t.node_state(first).focused),
            Some(true)
        );
        assert_eq!(
            app.active_widget_tree()
                .map(|t| t.node_state(second).focused),
            Some(false)
        );
    }

    // -----------------------------------------------------------------------
    // App title / sub-title API tests (DG-02)
    // -----------------------------------------------------------------------

    #[test]
    fn set_title_stores_title_and_enqueues_message() {
        let mut app = App::new().expect("app should initialize");
        assert_eq!(app.title(), "");
        app.set_title("Code Browser");
        assert_eq!(app.title(), "Code Browser");
        assert_eq!(app.pending_app_messages.len(), 1);
    }

    #[test]
    fn set_sub_title_stores_sub_title_and_enqueues_message() {
        let mut app = App::new().expect("app should initialize");
        assert_eq!(app.sub_title(), None);
        app.set_sub_title("/path/to/file.rs");
        assert_eq!(app.sub_title(), Some("/path/to/file.rs"));
        assert_eq!(app.pending_app_messages.len(), 1);
    }

    #[test]
    fn clear_sub_title_removes_sub_title_and_enqueues_message() {
        let mut app = App::new().expect("app should initialize");
        app.set_sub_title("some/path");
        app.clear_sub_title();
        assert_eq!(app.sub_title(), None);
        // Two messages queued (one for set, one for clear).
        assert_eq!(app.pending_app_messages.len(), 2);
    }

    #[test]
    fn drain_pending_app_messages_clears_queue() {
        let mut app = App::new().expect("app should initialize");
        app.set_title("Hello");
        app.set_sub_title("World");
        let drained = app.drain_pending_app_messages();
        assert_eq!(drained.len(), 2);
        assert!(app.pending_app_messages.is_empty());
    }

    #[test]
    fn enqueued_message_is_screen_title_changed() {
        use crate::message::ScreenTitleChanged;
        let mut app = App::new().expect("app should initialize");
        app.set_title("My App");
        app.set_sub_title("some/path");
        let msgs = app.drain_pending_app_messages();
        // Second message carries both title and subtitle.
        let m = msgs[1]
            .downcast_ref::<ScreenTitleChanged>()
            .expect("expected ScreenTitleChanged message");
        assert_eq!(m.title.as_deref(), Some("My App"));
        assert_eq!(m.sub_title.as_deref(), Some("some/path"));
    }

    fn app_assign_style_id(tree: &mut WidgetTree, node: NodeId, id: &str) {
        // CSS id is stored in the node record only; no wrapper struct needed.
        tree.set_css_id(node, Some(id.to_string()));
    }
}
