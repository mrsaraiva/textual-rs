mod devtools;
pub(crate) mod dispatch_ctx;
mod event_loop;
mod helpers;
mod render;
mod routing;
mod tasks;
mod timers;
mod types;

// Public re-exports for integration testing via `textual::runtime::*`.
pub use event_loop::resolve_transition_for_property;
pub use render::{
    apply_text_overflow_to_line, constrain_overlay_position, render_tree_to_frame,
    resolve_axis_constrain, run_layout_pass, text_overflow_mode,
};
pub use routing::{dispatch_event_to_target_tree, dispatch_event_tree, focused_node_id_tree};
pub use types::DispatchOutcome;

use crate::animation::{Animator, animation_level_from_env};
use crate::compose::{ChildDecl, WidgetBuilder};
use crate::css::{StyleSheet, default_widget_stylesheet};
use crate::debug::{DebugLayout, debug_input, debug_render};
use crate::driver::{DriverOptions, KeyboardProtocol, PointerShape, TerminalDriver};
use crate::event::{ActionMap, BindingHint, Event, KeyBind};
use crate::message::MessageEvent;
use crate::node_id::NodeId;
use crate::node_id::node_id_from_ffi;
use crate::node_id::node_id_to_ffi;
use crate::render::FrameBuffer;
use crate::screen::ScreenStack;
use crate::style::{Color, Theme, Visibility};
use crate::widget_tree::{QueryError, WidgetTree};
use crate::widgets::{BindingDecl, HelpPanel, ToastSeverity, Widget, WidgetStyles};
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
use std::time::{Duration, Instant};

use tasks::AsyncTaskRuntime;
use timers::OneShotTimerRuntime;
use types::{
    AppNotification, BindingHintEntry, DEFAULT_NOTIFICATION_TIMEOUT, HitTestMap, StylesheetReload,
    StylesheetWatcher,
};

use helpers::{
    ClickTracker, apply_size, collect_focus_chain_tree, default_action_map,
    tree_content_local_coords, widget_at_tree_layout,
};

type SuspendProcessFn = fn() -> io::Result<()>;

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

    pub fn set_styles(self, mut f: impl FnMut(&mut WidgetStyles)) -> Self {
        self.update(|widget| {
            if let Some(styles) = widget.styles_mut() {
                f(styles);
            }
        })
    }

    pub fn set_focus(self, focused: bool) -> Self {
        self.update(|widget| widget.set_focus(focused))
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
            && let Some(node) = tree.get_mut(focused_id)
        {
            node.widget.set_focus(false);
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
            query.update(|widget| widget.set_disabled_state(disabled))
        } else {
            query
        };

        if let Some(loading) = loading {
            query.update(|widget| widget.set_loading_state(loading))
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
    app_inline: bool,
    app_ansi: bool,
    app_nocolor: bool,
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
    /// Last resolved CSS style per node, used for automatic style-transition
    /// dispatch (P2-36).
    style_snapshot_cache: HashMap<NodeId, crate::style::Style>,
    /// Pending refresh targets requested via `DomQueryMut::refresh()`.
    pending_query_refresh_nodes: Vec<NodeId>,
    /// Runtime hook used by `action_suspend_process()` (injectable in tests).
    suspend_process_impl: SuspendProcessFn,
    /// Pending highlight clear: (node_id, clear_at_instant).
    /// Set by HIGHLIGHT devtools command, cleared after timeout.
    pending_highlight_clear: Option<(NodeId, std::time::Instant)>,
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
                    .with_priority(true),
            }),
            theme: Theme::default(),
            dark_mode: true,
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
            app_inline: false,
            app_ansi: matches!(std::env::var("TEXTUAL_APP_ANSI").ok().as_deref(), Some("1")),
            app_nocolor: matches!(
                std::env::var("TEXTUAL_APP_NOCOLOR").ok().as_deref(),
                Some("1")
            ),
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
            style_snapshot_cache: HashMap::new(),
            pending_query_refresh_nodes: Vec::new(),
            suspend_process_impl: suspend_process_default,
            pending_highlight_clear: None,
            widget_tree: None,
            screen_stack: ScreenStack::new(),
            modes: HashMap::new(),
            current_mode: None,
        };
        Ok(app)
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

    /// Query nodes in the active arena tree using a CSS selector.
    ///
    /// Returns a snapshot query object in tree traversal order.
    pub fn query(&self, selector: &str) -> std::result::Result<DomQuery, QueryError> {
        match self.widget_tree.as_ref() {
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
        match self.widget_tree.as_ref() {
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
        let Some(tree) = self.widget_tree.as_ref() else {
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
        let Some(tree) = self.widget_tree.as_ref() else {
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
        let Some(tree) = self.widget_tree.as_mut() else {
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
        let Some(tree) = self.widget_tree.as_mut() else {
            return Err(QueryError::NoMatch);
        };
        let Some(root) = tree.root() else {
            return Err(QueryError::NoMatch);
        };
        tree.mount_all(root, widgets);
        self.clear_on_next_render = true;
        Ok(())
    }

    /// Mutable query handle for chainable bulk mutations.
    pub fn query_mut(
        &mut self,
        selector: &str,
    ) -> std::result::Result<DomQueryMut<'_>, QueryError> {
        let nodes = self.query(selector)?.into_ids();
        Ok(DomQueryMut::new(self, nodes))
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
        let Some(tree) = self.widget_tree.as_mut() else {
            return false;
        };
        if !tree.contains(node_id) || !tree.is_displayed(node_id) {
            return false;
        }
        let current = routing::focused_node_id_tree(tree);
        if current == Some(node_id) {
            return false;
        }
        if let Some(current) = current
            && let Some(node) = tree.get_mut(current)
        {
            node.widget.set_focus(false);
        }
        if let Some(node) = tree.get_mut(node_id) {
            node.widget.set_focus(true);
            return true;
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
        let Some(tree) = self.widget_tree.as_mut() else {
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
        let Some(tree) = self.widget_tree.as_mut() else {
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

    pub fn action_notify(&mut self, message: &str, title: &str, severity: &str) {
        let severity = match severity.to_ascii_lowercase().as_str() {
            "warning" => ToastSeverity::Warning,
            "error" => ToastSeverity::Error,
            _ => ToastSeverity::Information,
        };
        self.notify(message.to_string(), title.to_string(), severity, None);
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
        if let Some(tree) = self.widget_tree.as_mut() {
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
        let mut mount_parent = match self.widget_tree.as_ref().and_then(|tree| tree.root()) {
            Some(root) => root,
            None => return Ok(false),
        };

        if let Ok(command_palette_ids) = self.query("CommandPalette")
            && let Some(command_palette_id) = command_palette_ids.into_ids().first().copied()
            && let Some(tree) = self.widget_tree.as_ref()
            && let Some(adapter_id) = tree.get(command_palette_id).and_then(|node| node.parent)
            && let Some(app_content_id) = tree.children(adapter_id).first().copied()
        {
            // In TextualApp runtime roots, CommandPalette is hosted as the second child
            // of the adapter (first child = normal app subtree). Mount HelpPanel into
            // that app subtree so CommandPalette remains the top-most overlay.
            mount_parent = app_content_id;
        }

        if let Some(tree) = self.widget_tree.as_mut() {
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

    fn request_query_refresh(&mut self, nodes: &[NodeId]) {
        let queued: Vec<NodeId> = {
            let Some(tree) = self.widget_tree.as_ref() else {
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
        self.widget_tree
            .as_mut()?
            .get_mut(node_id)
            .map(|node| f(node.widget.as_mut()))
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
    pub fn with_query_one_mut<R>(
        &mut self,
        selector: &str,
        f: impl FnOnce(&mut dyn Widget) -> R,
    ) -> std::result::Result<R, QueryError> {
        let node_id = self.query_one(selector)?;
        self.with_widget_mut(node_id, f).ok_or(QueryError::NoMatch)
    }

    /// Query one widget by selector and mutably downcast it to `T`.
    pub fn with_query_one_mut_as<T: Widget + 'static, R>(
        &mut self,
        selector: &str,
        f: impl FnOnce(&mut T) -> R,
    ) -> std::result::Result<R, QueryError> {
        let node_id = self.query_one(selector)?;
        self.with_widget_mut_as(node_id, f)
            .ok_or(QueryError::NoMatch)
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

        if tree.len() <= 1 {
            // Only root stub, no composed children — run in root-only mode.
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
    fn extract_children_to_tree(tree: &mut WidgetTree, parent: NodeId, widget: &mut dyn Widget) {
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
            // Apply CSS id/classes from the declaration before mount.
            if let Some(id_str) = &decl.id {
                widget.set_style_id(Some(id_str.clone()));
            }
            let node_id = tree.mount(parent, widget);
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
                        .with_priority(true),
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
                .with_priority(true),
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

    fn dispatch_screen_lifecycle_event(&mut self, event: Event) {
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
            // Update hover state via centralized widget mutation helper.
            if let Some(old_id) = self.hovered {
                let _ = self.with_widget_mut(old_id, |widget| widget.set_hovered(false));
            }
            if let Some(new_id) = hovered {
                let _ = self.with_widget_mut(new_id, |widget| widget.set_hovered(true));
            }
            self.hovered = hovered;
            let shape = self.pointer_shape_for_hover_auto(root, self.hovered);
            let _ = self.set_pointer_shape(shape);
        }

        // Forward updated coordinates so widgets can track intra-widget mouse position.
        let moved_changed = if let Some(id) = self.hovered {
            let (lx, ly) = self.content_local_coords_auto(id, x as u16, y as u16);
            self.call_on_mouse_move_auto(root, id, lx, ly)
        } else {
            // No hover target:
            // - In tree mode, the arena root is a synthetic stub and should not
            //   receive pointer movement directly.
            // - In root-only mode, the root widget is the only dispatch target.
            //
            // In both cases, forward through the real root widget.
            if self.widget_tree.is_some() {
                debug_input(&format!(
                    "[hover] fallback root-move via real-root screen=({}, {})",
                    x, y
                ));
            }
            root.on_mouse_move(x as u16, y as u16)
        };

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

        if let Some(tree) = &self.widget_tree {
            if !tree.contains(target) {
                return None;
            }
        }

        Some(target)
    }

    fn widget_at_auto(&self, x: u16, y: u16) -> Option<NodeId> {
        let frame_target = self.widget_at(x, y);
        if let Some(tree) = self.widget_tree.as_ref() {
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
        if let Some(tree) = &self.widget_tree {
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
/// Replicates the extraction logic of [`App::build_widget_tree()`]:
/// 1. Creates a `TreeStubWidget` root node.
/// 2. Recursively extracts children via `take_composed_children()`.
/// 3. Processes `compose()` declarations.
/// 4. Returns `None` if the root has no children (no tree to build).
pub fn build_widget_tree_from_root(root: &mut dyn Widget) -> Option<WidgetTree> {
    let mut tree = WidgetTree::new();
    let root_node_id = tree.set_root(Box::new(TreeStubWidget::from_widget(root)));

    App::extract_children_to_tree(&mut tree, root_node_id, root);

    let declarations = root.compose();
    if !declarations.is_empty() {
        App::mount_declarations(&mut tree, root_node_id, declarations);
    }

    if tree.len() <= 1 {
        return None;
    }

    let _ = tree.drain_lifecycle();
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
    use crate::widgets::{AppRoot, BindingDecl, Button, Label};
    use rich_rs::Segments;
    use rich_rs::{Console, ConsoleOptions};

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
    fn app_with_query_one_mut_updates_widget_state() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let target = tree.mount(root, Box::new(Button::new("focus me")));

        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);

        app.query_mut("Button")
            .expect("query should succeed")
            .update(|widget| widget.set_focus(true));

        app.with_query_one_mut("Button", |widget| widget.set_hovered(true))
            .expect("target should exist");

        let (focused, hovered) = app
            .widget_tree
            .as_ref()
            .and_then(|tree| tree.get(target))
            .map(|node| (node.widget.has_focus(), node.widget.is_hovered()))
            .unwrap_or((false, false));
        assert!(focused);
        assert!(hovered);
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
        let _ = app.with_widget_mut(second, |widget| widget.set_focus(true));
        let only_second = app
            .query("Button")
            .expect("query")
            .results_where(&app, |widget| widget.has_focus());
        assert_eq!(only_second.only_one(), Ok(second));

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
        let first_node = tree.get(first).expect("first exists");
        let second_node = tree.get(second).expect("second exists");
        assert!(
            first_node
                .widget
                .styles()
                .is_some_and(|styles| styles.style.bold == Some(true))
        );
        assert!(
            second_node
                .widget
                .styles()
                .is_some_and(|styles| styles.style.bold == Some(true))
        );
        assert!(!tree.is_displayed(first));
        assert!(!tree.is_displayed(second));
        assert_eq!(tree.visibility(first), Visibility::Hidden);
        assert_eq!(tree.visibility(second), Visibility::Hidden);
        assert!(first_node.widget.has_focus());
        assert!(!second_node.widget.has_focus());
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
        struct StateProbe {
            disabled: bool,
            loading: bool,
        }

        impl Widget for StateProbe {
            fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
                Segments::new()
            }

            fn is_disabled(&self) -> bool {
                self.disabled
            }

            fn set_disabled_state(&mut self, disabled: bool) {
                self.disabled = disabled;
            }

            fn is_loading(&self) -> bool {
                self.loading
            }

            fn set_loading_state(&mut self, loading: bool) {
                self.loading = loading;
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
        let node = tree.get(probe).expect("probe exists");
        assert!(node.widget.is_disabled());
        assert!(node.widget.is_loading());
    }

    #[test]
    fn dom_query_focus_and_blur_follow_first_match_semantics() {
        struct FocusProbe {
            id: String,
            focused: bool,
        }

        impl FocusProbe {
            fn new(id: &str) -> Self {
                Self {
                    id: id.to_string(),
                    focused: false,
                }
            }
        }

        impl Widget for FocusProbe {
            fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
                Segments::new()
            }

            fn style_id(&self) -> Option<&str> {
                Some(self.id.as_str())
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
        }

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let first = tree.mount(root, Box::new(FocusProbe::new("first")));
        let second = tree.mount(root, Box::new(FocusProbe::new("second")));
        if let Some(node) = tree.get_mut(second) {
            node.widget.set_focus(true);
        }
        let mut app = App::new().expect("app should initialize");
        app.widget_tree = Some(tree);

        app.query_mut("FocusProbe").expect("query").focus();
        let tree = app.widget_tree.as_ref().expect("tree exists");
        assert!(tree.get(first).expect("first exists").widget.has_focus());
        assert!(!tree.get(second).expect("second exists").widget.has_focus());
        app.query_mut("FocusProbe").expect("query").blur();
        let tree = app.widget_tree.as_ref().expect("tree exists");
        assert!(!tree.get(first).expect("first exists").widget.has_focus());
        assert!(!tree.get(second).expect("second exists").widget.has_focus());
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
        struct FocusProbe {
            id: String,
            focused: bool,
        }

        impl FocusProbe {
            fn new(id: &str) -> Self {
                Self {
                    id: id.to_string(),
                    focused: false,
                }
            }
        }

        impl Widget for FocusProbe {
            fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
                Segments::new()
            }

            fn style_id(&self) -> Option<&str> {
                Some(self.id.as_str())
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
        }

        struct RuntimeModeScreen;
        impl crate::screen::Screen for RuntimeModeScreen {
            fn compose(&self) -> Box<dyn Widget> {
                Box::new(AppRoot::new())
            }
        }

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let first = tree.mount(root, Box::new(FocusProbe::new("first")));
        let second = tree.mount(root, Box::new(FocusProbe::new("second")));
        if let Some(node) = tree.get_mut(first) {
            node.widget.set_focus(true);
        }

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
        assert!(
            tree.get(second)
                .map(|node| node.widget.has_focus())
                .unwrap_or(false)
        );
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

    fn app_assign_style_id(tree: &mut WidgetTree, node: NodeId, id: &str) {
        struct IdWrapper {
            id: String,
            inner: Box<dyn Widget>,
        }

        impl Widget for IdWrapper {
            fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
                self.inner.render(console, options)
            }

            fn style_type(&self) -> &'static str {
                self.inner.style_type()
            }

            fn style_id(&self) -> Option<&str> {
                Some(self.id.as_str())
            }
        }

        if let Some(node_ref) = tree.get_mut(node) {
            let inner = std::mem::replace(&mut node_ref.widget, Box::new(Label::new("tmp")));
            node_ref.widget = Box::new(IdWrapper {
                id: id.to_string(),
                inner,
            });
        }
    }
}
