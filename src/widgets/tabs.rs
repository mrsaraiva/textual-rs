use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments, Text};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::action::ParsedAction;
use crate::compose::{ChildDecl, ComposeResult};
use crate::event::{
    AnimationEase, AnimationLevel, AnimationRequest, AnimationValueEvent, BindingHint, Event,
    EventCtx,
};
use crate::message::{
    Message, TabActivated, TabDisabled, TabEnabled, TabHidden, TabShown, TabsCleared,
};
use crate::style::{Dock, TransitionTiming};

use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

use super::{
    BindingDecl, Container, Horizontal, Vertical, Widget, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
};

#[derive(Debug, Clone)]
pub struct Tab {
    id: Option<String>,
    label: String,
    disabled: bool,
    classes: Vec<String>,
    styles: WidgetStyles,
    hovered: bool,
}

impl Tab {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: None,
            label: label.into(),
            disabled: false,
            classes: Vec::new(),
            styles: WidgetStyles::default(),
            hovered: false,
        }
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }

    pub fn classes(mut self, classes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        for class in classes {
            self.classes.push(class.into());
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

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    fn is_disabled(&self) -> bool {
        self.disabled
    }

    fn set_disabled_state(&mut self, disabled: bool) {
        self.disabled = disabled;
    }

    fn style_id(&self) -> Option<&str> {
        self.id.as_deref().or_else(|| self.styles.style_id.as_deref())
    }

    fn style_classes(&self) -> &[String] {
        if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.disabled {
            return;
        }
        if let Event::MouseDown(mouse) = event {
            if mouse.target == self.node_id() {
                ctx.post_message(Message::TabClicked(crate::message::TabClicked {
                    id: self.id.clone().unwrap_or_default(),
                    title: self.label.clone(),
                }));
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
    styles: WidgetStyles,
}

impl Underline {
    pub fn new(state: Arc<Mutex<UnderlineState>>) -> Self {
        Self {
            state,
            styles: WidgetStyles::default(),
        }
    }

    fn render_underline_line(
        width: usize,
        start: f32,
        end: f32,
        base_style: rich_rs::Style,
        active_style: rich_rs::Style,
    ) -> Vec<Segment> {
        if width == 0 {
            return Vec::new();
        }
        let mut start = start.max(0.0);
        let mut end = end.min(width as f32);
        if (start == 0.0 && end == 0.0) || end < 0.0 || start > end {
            return vec![Segment::styled("━".repeat(width), base_style)];
        }
        start = (start * 2.0).round() / 2.0;
        end = (end * 2.0).round() / 2.0;

        let half_start = (start - start.trunc()).abs() > f32::EPSILON;
        let half_end = (end - end.trunc()).abs() > f32::EPSILON;
        let mut out = Vec::new();

        let initial_len = (start - 0.5) as i32;
        if initial_len > 0 {
            out.push(Segment::styled(
                "━".repeat(initial_len as usize),
                base_style.clone(),
            ));
        }
        if !half_start && start > 0.0 {
            out.push(Segment::styled("╸".to_string(), base_style.clone()));
        }

        let bar_width = (end as i32) - (start as i32);
        if half_start {
            let mut highlight = String::from("╺");
            if bar_width > 1 {
                highlight.push_str(&"━".repeat((bar_width - 1) as usize));
            }
            out.push(Segment::styled(highlight, active_style.clone()));
        } else if bar_width > 0 {
            out.push(Segment::styled(
                "━".repeat(bar_width as usize),
                active_style.clone(),
            ));
        }
        if half_end {
            out.push(Segment::styled("╸".to_string(), active_style.clone()));
        }

        if !half_end && (end - width as f32).abs() > f32::EPSILON {
            out.push(Segment::styled("╺".to_string(), base_style.clone()));
        }
        let tail_len = (width as i32) - (end as i32) - 1;
        if tail_len > 0 {
            out.push(Segment::styled("━".repeat(tail_len as usize), base_style));
        }
        out
    }
}

impl Widget for Underline {
    fn style_type(&self) -> &'static str {
        "Underline"
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
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
        let mut base_style = bar_style;
        base_style.color = None;
        let line = Self::render_underline_line(width, start, end, base_style, bar_style);
        let mut out = Segments::new();
        out.extend(line);
        out
    }
}

impl Renderable for Underline {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct Tabs {
    state: Arc<Mutex<TabsState>>,
    focused: bool,
    hovered: bool,
    hovered_tab: Option<usize>,
    layout_width: usize,
    last_size: Option<(u16, u16)>,
    underline: Arc<Mutex<UnderlineState>>,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
    pending_messages: Arc<Mutex<Vec<Message>>>,
}

impl Tabs {
    const UNDERLINE_START_ATTR: &'static str = "tabs.underline_start";
    const UNDERLINE_END_ATTR: &'static str = "tabs.underline_end";
    const UNDERLINE_ANIMATION_DURATION: Duration = Duration::from_millis(300);
    const UNDERLINE_ANIMATION_DELAY: Duration = Duration::ZERO;

    pub fn new() -> Self {
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
            classes: Vec::new(),
            focused_classes: Vec::new(),
            styles: WidgetStyles::default(),
            pending_messages: Arc::new(Mutex::new(Vec::new())),
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
        state.tabs.push(TabEntry {
            tab_id,
            label: tab.label.clone(),
            disabled: tab.disabled,
            hidden: false,
        });
        if state.active.is_none() {
            state.active = state.tabs.first().map(|entry| entry.tab_id.clone());
        }
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
        self.styles.style.dock = Some(dock);
    }

    pub fn active(&self) -> Option<String> {
        let state = self.state.lock().expect("tabs state lock");
        state.active.clone()
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
            .push(Message::TabsCleared(TabsCleared));
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
                .push(Message::TabDisabled(TabDisabled { id: tab_id.clone() }));
        } else {
            self.pending_messages
                .lock()
                .expect("tabs pending lock")
                .push(Message::TabEnabled(TabEnabled { id: tab_id.clone() }));
        }
        self.pending_messages
            .lock()
            .expect("tabs pending lock")
            .push(Message::AppSetDisabled(crate::message::AppSetDisabled {
                selector: format!("#tabs-list > #{tab_id}"),
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
            pending.push(Message::TabHidden(TabHidden { id: tab_id.clone() }));
            pending.push(Message::AppAddClass(crate::message::AppAddClass {
                selector: format!("#tabs-list > #{tab_id}"),
                class_name: "-hidden".to_string(),
            }));
        } else {
            let mut pending = self.pending_messages.lock().expect("tabs pending lock");
            pending.push(Message::TabShown(TabShown { id: tab_id.clone() }));
            pending.push(Message::AppRemoveClass(crate::message::AppRemoveClass {
                selector: format!("#tabs-list > #{tab_id}"),
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
                    .push(Message::AppRemoveClass(crate::message::AppRemoveClass {
                        selector: format!("#tabs-list > #{prev}"),
                        class_name: "-active".to_string(),
                    }));
            }
            if let Some(next) = state.active.clone() {
                self.pending_messages
                    .lock()
                    .expect("tabs pending lock")
                    .push(Message::AppAddClass(crate::message::AppAddClass {
                        selector: format!("#tabs-list > #{next}"),
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
        let previous_active_index = self.index_for_id(&state, state.active.as_deref().unwrap_or(""));
        if Some(next) != previous_active_index {
            let new_id = state.tabs[next].tab_id.clone();
            let prev_id = previous_active_index.map(|idx| state.tabs[idx].tab_id.clone());
            state.active = Some(new_id.clone());
            drop(state);
            if let Some(ctx) = ctx.as_mut() {
                if let Some(prev) = prev_id {
                    ctx.post_message(Message::AppRemoveClass(crate::message::AppRemoveClass {
                        selector: format!("#tabs-list > #{prev}"),
                        class_name: "-active".to_string(),
                    }));
                }
                ctx.post_message(Message::AppAddClass(crate::message::AppAddClass {
                    selector: format!("#tabs-list > #{new_id}"),
                    class_name: "-active".to_string(),
                }));
            } else {
                if let Some(prev) = prev_id {
                    self.pending_messages
                        .lock()
                        .expect("tabs pending lock")
                        .push(Message::AppRemoveClass(crate::message::AppRemoveClass {
                            selector: format!("#tabs-list > #{prev}"),
                            class_name: "-active".to_string(),
                        }));
                }
                self.pending_messages
                    .lock()
                    .expect("tabs pending lock")
                    .push(Message::AppAddClass(crate::message::AppAddClass {
                        selector: format!("#tabs-list > #{new_id}"),
                        class_name: "-active".to_string(),
                    }));
            }
            let target_span = self.span_for_index(next);
            if let Some(ctx) = ctx.as_mut() {
                if let Some((target_start, target_end)) = target_span {
                    let (duration, delay, ease) = self.underline_animation_params();
                    let fallback_source = previous_active_index
                        .and_then(|prev| self.span_for_index(prev))
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
                ctx.post_message(Message::TabActivated(TabActivated {
                    id: new_id,
                    index: next,
                    title,
                }));
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

    fn span_for_index(&self, index: usize) -> Option<(f32, f32)> {
        self.tab_spans(self.layout_width)
            .into_iter()
            .find(|(_, _, tab_index)| *tab_index == index)
            .map(|(start, end, _)| (start as f32, end as f32))
    }

    fn sync_underline_to_active(&mut self) {
        if let Some(active) = self.active_index()
            && let Some((start, end)) = self.span_for_index(active)
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
            .map(|entry| ChildDecl::from(Tab::new(entry.label.clone()).id(entry.tab_id.clone())))
            .collect()
    }
}

impl Widget for Tabs {
    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
        if !hovered {
            self.hovered_tab = None;
        }
    }

    fn on_mount(&mut self) {
        let mut state = self.state.lock().expect("tabs state lock");
        self.ensure_active_exists(&mut state);
        drop(state);
        self.sync_underline_to_active();
        if let Some(active) = self.active() {
            self.pending_messages
                .lock()
                .expect("tabs pending lock")
                .push(Message::AppAddClass(crate::message::AppAddClass {
                    selector: format!("#tabs-list > #{active}"),
                    class_name: "-active".to_string(),
                }));
        }
        let state = self.state.lock().expect("tabs state lock");
        for tab in &state.tabs {
            if tab.hidden {
                self.pending_messages
                    .lock()
                    .expect("tabs pending lock")
                    .push(Message::AppAddClass(crate::message::AppAddClass {
                        selector: format!("#tabs-list > #{}", tab.tab_id),
                        class_name: "-hidden".to_string(),
                    }));
            }
            if tab.disabled {
                self.pending_messages
                    .lock()
                    .expect("tabs pending lock")
                    .push(Message::AppSetDisabled(crate::message::AppSetDisabled {
                        selector: format!("#tabs-list > #{}", tab.tab_id),
                        disabled: true,
                    }));
            }
        }
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
        {
            let mut pending = self.pending_messages.lock().expect("tabs pending lock");
            for msg in pending.drain(..) {
                ctx.post_message(msg);
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
                    if self.activate(index, Some(ctx)) {
                        ctx.set_handled();
                        return;
                    }
                }
            }
        }
    }

    fn on_message(&mut self, message: &crate::message::MessageEvent, ctx: &mut EventCtx) {
        match &message.message {
            Message::TabClicked(clicked) => {
                if clicked.id.is_empty() {
                    return;
                }
                let state = self.state.lock().expect("tabs state lock");
                let index = self.index_for_id(&state, &clicked.id);
                drop(state);
                if let Some(index) = index {
                    if self.activate(index, Some(ctx)) {
                        ctx.set_handled();
                    }
                }
            }
            _ => {}
        }
    }

    fn binding_hints(&self) -> Vec<BindingHint> {
        let state = self.state.lock().expect("tabs state lock");
        if self.potential_active_indices(&state).len() <= 1 {
            return Vec::new();
        }
        vec![
            BindingHint::new("left/right", "Switch tab")
                .with_key_display("←/→")
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
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        Some(2)
    }

    fn style_classes(&self) -> &[String] {
        if self.focused {
            &self.focused_classes
        } else if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
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
            classes: self.classes.clone(),
            focused_classes: self.focused_classes.clone(),
            styles: self.styles.clone(),
            pending_messages: self.pending_messages.clone(),
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
        tabs.set_focus(true);
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
        assert!(messages.iter().any(|m| matches!(m.message, Message::TabActivated(..))));
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
        assert!(messages.iter().all(|m| !matches!(m.message, Message::TabActivated(..))));
    }
}
