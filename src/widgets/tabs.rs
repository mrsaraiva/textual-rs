use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};
use std::time::Duration;

use crate::event::{
    AnimationEase, AnimationLevel, AnimationRequest, AnimationValueEvent, BindingHint, Event,
    EventCtx,
};
use crate::message::*;
use crate::style::TransitionTiming;

use crate::node_id::NodeId;
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

use crate::action::ParsedAction;

use super::{
    BindingDecl, Widget, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
};

pub struct Tabs {
    tabs: Vec<Tab>,
    /// The string ID of the currently active tab, or `None` if no tab is active.
    active: Option<String>,
    focused: bool,
    hovered: bool,
    hovered_tab: Option<usize>,
    layout_width: usize,
    tab_row_height: usize,
    last_size: Option<(u16, u16)>,
    underline_start: f32,
    underline_end: f32,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
    /// Messages queued by methods that lack `EventCtx` (e.g. `disable_tab`).
    /// Drained into `ctx` at the start of the next `on_event` call.
    pending_messages: Vec<Message>,
}

pub struct Tab {
    pub tab_id: String,
    title: String,
    child: Box<dyn Widget>,
    disabled: bool,
    hidden: bool,
}

impl Tabs {
    const UNDERLINE_START_ATTR: &'static str = "tabs.underline_start";
    const UNDERLINE_END_ATTR: &'static str = "tabs.underline_end";
    const UNDERLINE_ANIMATION_DURATION: Duration = Duration::from_millis(300);
    const UNDERLINE_ANIMATION_DELAY: Duration = Duration::ZERO;

    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: None,
            focused: false,
            hovered: false,
            hovered_tab: None,
            layout_width: 1,
            tab_row_height: 2,
            last_size: None,
            underline_start: 0.0,
            underline_end: 0.0,
            classes: vec!["tabs".to_string()],
            focused_classes: vec!["tabs".to_string(), "focused".to_string()],
            styles: WidgetStyles::default(),
            pending_messages: Vec::new(),
        }
    }

    /// Add a tab using the title as the tab ID.
    pub fn with_tab(mut self, title: impl Into<String>, child: impl Widget + 'static) -> Self {
        let title = title.into();
        let tab_id = title.clone();
        self.tabs.push(Tab {
            tab_id: tab_id.clone(),
            title,
            child: Box::new(child),
            disabled: false,
            hidden: false,
        });
        if self.active.is_none() {
            self.active = Some(tab_id);
        }
        self
    }

    /// Add a tab with an explicit string ID.
    pub fn with_tab_id(
        mut self,
        id: impl Into<String>,
        title: impl Into<String>,
        child: impl Widget + 'static,
    ) -> Self {
        let tab_id = id.into();
        self.tabs.push(Tab {
            tab_id: tab_id.clone(),
            title: title.into(),
            child: Box::new(child),
            disabled: false,
            hidden: false,
        });
        if self.active.is_none() {
            self.active = Some(tab_id);
        }
        self
    }

    /// Add a tab at runtime using the title as the tab ID.
    pub fn add_tab(&mut self, title: impl Into<String>, child: impl Widget + 'static) {
        let title = title.into();
        let tab_id = title.clone();
        self.tabs.push(Tab {
            tab_id: tab_id.clone(),
            title,
            child: Box::new(child),
            disabled: false,
            hidden: false,
        });
        if self.active.is_none() {
            self.active = Some(tab_id);
        }
    }

    /// Add a tab at runtime with an explicit string ID.
    pub fn add_tab_id(
        &mut self,
        id: impl Into<String>,
        title: impl Into<String>,
        child: impl Widget + 'static,
    ) {
        let tab_id = id.into();
        self.tabs.push(Tab {
            tab_id: tab_id.clone(),
            title: title.into(),
            child: Box::new(child),
            disabled: false,
            hidden: false,
        });
        if self.active.is_none() {
            self.active = Some(tab_id);
        }
    }

    // ── Reactive getters ──────────────────────────────────────────────

    /// The string ID of the currently active tab, or `None` if no tab is active.
    pub fn active(&self) -> Option<&str> {
        self.active.as_deref()
    }

    /// The index of the currently active tab, or `None`.
    pub fn active_index(&self) -> Option<usize> {
        let id = self.active.as_ref()?;
        self.index_for_id(id)
    }

    pub fn is_tab_disabled(&self, id: &str) -> bool {
        self.tabs
            .iter()
            .find(|tab| tab.tab_id == id)
            .map(|tab| tab.disabled)
            .unwrap_or(false)
    }

    pub fn is_tab_hidden(&self, id: &str) -> bool {
        self.tabs
            .iter()
            .find(|tab| tab.tab_id == id)
            .map(|tab| tab.hidden)
            .unwrap_or(false)
    }

    // ── Reactive setters ──────────────────────────────────────────────

    /// Activate a tab by its string ID.
    pub fn set_active(&mut self, id: &str, ctx: &mut ReactiveCtx) {
        if self.active.as_deref() == Some(id) {
            return;
        }
        let old = self.active.clone();
        if let Some(index) = self.index_for_id(id) {
            // activate() sets self.active and handles focus/underline side effects.
            let _ = self.activate(index, None);
            ctx.record_change(
                "active",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(self.active.clone()),
            );
        }
    }

    pub fn set_tab_disabled(&mut self, id: &str, disabled: bool, ctx: &mut ReactiveCtx) -> bool {
        let Some(index) = self.index_for_id(id) else {
            return false;
        };
        self.set_tab_disabled_index(index, disabled, ctx)
    }

    pub fn disable_tab(&mut self, id: &str, ctx: &mut ReactiveCtx) -> bool {
        self.set_tab_disabled(id, true, ctx)
    }

    pub fn enable_tab(&mut self, id: &str, ctx: &mut ReactiveCtx) -> bool {
        self.set_tab_disabled(id, false, ctx)
    }

    pub fn set_tab_hidden(&mut self, id: &str, hidden: bool, ctx: &mut ReactiveCtx) -> bool {
        let Some(index) = self.index_for_id(id) else {
            return false;
        };
        self.set_tab_hidden_index(index, hidden, ctx)
    }

    pub fn hide_tab(&mut self, id: &str, ctx: &mut ReactiveCtx) -> bool {
        self.set_tab_hidden(id, true, ctx)
    }

    pub fn show_tab(&mut self, id: &str, ctx: &mut ReactiveCtx) -> bool {
        self.set_tab_hidden(id, false, ctx)
    }

    // ── Watchers ──────────────────────────────────────────────────────
    // `active` is watched: side effects are handled directly in set_active via activate().

    /// Remove a tab by its string ID. Returns `true` if found and removed.
    pub fn remove_tab(&mut self, id: &str) -> bool {
        let Some(index) = self.index_for_id(id) else {
            return false;
        };
        let is_active = self.active.as_deref() == Some(id);
        let replacement = if is_active {
            self.replacement_after_deactivation(index)
        } else {
            None
        };
        self.tabs[index].child.set_focus(false);
        self.tabs[index].child.on_unmount();
        self.tabs.remove(index);
        // Adjust hovered_tab after removal.
        if let Some(h) = self.hovered_tab {
            if h == index {
                self.hovered_tab = None;
            } else if h > index {
                self.hovered_tab = Some(h - 1);
            }
        }
        if is_active {
            if let Some(next) = replacement {
                // The removal shifted indices — clamp if needed.
                let next = next.min(self.tabs.len().saturating_sub(1));
                let _ = self.activate(next, None);
            } else {
                self.active = None;
                self.ensure_active_exists();
            }
        }
        self.sync_underline_to_active();
        true
    }

    /// Remove all tabs.
    pub fn clear(&mut self) {
        for tab in &mut self.tabs {
            tab.child.set_focus(false);
            tab.child.on_unmount();
        }
        self.tabs.clear();
        self.active = None;
        self.hovered_tab = None;
        self.underline_start = 0.0;
        self.underline_end = 0.0;
        self.pending_messages.push(Message::TabsCleared(TabsCleared));
    }

    /// Number of tabs.
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    // ── Internal helpers ─────────────────────────────────────────────

    fn index_for_id(&self, id: &str) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.tab_id == id)
    }

    fn set_tab_disabled_index(
        &mut self,
        index: usize,
        disabled: bool,
        ctx: &mut ReactiveCtx,
    ) -> bool {
        let Some(tab) = self.tabs.get_mut(index) else {
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
                .push(Message::TabDisabled(TabDisabled { id: tab_id }));
        } else {
            self.pending_messages
                .push(Message::TabEnabled(TabEnabled { id: tab_id }));
        }
        if self.active.is_none() {
            self.ensure_active_exists();
        }
        self.sync_underline_to_active();
        true
    }

    fn set_tab_hidden_index(
        &mut self,
        index: usize,
        hidden: bool,
        ctx: &mut ReactiveCtx,
    ) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        let was_hidden = self.tabs[index].hidden;
        if was_hidden == hidden {
            return true;
        }
        let tab_id = self.tabs[index].tab_id.clone();
        let is_active = self.active_index() == Some(index);
        let replacement = if hidden && is_active {
            self.replacement_after_deactivation(index)
        } else {
            None
        };
        self.tabs[index].hidden = hidden;
        ctx.record_change(
            "tab_hidden",
            ReactiveFlags::reactive_layout(),
            Box::new(was_hidden),
            Box::new(hidden),
        );
        if hidden {
            self.pending_messages
                .push(Message::TabHidden(TabHidden { id: tab_id }));
        } else {
            self.pending_messages
                .push(Message::TabShown(TabShown { id: tab_id }));
        }
        if hidden && is_active {
            if let Some(next) = replacement {
                let _ = self.activate(next, None);
            } else {
                self.clear_active();
            }
        } else if !hidden && self.active.is_none() {
            let _ = self.activate(index, None);
        }
        self.sync_underline_to_active();
        true
    }

    fn activate(&mut self, index: usize, mut ctx: Option<&mut EventCtx>) -> bool {
        if self.tabs.is_empty() {
            self.clear_active();
            return false;
        }
        let next = index.min(self.tabs.len() - 1);
        if !self.is_activatable(next) {
            return false;
        }
        let previous_active_index = self.active_index();
        if Some(next) != previous_active_index {
            if let Some(prev_idx) = previous_active_index {
                if let Some(prev) = self.tabs.get_mut(prev_idx) {
                    prev.child.set_focus(false);
                }
            }
            self.active = Some(self.tabs[next].tab_id.clone());
            if let Some(tab) = self.tabs.get_mut(next) {
                tab.child.set_focus(self.focused);
                if let Some((width, height)) = self.last_size {
                    let content_height = height.saturating_sub(self.tab_row_height as u16);
                    tab.child.on_resize(width, content_height);
                    tab.child.on_layout(width, content_height);
                }
            }
            let target_span = self.span_for_index(next);
            if let Some(ctx) = ctx.as_mut() {
                if let Some((target_start, target_end)) = target_span {
                    let (duration, delay, ease) = self.underline_animation_params();
                    let fallback_source = previous_active_index
                        .and_then(|prev| self.span_for_index(prev))
                        .unwrap_or((target_start, target_end));
                    let from_start = if self.underline_end > self.underline_start {
                        self.underline_start
                    } else {
                        fallback_source.0
                    };
                    let from_end = if self.underline_end > self.underline_start {
                        self.underline_end
                    } else {
                        fallback_source.1
                    };
                    // TODO(P1-14 integration): wire tree-based NodeId comparison
                    ctx.request_animation(
                        AnimationRequest::new(
                            NodeId::default(),
                            Self::UNDERLINE_START_ATTR,
                            from_start,
                            target_start,
                            duration,
                        )
                        .with_delay(delay)
                        .with_ease(ease)
                        .with_level(AnimationLevel::Basic),
                    );
                    // TODO(P1-14 integration): wire tree-based NodeId comparison
                    ctx.request_animation(
                        AnimationRequest::new(
                            NodeId::default(),
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
                    self.underline_start = 0.0;
                    self.underline_end = 0.0;
                }
                ctx.post_message(
                    Message::TabActivated(TabActivated {
                        id: self.tabs[next].tab_id.clone(),
                        index: next,
                        title: self.tabs[next].title.clone(),
                    }),
                );
                ctx.request_repaint();
            } else if let Some((target_start, target_end)) = target_span {
                self.underline_start = target_start;
                self.underline_end = target_end;
            } else {
                self.underline_start = 0.0;
                self.underline_end = 0.0;
            }
        }
        true
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

    fn clear_active(&mut self) {
        if let Some(idx) = self.active_index() {
            if let Some(tab) = self.tabs.get_mut(idx) {
                tab.child.set_focus(false);
            }
        }
        self.active = None;
        self.underline_start = 0.0;
        self.underline_end = 0.0;
    }

    fn ensure_active_exists(&mut self) {
        if let Some(idx) = self.active_index() {
            if self.is_visible(idx) {
                return;
            }
        }
        if let Some(next) = self.first_activatable() {
            self.active = Some(self.tabs[next].tab_id.clone());
        } else {
            self.active = None;
        }
    }

    fn is_visible(&self, index: usize) -> bool {
        self.tabs.get(index).map(|tab| !tab.hidden).unwrap_or(false)
    }

    fn is_activatable(&self, index: usize) -> bool {
        self.tabs
            .get(index)
            .map(|tab| !tab.hidden && !tab.disabled)
            .unwrap_or(false)
    }

    fn potential_active_indices(&self) -> Vec<usize> {
        let active_idx = self.active_index();
        self.tabs
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

    fn first_activatable(&self) -> Option<usize> {
        self.tabs
            .iter()
            .enumerate()
            .find(|(_, tab)| !tab.hidden && !tab.disabled)
            .map(|(index, _)| index)
    }

    fn last_activatable(&self) -> Option<usize> {
        self.tabs
            .iter()
            .enumerate()
            .rev()
            .find(|(_, tab)| !tab.hidden && !tab.disabled)
            .map(|(index, _)| index)
    }

    fn replacement_after_deactivation(&self, index: usize) -> Option<usize> {
        let mut candidates = self.potential_active_indices();
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
        let candidates = self.potential_active_indices();
        if candidates.is_empty() {
            return;
        }
        let active_idx = self.active_index();
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
                    self.first_activatable().unwrap_or(candidates[0])
                } else {
                    self.last_activatable()
                        .unwrap_or(*candidates.last().unwrap_or(&candidates[0]))
                }
            }
        };
        let _ = self.activate(target, ctx);
    }

    fn tab_spans(&self, width: usize) -> Vec<(usize, usize, usize)> {
        let mut spans = Vec::new();
        let mut cursor = 0usize;
        for (index, tab) in self.tabs.iter().enumerate() {
            if tab.hidden {
                continue;
            }
            if cursor >= width {
                break;
            }
            let label = format!(" {} ", tab.title);
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
            self.underline_start = start;
            self.underline_end = end;
        } else {
            self.underline_start = 0.0;
            self.underline_end = 0.0;
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

        let mut cells = vec![('─', false); width];
        let start = start.clamp(0.0, width as f32);
        let end = end.clamp(0.0, width as f32);
        if end > start {
            let start = (start * 2.0).round() / 2.0;
            let end = (end * 2.0).round() / 2.0;

            let full_start = start.ceil() as usize;
            let full_end = end.floor() as usize;
            for idx in full_start..full_end.min(width) {
                cells[idx] = ('━', true);
            }

            if start.fract().abs() > f32::EPSILON {
                let idx = start.floor() as usize;
                if idx < width {
                    cells[idx] = ('╺', true);
                }
            }

            if end.fract().abs() > f32::EPSILON {
                let idx = end.floor() as usize;
                if idx < width {
                    cells[idx] = if cells[idx].1 {
                        ('━', true)
                    } else {
                        ('╸', true)
                    };
                }
            }
        }

        let mut out = Vec::new();
        let mut current_active = cells[0].1;
        let mut buffer = String::new();
        for (ch, active) in cells {
            if active == current_active {
                buffer.push(ch);
            } else {
                let style = if current_active {
                    active_style.clone()
                } else {
                    base_style.clone()
                };
                out.push(Segment::styled(std::mem::take(&mut buffer), style));
                buffer.push(ch);
                current_active = active;
            }
        }
        if !buffer.is_empty() {
            let style = if current_active {
                active_style
            } else {
                base_style
            };
            out.push(Segment::styled(buffer, style));
        }
        out
    }
}

impl Widget for Tabs {
    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        if let Some(tab) = self.active_index().and_then(|idx| self.tabs.get_mut(idx)) {
            tab.child.set_focus(focused);
        }
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
        self.ensure_active_exists();
        for tab in &mut self.tabs {
            tab.child.on_mount();
        }
        self.sync_underline_to_active();
    }

    fn on_unmount(&mut self) {
        self.focused = false;
        self.hovered = false;
        self.hovered_tab = None;
        self.last_size = None;
        for tab in &mut self.tabs {
            tab.child.set_focus(false);
            tab.child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if let Some(tab) = self.active_index().and_then(|idx| self.tabs.get_mut(idx)) {
            tab.child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.last_size = Some((width, height));
        if let Some(tab) = self.active_index().and_then(|idx| self.tabs.get_mut(idx)) {
            tab.child
                .on_resize(width, height.saturating_sub(self.tab_row_height as u16));
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.last_size = Some((width, height));
        let next_layout_width = usize::from(width).max(1);
        if next_layout_width != self.layout_width {
            self.layout_width = next_layout_width;
            self.sync_underline_to_active();
        } else {
            self.layout_width = next_layout_width;
        }
        if let Some(tab) = self.active_index().and_then(|idx| self.tabs.get_mut(idx)) {
            tab.child
                .on_layout(width, height.saturating_sub(self.tab_row_height as u16));
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

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Some(tab) = self.active_index().and_then(|idx| self.tabs.get_mut(idx)) {
            tab.child.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        // Drain any pending messages queued by methods that lack EventCtx.
        for msg in self.pending_messages.drain(..) {
            ctx.post_message(msg);
        }

        if let Event::AnimationValue(AnimationValueEvent {
            target,
            attribute,
            value,
            ..
        }) = event
        {
            // TODO(P1-14 integration): wire tree-based NodeId comparison
            if *target == NodeId::default() {
                if attribute == Self::UNDERLINE_START_ATTR {
                    self.underline_start = *value;
                    ctx.request_repaint();
                    ctx.set_handled();
                    return;
                }
                if attribute == Self::UNDERLINE_END_ATTR {
                    self.underline_end = *value;
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
            // TODO(P1-14 integration): wire tree-based NodeId comparison
            if mouse.target == NodeId::default() {
                if let Some(index) = self.hit_tab(mouse.x as usize, mouse.y as usize) {
                    if self.activate(index, Some(ctx)) {
                        ctx.set_handled();
                        return;
                    }
                }
            }
        }
        if let Some(tab) = self.active_index().and_then(|idx| self.tabs.get_mut(idx)) {
            tab.child.on_event(event, ctx);
        }
    }

    fn on_message(&mut self, message: &crate::message::MessageEvent, ctx: &mut EventCtx) {
        if let Some(tab) = self.active_index().and_then(|idx| self.tabs.get_mut(idx)) {
            tab.child.on_message(message, ctx);
        }
    }

    fn binding_hints(&self) -> Vec<BindingHint> {
        if self.potential_active_indices().len() <= 1 {
            return Vec::new();
        }
        vec![BindingHint::new("left/right", "Switch tab").with_key_display("←/→")]
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        let hovered = self.hit_tab(x as usize, y as usize);
        if hovered != self.hovered_tab {
            self.hovered_tab = hovered;
            return true;
        }
        false
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let active_idx = self.active_index();
        let bar_style = crate::css::resolve_component_style(self, &["tabs--bar"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);
        let mut base_underline_classes = vec!["tabs--underline"];
        let mut active_underline_classes = vec!["tabs--underline", "-active"];
        if self.focused {
            base_underline_classes.push("-focus");
            active_underline_classes.push("-focus");
        }
        let base_underline_style =
            crate::css::resolve_component_style(self, &base_underline_classes)
                .to_rich()
                .unwrap_or_else(rich_rs::Style::new);
        let active_underline_style =
            crate::css::resolve_component_style(self, &active_underline_classes)
                .to_rich()
                .unwrap_or(base_underline_style);

        let mut header_line = Vec::new();
        if self.tabs.is_empty() {
            header_line.push(Segment::styled(" no tabs ".to_string(), bar_style));
        } else {
            for (idx, tab) in self.tabs.iter().enumerate() {
                if tab.hidden {
                    continue;
                }
                let mut classes = vec!["tabs--tab"];
                if tab.disabled {
                    classes.push("-disabled");
                }
                if active_idx == Some(idx) {
                    classes.push("-active");
                    if self.focused {
                        classes.push("-focus");
                    }
                }
                if self.hovered_tab == Some(idx) {
                    classes.push("-hover");
                }
                let style = crate::css::resolve_component_style(self, &classes)
                    .to_rich()
                    .unwrap_or(bar_style);
                header_line.push(Segment::styled(format!(" {} ", tab.title), style));
            }
        }
        let mut header_line = Segment::adjust_line_length(&header_line, width, None, false);
        let mut underline_line = Segment::adjust_line_length(
            &Self::render_underline_line(
                width,
                self.underline_start,
                self.underline_end,
                base_underline_style,
                active_underline_style,
            ),
            width,
            None,
            false,
        );
        let header_len = Segment::get_line_length(&header_line);
        if header_len < width {
            header_line.push(Segment::styled(" ".repeat(width - header_len), bar_style));
        }
        let underline_len = Segment::get_line_length(&underline_line);
        if underline_len < width {
            underline_line.push(Segment::styled(
                "─".repeat(width - underline_len),
                base_underline_style,
            ));
        }
        let mut lines = vec![header_line, underline_line];

        if height > self.tab_row_height {
            if let Some(tab) = active_idx.and_then(|idx| self.tabs.get(idx)) {
                let mut child_options = options.clone();
                child_options.size = (width, height - self.tab_row_height);
                child_options.max_width = width;
                child_options.max_height = height - self.tab_row_height;
                let child_segments = tab.child.render_styled(console, &child_options);
                let mut child_lines =
                    Segment::split_and_crop_lines(child_segments, width, None, true, false);
                child_lines = Segment::set_shape(
                    &child_lines,
                    width,
                    Some(height - self.tab_row_height),
                    None,
                    false,
                );
                lines.extend(child_lines);
            }
        }

        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        let child_height = self
            .active_index()
            .and_then(|idx| self.tabs.get(idx))
            .and_then(|tab| tab.child.layout_height());
        child_height.map(|height| height + self.tab_row_height)
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
            match change.field_name {
                "active" => {
                    // Side effects handled directly in set_active via activate().
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MouseDownEvent;
    use crate::keys::KeyEventData;
    use crate::prelude::Label;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct ProbeWidget {
        resize_calls: Arc<Mutex<Vec<(u16, u16)>>>,
        layout_calls: Arc<Mutex<Vec<(u16, u16)>>>,
        focus_calls: Arc<Mutex<Vec<bool>>>,
    }

    impl ProbeWidget {
        fn new(
            resize_calls: Arc<Mutex<Vec<(u16, u16)>>>,
            layout_calls: Arc<Mutex<Vec<(u16, u16)>>>,
            focus_calls: Arc<Mutex<Vec<bool>>>,
        ) -> Self {
            Self {
                resize_calls,
                layout_calls,
                focus_calls,
            }
        }
    }

    impl Widget for ProbeWidget {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn on_resize(&mut self, width: u16, height: u16) {
            self.resize_calls
                .lock()
                .expect("resize_calls lock")
                .push((width, height));
        }

        fn on_layout(&mut self, width: u16, height: u16) {
            self.layout_calls
                .lock()
                .expect("layout_calls lock")
                .push((width, height));
        }

        fn set_focus(&mut self, focused: bool) {
            self.focus_calls
                .lock()
                .expect("focus_calls lock")
                .push(focused);
        }
    }

    #[test]
    fn keyboard_activation_posts_message_and_requests_repaint() {
        let mut tabs = Tabs::new()
            .with_tab("One", Label::new("first"))
            .with_tab("Two", Label::new("second"));
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
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0].message,
            Message::TabActivated(TabActivated { index: 1, ref title, .. }) if title == "Two"
        ));
        assert_eq!(ctx.take_animation_requests().len(), 2);
    }

    #[test]
    fn clicking_active_tab_is_handled_but_emits_no_activation_message() {
        let mut tabs = Tabs::new()
            .with_tab("One", Label::new("first"))
            .with_tab("Two", Label::new("second"));
        tabs.on_layout(40, 6);

        // Use NodeId::default() as target — production code compares against
        // NodeId::default() for self-targeting (P1-14 migration).
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
        assert!(ctx.take_messages().is_empty());
        assert!(!ctx.repaint_requested());
        assert!(ctx.take_animation_requests().is_empty());
    }

    #[test]
    fn on_resize_forwards_content_height_to_active_tab() {
        let resize_calls = Arc::new(Mutex::new(Vec::new()));
        let layout_calls = Arc::new(Mutex::new(Vec::new()));
        let focus_calls = Arc::new(Mutex::new(Vec::new()));
        let probe = ProbeWidget::new(resize_calls.clone(), layout_calls, focus_calls);
        let mut tabs = Tabs::new().with_tab("One", probe);

        tabs.on_resize(80, 10);

        let calls = resize_calls.lock().expect("resize_calls lock");
        assert_eq!(*calls, vec![(80, 8)]);
    }

    #[test]
    fn activation_after_layout_forwards_latest_geometry_to_new_active_tab() {
        let first_resize_calls = Arc::new(Mutex::new(Vec::new()));
        let first_layout_calls = Arc::new(Mutex::new(Vec::new()));
        let first_focus_calls = Arc::new(Mutex::new(Vec::new()));
        let second_resize_calls = Arc::new(Mutex::new(Vec::new()));
        let second_layout_calls = Arc::new(Mutex::new(Vec::new()));
        let second_focus_calls = Arc::new(Mutex::new(Vec::new()));
        let first = ProbeWidget::new(first_resize_calls, first_layout_calls, first_focus_calls);
        let second = ProbeWidget::new(
            second_resize_calls.clone(),
            second_layout_calls.clone(),
            second_focus_calls,
        );
        let mut tabs = Tabs::new().with_tab("One", first).with_tab("Two", second);
        tabs.on_layout(60, 9);
        tabs.set_focus(true);
        let mut ctx = EventCtx::default();

        tabs.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Right,
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );

        let resize_calls = second_resize_calls.lock().expect("resize_calls lock");
        let layout_calls = second_layout_calls.lock().expect("layout_calls lock");
        assert_eq!(*resize_calls, vec![(60, 7)]);
        assert_eq!(*layout_calls, vec![(60, 7)]);
    }

    #[test]
    fn binding_hints_require_more_than_one_switchable_tab() {
        let mut tabs = Tabs::new()
            .with_tab("One", Label::new("first"))
            .with_tab("Two", Label::new("second"))
            .with_tab("Three", Label::new("third"));
        let mut rctx = ReactiveCtx::new(NodeId::default());
        assert!(tabs.disable_tab("Two", &mut rctx));
        assert!(tabs.hide_tab("Three", &mut rctx));

        assert!(tabs.binding_hints().is_empty());
    }

    #[test]
    fn on_unmount_resets_transient_state_and_unfocuses_children() {
        let resize_calls = Arc::new(Mutex::new(Vec::new()));
        let layout_calls = Arc::new(Mutex::new(Vec::new()));
        let focus_calls = Arc::new(Mutex::new(Vec::new()));
        let probe = ProbeWidget::new(resize_calls, layout_calls, focus_calls.clone());
        let mut tabs = Tabs::new().with_tab("One", probe);
        tabs.on_layout(30, 6);
        tabs.set_focus(true);
        tabs.set_hovered(true);
        assert!(tabs.on_mouse_move(1, 0));
        assert!(tabs.hovered_tab.is_some());
        assert!(tabs.has_focus());
        assert!(tabs.is_hovered());

        tabs.on_unmount();

        assert!(!tabs.has_focus());
        assert!(!tabs.is_hovered());
        assert!(tabs.hovered_tab.is_none());
        let focus_events = focus_calls.lock().expect("focus_calls lock");
        assert_eq!(*focus_events, vec![true, false]);
    }

    #[test]
    fn active_returns_string_id() {
        let tabs = Tabs::new()
            .with_tab("One", Label::new("first"))
            .with_tab("Two", Label::new("second"));
        assert_eq!(tabs.active(), Some("One"));
        assert_eq!(tabs.active_index(), Some(0));
    }

    #[test]
    fn set_active_by_id() {
        let mut tabs = Tabs::new()
            .with_tab("One", Label::new("first"))
            .with_tab("Two", Label::new("second"));
        let mut rctx = ReactiveCtx::new(NodeId::default());
        tabs.set_active("Two", &mut rctx);
        assert_eq!(tabs.active(), Some("Two"));
        assert_eq!(tabs.active_index(), Some(1));
    }

    #[test]
    fn with_tab_id_explicit_id() {
        let tabs = Tabs::new()
            .with_tab_id("tab-one", "Tab One", Label::new("first"))
            .with_tab_id("tab-two", "Tab Two", Label::new("second"));
        assert_eq!(tabs.active(), Some("tab-one"));
        assert!(tabs.is_tab_disabled("tab-one") == false);
    }

    #[test]
    fn remove_tab_active_moves_to_next() {
        let mut tabs = Tabs::new()
            .with_tab("One", Label::new("first"))
            .with_tab("Two", Label::new("second"))
            .with_tab("Three", Label::new("third"));
        assert_eq!(tabs.active(), Some("One"));

        assert!(tabs.remove_tab("One"));
        assert_eq!(tabs.tab_count(), 2);
        // Active should move to the next available tab
        assert!(tabs.active().is_some());
    }

    #[test]
    fn remove_tab_nonexistent_returns_false() {
        let mut tabs = Tabs::new().with_tab("One", Label::new("first"));
        assert!(!tabs.remove_tab("NonExistent"));
        assert_eq!(tabs.tab_count(), 1);
    }

    #[test]
    fn clear_removes_all_tabs() {
        let mut tabs = Tabs::new()
            .with_tab("One", Label::new("first"))
            .with_tab("Two", Label::new("second"));
        tabs.clear();
        assert_eq!(tabs.tab_count(), 0);
        assert_eq!(tabs.active(), None);
        assert_eq!(tabs.active_index(), None);
    }

    #[test]
    fn tab_activated_message_includes_id() {
        let mut tabs = Tabs::new()
            .with_tab_id("tab-1", "One", Label::new("first"))
            .with_tab_id("tab-2", "Two", Label::new("second"));
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

        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        match &messages[0].message {
            Message::TabActivated(TabActivated { id, index, title }) => {
                assert_eq!(id, "tab-2");
                assert_eq!(*index, 1);
                assert_eq!(title, "Two");
            }
            other => panic!("expected TabActivated, got {:?}", other),
        }
    }

    #[test]
    fn bindings_are_declared() {
        let tabs = Tabs::new()
            .with_tab("One", Label::new("first"))
            .with_tab("Two", Label::new("second"));
        let bindings = tabs.bindings();
        assert!(!bindings.is_empty());
        assert!(bindings.iter().any(|b| b.action == "previous"));
        assert!(bindings.iter().any(|b| b.action == "next"));
    }

    #[test]
    fn execute_action_handles_next() {
        use crate::action::ParsedAction;
        let mut tabs = Tabs::new()
            .with_tab("One", Label::new("first"))
            .with_tab("Two", Label::new("second"));
        tabs.set_focus(true);
        tabs.on_layout(40, 6);
        let mut ctx = EventCtx::default();
        let action = ParsedAction {
            namespace: None,
            name: "next".to_string(),
            arguments: vec![],
        };
        assert!(tabs.execute_action(&action, &mut ctx));
        assert_eq!(tabs.active(), Some("Two"));
    }
}
