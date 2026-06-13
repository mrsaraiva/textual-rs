use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segments, Text};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::action::ParsedAction;
use crate::compose::{ChildDecl, ComposeResult};
use crate::event::{
    AnimationEase, AnimationLevel, AnimationRequest, AnimationValueEvent, BindingHint, Event,
    EventCtx,
};
use crate::message::{
    Message, TabActivated, TabClicked, TabDisabled, TabEnabled, TabHidden, TabShown, TabsCleared,
};
use crate::style::{Dock, TransitionTiming};

use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

use super::{BindingDecl, Container, Horizontal, NodeSeed, Vertical, Widget};

#[derive(Debug, Clone)]
pub struct Tab {
    id: Option<String>,
    label: String,
    disabled: bool,
    classes: Vec<String>,
    hovered: bool,
    seed: NodeSeed,
}

impl Tab {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: None,
            label: label.into(),
            disabled: false,
            classes: Vec::new(),
            hovered: false,
            seed: NodeSeed::default(),
        }
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        let id = id.into();
        self.seed.css_id = Some(id.clone());
        self.id = Some(id);
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        let class = class.into();
        self.seed.classes.push(class.clone());
        self.classes.push(class);
        self
    }

    pub fn classes(mut self, classes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        for class in classes {
            let class = class.into();
            self.seed.classes.push(class.clone());
            self.classes.push(class);
        }
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn label(&self) -> &str {
        self.label.as_str()
    }

    pub fn tab_id(&self) -> Option<&str> {
        self.id.as_deref()
    }
}

impl Widget for Tab {
    fn focusable(&self) -> bool {
        false
    }

    fn mouse_interactive(&self) -> bool {
        true
    }

    fn on_node_state_changed(
        &mut self,
        _old: crate::widgets::NodeState,
        new: crate::widgets::NodeState,
    ) {
        self.hovered = new.hovered;
        self.disabled = new.disabled;
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    fn content_width(&self) -> Option<usize> {
        // Return only the label's cell width (without padding). The layout
        // layer adds CSS padding on top of content_width() automatically, so
        // including it here would double-count and misalign the rendered tab
        // positions relative to what tab_spans() computes.
        Some(rich_rs::cell_len(self.label.as_str()).max(1))
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.disabled {
            return;
        }
        if let Event::MouseDown(mouse) = event {
            if mouse.target == self.node_id() {
                ctx.post_message(crate::message::TabClicked {
                    id: self.id.clone().unwrap_or_default(),
                    title: self.label.clone(),
                });
                ctx.set_handled();
            }
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Text::plain(self.label.clone()).render(console, options)
    }
}

impl Renderable for Tab {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
struct TabEntry {
    tab_id: String,
    label: String,
    disabled: bool,
    hidden: bool,
}

#[derive(Debug, Clone)]
struct TabsState {
    tabs: Vec<TabEntry>,
    active: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct UnderlineState {
    highlight_start: f32,
    highlight_end: f32,
    show_highlight: bool,
}

#[derive(Clone)]
pub struct Underline {
    state: Arc<Mutex<UnderlineState>>,
    seed: NodeSeed,
}

impl Underline {
    pub fn new(state: Arc<Mutex<UnderlineState>>) -> Self {
        Self {
            state,
            seed: NodeSeed::default(),
        }
    }
}

impl Widget for Underline {
    fn style_type(&self) -> &'static str {
        "Underline"
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1) as usize;
        let state = self.state.lock().expect("underline state lock");
        let (start, end) = if state.show_highlight {
            (state.highlight_start, state.highlight_end)
        } else {
            (0.0, 0.0)
        };
        let bar_style = crate::css::resolve_component_style(self, &["underline--bar"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);
        let mut base_style = rich_rs::Style::new();
        if let Some(bg) = bar_style.bgcolor {
            base_style = base_style.with_color(bg);
        } else if let Some(fg) = bar_style.color {
            base_style = base_style.with_color(fg);
        }
        let mut active_style = rich_rs::Style::new();
        if let Some(fg) = bar_style.color {
            active_style = active_style.with_color(fg);
        }
        let bar = crate::renderables::Bar::new((start, end), active_style, base_style).width(width);
        bar.render(_console, options)
    }
}

impl Renderable for Underline {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

static NEXT_TABS_SCOPE_ID: AtomicU64 = AtomicU64::new(1);

pub struct Tabs {
    state: Arc<Mutex<TabsState>>,
    focused: bool,
    hovered: bool,
    hovered_tab: Option<usize>,
    layout_width: usize,
    last_size: Option<(u16, u16)>,
    underline: Arc<Mutex<UnderlineState>>,
    seed: NodeSeed,
    /// The auto-generated CSS id for this Tabs instance (e.g. `__tabs-3`).
    /// Kept as a dedicated field so it remains accessible after `take_node_seed()` consumes the seed.
    scope_id: String,
    dock: Option<crate::style::Dock>,
    pending_messages: Arc<Mutex<Vec<Box<dyn Message>>>>,
    /// True after the first event dispatch (widget is live in the tree).
    /// Used to gate runtime-only messages from `add_tab`.
    live: bool,
}

impl Tabs {
    const UNDERLINE_START_ATTR: &'static str = "tabs.underline_start";
    const UNDERLINE_END_ATTR: &'static str = "tabs.underline_end";
    const UNDERLINE_ANIMATION_DURATION: Duration = Duration::from_millis(300);
    const UNDERLINE_ANIMATION_DELAY: Duration = Duration::ZERO;

    pub fn new() -> Self {
        let n = NEXT_TABS_SCOPE_ID.fetch_add(1, Ordering::Relaxed);
        let auto_id = format!("__tabs-{n}");
        let mut seed = NodeSeed::default();
        seed.css_id = Some(auto_id.clone());
        Self {
            state: Arc::new(Mutex::new(TabsState {
                tabs: Vec::new(),
                active: None,
            })),
            focused: false,
            hovered: false,
            hovered_tab: None,
            layout_width: 1,
            last_size: None,
            underline: Arc::new(Mutex::new(UnderlineState {
                highlight_start: 0.0,
                highlight_end: 0.0,
                show_highlight: true,
            })),
            seed,
            scope_id: auto_id,
            dock: None,
            pending_messages: Arc::new(Mutex::new(Vec::new())),
            live: false,
        }
    }

    pub fn with_tab(mut self, tab: impl Into<Tab>) -> Self {
        self.add_tab(tab);
        self
    }

    pub fn add_tab(&mut self, tab: impl Into<Tab>) {
        let mut tab = tab.into();
        let tab_id = tab
            .tab_id()
            .map(str::to_string)
            .unwrap_or_else(|| self.next_tab_id());
        tab.id = Some(tab_id.clone());
        let mut state = self.state.lock().expect("tabs state lock");
        let was_empty = state.active.is_none();
        state.tabs.push(TabEntry {
            tab_id,
            label: tab.label.clone(),
            disabled: tab.disabled,
            hidden: false,
        });
        if state.active.is_none() {
            state.active = state.tabs.first().map(|entry| entry.tab_id.clone());
        }
        // Emit TabActivated when the first tab is added to a live (mounted)
        // empty Tabs, matching Python's behavior where add_tab fires
        // TabActivated via refresh_active() when going from empty to non-empty.
        // Skip during initial construction (before any on_event call).
        if was_empty && self.live {
            if let Some(first) = state.tabs.first() {
                let msg = TabActivated {
                    id: first.tab_id.clone(),
                    index: 0,
                    title: first.label.clone(),
                };
                drop(state);
                self.pending_messages
                    .lock()
                    .expect("tabs pending lock")
                    .push(Box::new(msg));
                return;
            }
        }
        drop(state);
    }

    pub fn with_tab_id(mut self, id: impl Into<String>, title: impl Into<String>) -> Self {
        let tab = Tab::new(title).id(id.into());
        self.add_tab(tab);
        self
    }

    pub fn add_tab_id(&mut self, id: impl Into<String>, title: impl Into<String>) {
        let tab = Tab::new(title).id(id.into());
        self.add_tab(tab);
    }

    pub fn set_dock(&mut self, dock: Dock) {
        self.dock = Some(dock);
    }

    pub fn active(&self) -> Option<String> {
        let state = self.state.lock().expect("tabs state lock");
        state.active.clone()
    }

    pub fn is_active(&self, id: &str) -> bool {
        let state = self.state.lock().expect("tabs state lock");
        state.active.as_deref() == Some(id)
    }

    pub fn with_active_id<R>(&self, f: impl FnOnce(Option<&str>) -> R) -> R {
        let state = self.state.lock().expect("tabs state lock");
        f(state.active.as_deref())
    }

    pub fn active_index(&self) -> Option<usize> {
        let state = self.state.lock().expect("tabs state lock");
        let id = state.active.as_ref()?;
        self.index_for_id(&state, id)
    }

    pub fn is_tab_disabled(&self, id: &str) -> bool {
        let state = self.state.lock().expect("tabs state lock");
        self.query_tab_by_id(&state, id)
            .map(|tab| tab.disabled)
            .unwrap_or(false)
    }

    pub fn is_tab_hidden(&self, id: &str) -> bool {
        let state = self.state.lock().expect("tabs state lock");
        self.query_tab_by_id(&state, id)
            .map(|tab| tab.hidden)
            .unwrap_or(false)
    }

    pub fn set_active_id(&mut self, id: &str, ctx: Option<&mut EventCtx>) -> bool {
        let state = self.state.lock().expect("tabs state lock");
        let Some(index) = self.index_for_id(&state, id) else {
            return false;
        };
        drop(state);
        self.activate(index, ctx)
    }

    pub fn set_active(&mut self, id: &str, ctx: &mut ReactiveCtx) {
        if self.active().as_deref() == Some(id) {
            return;
        }
        let old = self.active();
        if let Some(index) = {
            let state = self.state.lock().expect("tabs state lock");
            self.index_for_id(&state, id)
        } {
            let _ = self.activate(index, None);
            ctx.record_change(
                "active",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(self.active()),
            );
        }
    }

    pub fn set_tab_disabled(&mut self, id: &str, disabled: bool, ctx: &mut ReactiveCtx) -> bool {
        let mut state = self.state.lock().expect("tabs state lock");
        let Some(index) = self.index_for_id(&state, id) else {
            return false;
        };
        let updated = self.set_tab_disabled_index(&mut state, index, disabled, ctx);
        drop(state);
        if updated {
            self.sync_underline_to_active();
        }
        updated
    }

    pub fn disable_tab(&mut self, id: &str, ctx: &mut ReactiveCtx) -> bool {
        self.set_tab_disabled(id, true, ctx)
    }

    pub fn enable_tab(&mut self, id: &str, ctx: &mut ReactiveCtx) -> bool {
        self.set_tab_disabled(id, false, ctx)
    }

    pub fn set_tab_hidden(&mut self, id: &str, hidden: bool, ctx: &mut ReactiveCtx) -> bool {
        let mut state = self.state.lock().expect("tabs state lock");
        let Some(index) = self.index_for_id(&state, id) else {
            return false;
        };
        let updated = self.set_tab_hidden_index(&mut state, index, hidden, ctx);
        drop(state);
        if updated {
            self.sync_underline_to_active();
        }
        updated
    }

    pub fn hide_tab(&mut self, id: &str, ctx: &mut ReactiveCtx) -> bool {
        self.set_tab_hidden(id, true, ctx)
    }

    pub fn show_tab(&mut self, id: &str, ctx: &mut ReactiveCtx) -> bool {
        self.set_tab_hidden(id, false, ctx)
    }

    fn run_alias_reactive_update(
        &mut self,
        update: impl FnOnce(&mut Self, &mut ReactiveCtx) -> bool,
    ) -> bool {
        let mut rctx = ReactiveCtx::new(self.node_id());
        let handled = update(self, &mut rctx);
        if rctx.has_changes() {
            let changes = rctx.take_changes();
            self.reactive_dispatch(&changes, &mut rctx);
        }
        handled
    }

    pub fn disable(&mut self, id: &str) -> bool {
        self.run_alias_reactive_update(|this, ctx| this.disable_tab(id, ctx))
    }

    pub fn enable(&mut self, id: &str) -> bool {
        self.run_alias_reactive_update(|this, ctx| this.enable_tab(id, ctx))
    }

    pub fn hide(&mut self, id: &str) -> bool {
        self.run_alias_reactive_update(|this, ctx| this.hide_tab(id, ctx))
    }

    pub fn show(&mut self, id: &str) -> bool {
        self.run_alias_reactive_update(|this, ctx| this.show_tab(id, ctx))
    }

    pub fn remove_tab(&mut self, id: &str) -> bool {
        let mut state = self.state.lock().expect("tabs state lock");
        let Some(index) = self.index_for_id(&state, id) else {
            return false;
        };
        let was_active = state.active.as_deref() == Some(id);
        let replacement = if was_active {
            self.replacement_after_deactivation(&state, index)
        } else {
            None
        };
        state.tabs.remove(index);
        if was_active {
            if let Some(next) = replacement {
                let next = next.min(state.tabs.len().saturating_sub(1));
                state.active = state.tabs.get(next).map(|tab| tab.tab_id.clone());
            } else {
                state.active = None;
            }
        }
        drop(state);
        self.sync_underline_to_active();
        true
    }

    pub fn clear(&mut self) {
        let mut state = self.state.lock().expect("tabs state lock");
        state.tabs.clear();
        state.active = None;
        drop(state);
        self.hovered_tab = None;
        self.set_underline_range(0.0, 0.0);
        self.pending_messages
            .lock()
            .expect("tabs pending lock")
            .push(Box::new(TabsCleared));
    }

    pub fn tab_count(&self) -> usize {
        let state = self.state.lock().expect("tabs state lock");
        state.tabs.len()
    }

    fn next_tab_id(&mut self) -> String {
        let state = self.state.lock().expect("tabs state lock");
        format!("tab-{}", state.tabs.len() + 1)
    }

    fn index_for_id(&self, state: &TabsState, id: &str) -> Option<usize> {
        state.tabs.iter().position(|tab| tab.tab_id == id)
    }

    fn query_tab_by_id<'a>(&self, state: &'a TabsState, id: &str) -> Option<&'a TabEntry> {
        let index = self.index_for_id(state, id)?;
        state.tabs.get(index)
    }

    fn scoped_tab_selector(&self, tab_id: &str) -> String {
        format!("#{} #tabs-list > #{tab_id}", self.scope_id)
    }

    fn request_runtime_focus(&self, ctx: &mut EventCtx) {
        ctx.post_message(crate::message::AppFocus {
            widget_id: self.scope_id.clone(),
        });
    }

    fn set_tab_disabled_index(
        &self,
        state: &mut TabsState,
        index: usize,
        disabled: bool,
        ctx: &mut ReactiveCtx,
    ) -> bool {
        let Some(tab) = state.tabs.get_mut(index) else {
            return false;
        };
        if tab.disabled == disabled {
            return true;
        }
        let tab_id = tab.tab_id.clone();
        let old = tab.disabled;
        tab.disabled = disabled;
        ctx.record_change(
            "tab_disabled",
            ReactiveFlags::reactive(),
            Box::new(old),
            Box::new(disabled),
        );
        if disabled {
            self.pending_messages
                .lock()
                .expect("tabs pending lock")
                .push(Box::new(TabDisabled { id: tab_id.clone() }));
        } else {
            self.pending_messages
                .lock()
                .expect("tabs pending lock")
                .push(Box::new(TabEnabled { id: tab_id.clone() }));
        }
        self.pending_messages
            .lock()
            .expect("tabs pending lock")
            .push(Box::new(crate::message::AppSetDisabled {
                selector: self.scoped_tab_selector(&tab_id),
                disabled,
            }));
        if state.active.is_none() {
            self.ensure_active_exists(state);
        }
        true
    }

    fn set_tab_hidden_index(
        &self,
        state: &mut TabsState,
        index: usize,
        hidden: bool,
        ctx: &mut ReactiveCtx,
    ) -> bool {
        if index >= state.tabs.len() {
            return false;
        }
        let was_hidden = state.tabs[index].hidden;
        if was_hidden == hidden {
            return true;
        }
        let tab_id = state.tabs[index].tab_id.clone();
        let prev_active = state.active.clone();
        let is_active = prev_active.as_deref() == Some(tab_id.as_str());
        let replacement = if hidden && is_active {
            self.replacement_after_deactivation(state, index)
        } else {
            None
        };
        state.tabs[index].hidden = hidden;
        ctx.record_change(
            "tab_hidden",
            ReactiveFlags::reactive_layout(),
            Box::new(was_hidden),
            Box::new(hidden),
        );
        if hidden {
            let mut pending = self.pending_messages.lock().expect("tabs pending lock");
            pending.push(Box::new(TabHidden { id: tab_id.clone() }));
            pending.push(Box::new(crate::message::AppAddClass {
                selector: self.scoped_tab_selector(&tab_id),
                class_name: "-hidden".to_string(),
            }));
        } else {
            let mut pending = self.pending_messages.lock().expect("tabs pending lock");
            pending.push(Box::new(TabShown { id: tab_id.clone() }));
            pending.push(Box::new(crate::message::AppRemoveClass {
                selector: self.scoped_tab_selector(&tab_id),
                class_name: "-hidden".to_string(),
            }));
        }
        if hidden && is_active {
            if let Some(next) = replacement {
                state.active = state.tabs.get(next).map(|tab| tab.tab_id.clone());
            } else {
                state.active = None;
            }
        } else if !hidden && state.active.is_none() {
            state.active = Some(tab_id.clone());
        }
        if prev_active != state.active {
            if let Some(prev) = prev_active {
                self.pending_messages
                    .lock()
                    .expect("tabs pending lock")
                    .push(Box::new(crate::message::AppRemoveClass {
                        selector: self.scoped_tab_selector(&prev),
                        class_name: "-active".to_string(),
                    }));
            }
            if let Some(next) = state.active.clone() {
                self.pending_messages
                    .lock()
                    .expect("tabs pending lock")
                    .push(Box::new(crate::message::AppAddClass {
                        selector: self.scoped_tab_selector(&next),
                        class_name: "-active".to_string(),
                    }));
            }
        }
        true
    }

    fn activate(&mut self, index: usize, mut ctx: Option<&mut EventCtx>) -> bool {
        let mut state = self.state.lock().expect("tabs state lock");
        if state.tabs.is_empty() {
            state.active = None;
            drop(state);
            self.set_underline_range(0.0, 0.0);
            return false;
        }
        let next = index.min(state.tabs.len() - 1);
        if !self.is_activatable(&state, next) {
            return false;
        }
        let previous_active_index =
            self.index_for_id(&state, state.active.as_deref().unwrap_or(""));
        if Some(next) != previous_active_index {
            let new_id = state.tabs[next].tab_id.clone();
            let prev_id = previous_active_index.map(|idx| state.tabs[idx].tab_id.clone());
            state.active = Some(new_id.clone());
            drop(state);
            if let Some(ctx) = ctx.as_mut() {
                if let Some(prev) = prev_id {
                    ctx.post_message(crate::message::AppRemoveClass {
                        selector: self.scoped_tab_selector(&prev),
                        class_name: "-active".to_string(),
                    });
                }
                ctx.post_message(crate::message::AppAddClass {
                    selector: self.scoped_tab_selector(&new_id),
                    class_name: "-active".to_string(),
                });
            } else {
                if let Some(prev) = prev_id {
                    self.pending_messages
                        .lock()
                        .expect("tabs pending lock")
                        .push(Box::new(crate::message::AppRemoveClass {
                            selector: self.scoped_tab_selector(&prev),
                            class_name: "-active".to_string(),
                        }));
                }
                self.pending_messages
                    .lock()
                    .expect("tabs pending lock")
                    .push(Box::new(crate::message::AppAddClass {
                        selector: self.scoped_tab_selector(&new_id),
                        class_name: "-active".to_string(),
                    }));
            }
            let target_span = self.underline_span_for_index(next);
            if let Some(ctx) = ctx.as_mut() {
                if let Some((target_start, target_end)) = target_span {
                    let (duration, delay, ease) = self.underline_animation_params();
                    let fallback_source = previous_active_index
                        .and_then(|prev| self.underline_span_for_index(prev))
                        .unwrap_or((target_start, target_end));
                    let (from_start, from_end) = self.current_underline_range();
                    let from_start = if from_end > from_start {
                        from_start
                    } else {
                        fallback_source.0
                    };
                    let from_end = if from_end > from_start {
                        from_end
                    } else {
                        fallback_source.1
                    };
                    ctx.request_animation(
                        AnimationRequest::new(
                            self.node_id(),
                            Self::UNDERLINE_START_ATTR,
                            from_start,
                            target_start,
                            duration,
                        )
                        .with_delay(delay)
                        .with_ease(ease)
                        .with_level(AnimationLevel::Basic),
                    );
                    ctx.request_animation(
                        AnimationRequest::new(
                            self.node_id(),
                            Self::UNDERLINE_END_ATTR,
                            from_end,
                            target_end,
                            duration,
                        )
                        .with_delay(delay)
                        .with_ease(ease)
                        .with_level(AnimationLevel::Basic),
                    );
                } else {
                    self.set_underline_range(0.0, 0.0);
                }
                let title = {
                    let state = self.state.lock().expect("tabs state lock");
                    state.tabs[next].label.clone()
                };
                ctx.post_message(TabActivated {
                    id: new_id,
                    index: next,
                    title,
                });
                ctx.request_repaint();
            } else if let Some((target_start, target_end)) = target_span {
                self.set_underline_range(target_start, target_end);
            } else {
                self.set_underline_range(0.0, 0.0);
            }
            return true;
        }
        false
    }

    pub fn activate_prev(&mut self) {
        self.activate_prev_with_ctx(None);
    }

    fn activate_prev_with_ctx(&mut self, ctx: Option<&mut EventCtx>) {
        self.move_active(-1, ctx);
    }

    pub fn activate_next(&mut self) {
        self.activate_next_with_ctx(None);
    }

    fn activate_next_with_ctx(&mut self, ctx: Option<&mut EventCtx>) {
        self.move_active(1, ctx);
    }

    fn ensure_active_exists(&self, state: &mut TabsState) {
        if let Some(idx) = state
            .active
            .as_ref()
            .and_then(|id| self.index_for_id(state, id))
        {
            if self.is_visible(state, idx) {
                return;
            }
        }
        if let Some(next) = self.first_activatable(state) {
            state.active = Some(state.tabs[next].tab_id.clone());
        } else {
            state.active = None;
        }
    }

    fn is_visible(&self, state: &TabsState, index: usize) -> bool {
        state
            .tabs
            .get(index)
            .map(|tab| !tab.hidden)
            .unwrap_or(false)
    }

    fn is_activatable(&self, state: &TabsState, index: usize) -> bool {
        state
            .tabs
            .get(index)
            .map(|tab| !tab.hidden && !tab.disabled)
            .unwrap_or(false)
    }

    fn potential_active_indices(&self, state: &TabsState) -> Vec<usize> {
        let active_idx = state
            .active
            .as_ref()
            .and_then(|id| self.index_for_id(state, id));
        state
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(index, tab)| {
                if tab.hidden {
                    return None;
                }
                if tab.disabled && Some(index) != active_idx {
                    return None;
                }
                Some(index)
            })
            .collect()
    }

    fn first_activatable(&self, state: &TabsState) -> Option<usize> {
        state
            .tabs
            .iter()
            .enumerate()
            .find(|(_, tab)| !tab.hidden && !tab.disabled)
            .map(|(index, _)| index)
    }

    fn last_activatable(&self, state: &TabsState) -> Option<usize> {
        state
            .tabs
            .iter()
            .enumerate()
            .rev()
            .find(|(_, tab)| !tab.hidden && !tab.disabled)
            .map(|(index, _)| index)
    }

    fn replacement_after_deactivation(&self, state: &TabsState, index: usize) -> Option<usize> {
        let mut candidates = self.potential_active_indices(state);
        let position = candidates
            .iter()
            .position(|candidate| *candidate == index)?;
        candidates.remove(position);
        if candidates.is_empty() {
            None
        } else if position < candidates.len() {
            Some(candidates[position])
        } else {
            candidates.last().copied()
        }
    }

    fn move_active(&mut self, direction: i32, ctx: Option<&mut EventCtx>) {
        let state = self.state.lock().expect("tabs state lock");
        let candidates = self.potential_active_indices(&state);
        if candidates.is_empty() {
            return;
        }
        let active_idx = state
            .active
            .as_ref()
            .and_then(|id| self.index_for_id(&state, id));
        let target = match active_idx {
            Some(active) => match candidates.iter().position(|index| *index == active) {
                Some(position) => {
                    let len = candidates.len() as i32;
                    let next = (position as i32 + direction).rem_euclid(len) as usize;
                    candidates[next]
                }
                None => {
                    if direction >= 0 {
                        candidates[0]
                    } else {
                        *candidates.last().unwrap_or(&candidates[0])
                    }
                }
            },
            None => {
                if direction >= 0 {
                    self.first_activatable(&state).unwrap_or(candidates[0])
                } else {
                    self.last_activatable(&state)
                        .unwrap_or(*candidates.last().unwrap_or(&candidates[0]))
                }
            }
        };
        drop(state);
        let _ = self.activate(target, ctx);
    }

    fn tab_spans(&self, width: usize) -> Vec<(usize, usize, usize)> {
        let state = self.state.lock().expect("tabs state lock");
        let mut spans = Vec::new();
        let mut cursor = 0usize;
        for (index, tab) in state.tabs.iter().enumerate() {
            if tab.hidden {
                continue;
            }
            if cursor >= width {
                break;
            }
            let label = format!(" {} ", tab.label);
            let label_width = rich_rs::cell_len(&label);
            if label_width == 0 {
                continue;
            }
            let start = cursor;
            let end = start.saturating_add(label_width);
            spans.push((start, end, index));
            cursor = cursor.saturating_add(label_width);
        }
        spans
    }

    fn underline_span_for_index(&self, index: usize) -> Option<(f32, f32)> {
        let spans = self.tab_spans(self.layout_width);
        let (start, end, _) = spans
            .iter()
            .find(|(_, _, tab_index)| *tab_index == index)
            .copied()?;
        let state = self.state.lock().expect("tabs state lock");
        let label_width = state
            .tabs
            .get(index)
            .map(|tab| rich_rs::cell_len(tab.label.as_str()))
            .unwrap_or(0)
            .max(1);
        let span_width = end.saturating_sub(start);
        if span_width <= label_width {
            return Some((start as f32, end as f32));
        }
        let total_inset = span_width.saturating_sub(label_width);
        let left_inset = total_inset / 2;
        let right_inset = total_inset.saturating_sub(left_inset);
        Some((
            start.saturating_add(left_inset) as f32,
            end.saturating_sub(right_inset) as f32,
        ))
    }

    fn sync_underline_to_active(&mut self) {
        if let Some(active) = self.active_index()
            && let Some((start, end)) = self.underline_span_for_index(active)
        {
            self.set_underline_range(start, end);
        } else {
            self.set_underline_range(0.0, 0.0);
        }
    }

    fn underline_animation_params(&self) -> (Duration, Duration, AnimationEase) {
        let style = crate::css::resolve_component_style(self, &["tabs--underline", "-active"]);
        let duration = style
            .transition_duration
            .unwrap_or(Self::UNDERLINE_ANIMATION_DURATION);
        let delay = style
            .transition_delay
            .unwrap_or(Self::UNDERLINE_ANIMATION_DELAY);
        let ease = style
            .transition_timing
            .map(Self::transition_timing_to_animation_ease)
            .unwrap_or(AnimationEase::InOutCubic);
        (duration, delay, ease)
    }

    fn transition_timing_to_animation_ease(timing: TransitionTiming) -> AnimationEase {
        match timing {
            TransitionTiming::Linear => AnimationEase::Linear,
            TransitionTiming::InOutCubic => AnimationEase::InOutCubic,
            TransitionTiming::OutCubic => AnimationEase::OutCubic,
            TransitionTiming::Round => AnimationEase::Round,
            TransitionTiming::None => AnimationEase::None,
        }
    }

    fn hit_tab(&self, x: usize, y: usize) -> Option<usize> {
        if y > 0 {
            return None;
        }
        self.tab_spans(self.layout_width)
            .into_iter()
            .find(|(start, end, _)| x >= *start && x < *end)
            .map(|(_, _, index)| index)
    }

    fn current_underline_range(&self) -> (f32, f32) {
        let state = self.underline.lock().expect("underline lock");
        (state.highlight_start, state.highlight_end)
    }

    fn set_underline_range(&mut self, start: f32, end: f32) {
        let mut state = self.underline.lock().expect("underline lock");
        state.highlight_start = start;
        state.highlight_end = end;
        state.show_highlight = !(start == 0.0 && end == 0.0);
    }

    fn tab_decls(&self) -> Vec<ChildDecl> {
        let state = self.state.lock().expect("tabs state lock");
        state
            .tabs
            .iter()
            .map(|entry| {
                let tab = Tab::new(entry.label.clone())
                    .id(entry.tab_id.clone())
                    .disabled(entry.disabled);
                let mut classes: Vec<&str> = Vec::new();
                if entry.hidden {
                    classes.push("-hidden");
                }
                if state.active.as_deref() == Some(entry.tab_id.as_str()) {
                    classes.push("-active");
                }
                let mut decl = ChildDecl::from(tab);
                if !classes.is_empty() {
                    decl = decl.with_classes(&classes);
                }
                decl
            })
            .collect()
    }
}

impl Widget for Tabs {
    fn focusable(&self) -> bool {
        true
    }

    fn on_node_state_changed(
        &mut self,
        _old: crate::widgets::NodeState,
        new: crate::widgets::NodeState,
    ) {
        self.focused = new.focused;
        self.hovered = new.hovered;
        if !new.hovered {
            self.hovered_tab = None;
        }
    }

    fn is_initially_focused(&self) -> bool {
        self.focused
    }

    fn on_mount(&mut self) {
        let mut state = self.state.lock().expect("tabs state lock");
        self.ensure_active_exists(&mut state);
        drop(state);
        self.sync_underline_to_active();
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    fn style(&self) -> Option<crate::style::Style> {
        self.dock.map(|dock| {
            let mut s = crate::style::Style::default();
            s.dock = Some(dock);
            s
        })
    }

    fn on_unmount(&mut self) {
        self.focused = false;
        self.hovered = false;
        self.hovered_tab = None;
        self.last_size = None;
    }

    fn on_tick(&mut self, _tick: u64) {}

    fn on_resize(&mut self, width: u16, height: u16) {
        self.last_size = Some((width, height));
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        self.last_size = Some((width, _height));
        let next_layout_width = usize::from(width).max(1);
        if next_layout_width != self.layout_width {
            self.layout_width = next_layout_width;
            self.sync_underline_to_active();
        } else {
            self.layout_width = next_layout_width;
        }
    }

    fn action_namespace(&self) -> &str {
        "tabs"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("left,h", "previous", "Previous tab"),
            BindingDecl::new("right,l", "next", "Next tab"),
        ]
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut EventCtx) -> bool {
        match action.name.as_str() {
            "previous" => {
                self.activate_prev_with_ctx(Some(ctx));
                ctx.set_handled();
                true
            }
            "next" => {
                self.activate_next_with_ctx(Some(ctx));
                ctx.set_handled();
                true
            }
            _ => false,
        }
    }

    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut EventCtx) {}

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.live = true;
        {
            let mut pending = self.pending_messages.lock().expect("tabs pending lock");
            for msg in pending.drain(..) {
                ctx.post_message_boxed(msg);
            }
        }

        if let Event::AnimationValue(AnimationValueEvent {
            target,
            attribute,
            value,
            ..
        }) = event
        {
            if *target == self.node_id() {
                if attribute == Self::UNDERLINE_START_ATTR {
                    let mut state = self.underline.lock().expect("underline lock");
                    state.highlight_start = *value;
                    ctx.request_repaint();
                    ctx.set_handled();
                    return;
                }
                if attribute == Self::UNDERLINE_END_ATTR {
                    let mut state = self.underline.lock().expect("underline lock");
                    state.highlight_end = *value;
                    ctx.request_repaint();
                    ctx.set_handled();
                    return;
                }
            }
        }
        if self.focused {
            if let Event::Key(key) = event {
                match key.code {
                    KeyCode::Left => {
                        self.activate_prev_with_ctx(Some(ctx));
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Right => {
                        self.activate_next_with_ctx(Some(ctx));
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Char('h') => {
                        self.activate_prev_with_ctx(Some(ctx));
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Char('l') => {
                        self.activate_next_with_ctx(Some(ctx));
                        ctx.set_handled();
                        return;
                    }
                    _ => {}
                }
            }
        }
        if let Event::MouseDown(mouse) = event {
            if mouse.target == self.node_id() {
                if let Some(index) = self.hit_tab(mouse.x as usize, mouse.y as usize) {
                    self.request_runtime_focus(ctx);
                    let clicked_active = self.active_index() == Some(index);
                    if self.activate(index, Some(ctx)) || clicked_active {
                        ctx.set_handled();
                        return;
                    }
                }
            }
        }
    }

    fn on_message(&mut self, message: &crate::message::MessageEvent, ctx: &mut EventCtx) {
        if let Some(clicked) = message.downcast_ref::<TabClicked>() {
            if clicked.id.is_empty() {
                return;
            }
            let state = self.state.lock().expect("tabs state lock");
            let index = self.index_for_id(&state, &clicked.id);
            drop(state);
            if let Some(index) = index {
                self.request_runtime_focus(ctx);
                if self.activate(index, Some(ctx)) {
                    ctx.set_handled();
                }
            }
        }
    }

    fn binding_hints(&self) -> Vec<BindingHint> {
        let state = self.state.lock().expect("tabs state lock");
        if self.potential_active_indices(&state).len() <= 1 {
            return Vec::new();
        }
        vec![
            BindingHint::new("left", "Previous tab")
                .with_key_display("←")
                .hidden(true),
            BindingHint::new("right", "Next tab")
                .with_key_display("→")
                .hidden(true),
        ]
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        let hovered = self.hit_tab(x as usize, y as usize);
        if hovered != self.hovered_tab {
            self.hovered_tab = hovered;
            return true;
        }
        false
    }

    fn compose(&self) -> ComposeResult {
        let underline = Underline::new(self.underline.clone());
        let tabs_list = ChildDecl::from(Horizontal::new())
            .with_id("tabs-list")
            .with_children(self.tab_decls());
        let list_bar = ChildDecl::from(Vertical::new())
            .with_id("tabs-list-bar")
            .with_children(vec![tabs_list, ChildDecl::from(underline)]);
        let scroll = ChildDecl::from(Container::new())
            .with_id("tabs-scroll")
            .with_children(vec![list_bar]);
        vec![scroll]
    }

    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn layout_height(&self) -> Option<usize> {
        Some(2)
    }
}

impl Renderable for Tabs {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

impl ReactiveWidget for Tabs {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], _ctx: &mut ReactiveCtx) {
        for change in changes {
            if change.field_name == "active" {
                // Side effects handled directly in set_active via activate().
            }
        }
    }
}

impl Default for Tabs {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Tabs {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            focused: false,
            hovered: false,
            hovered_tab: None,
            layout_width: 1,
            last_size: None,
            underline: self.underline.clone(),
            seed: self.seed.clone(),
            scope_id: self.scope_id.clone(),
            dock: self.dock,
            pending_messages: self.pending_messages.clone(),
            live: self.live,
        }
    }
}

impl From<&str> for Tab {
    fn from(value: &str) -> Self {
        Tab::new(value)
    }
}

impl From<String> for Tab {
    fn from(value: String) -> Self {
        Tab::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MouseDownEvent;
    use crate::keys::KeyEventData;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn keyboard_activation_posts_message_and_requests_repaint() {
        let mut tabs = Tabs::new().with_tab("One").with_tab("Two");
        tabs.on_node_state_changed(
            crate::widgets::NodeState::default(),
            crate::widgets::NodeState {
                focused: true,
                ..Default::default()
            },
        );
        tabs.on_layout(40, 6);

        let mut ctx = EventCtx::default();
        tabs.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Right,
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );

        assert!(ctx.handled());
        assert!(ctx.repaint_requested());
        let messages = ctx.take_messages();
        assert!(messages.iter().any(|m| m.is::<TabActivated>()));
    }

    #[test]
    fn clicking_active_tab_is_handled_but_emits_no_activation_message() {
        let mut tabs = Tabs::new().with_tab("One").with_tab("Two");
        tabs.on_layout(40, 6);

        let mut ctx = EventCtx::default();
        tabs.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: crate::node_id::NodeId::default(),
                screen_x: 1,
                screen_y: 0,
                x: 1,
                y: 0,
            }),
            &mut ctx,
        );

        assert!(ctx.handled());
        let messages = ctx.take_messages();
        assert!(messages.iter().all(|m| !m.is::<TabActivated>()));
    }

    #[test]
    fn is_active_checks_current_id_without_allocating_callsite_string() {
        let tabs = Tabs::new()
            .with_tab_id("one", "One")
            .with_tab_id("two", "Two");
        assert!(tabs.is_active("one"));
        assert!(!tabs.is_active("two"));
    }

    #[test]
    fn with_active_id_exposes_borrowed_view_of_active_id() {
        let tabs = Tabs::new()
            .with_tab_id("one", "One")
            .with_tab_id("two", "Two");
        tabs.with_active_id(|active| {
            assert_eq!(active, Some("one"));
        });
    }

    #[test]
    fn underline_width_matches_active_tab_label_width() {
        let mut tabs = Tabs::new()
            .with_tab("Leto")
            .with_tab("Jessica")
            .with_tab("Paul");
        tabs.on_layout(80, 2);
        let (start, end) = tabs.current_underline_range();
        assert_eq!((end - start).round() as usize, rich_rs::cell_len("Leto"));

        assert!(tabs.activate(1, None));
        let (start, end) = tabs.current_underline_range();
        assert_eq!((end - start).round() as usize, rich_rs::cell_len("Jessica"));

        assert!(tabs.activate(2, None));
        let (start, end) = tabs.current_underline_range();
        assert_eq!((end - start).round() as usize, rich_rs::cell_len("Paul"));
    }
}
