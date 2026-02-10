use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};
use std::time::Duration;

use crate::event::{
    AnimationEase, AnimationLevel, AnimationRequest, AnimationValueEvent, BindingHint, Event,
    EventCtx,
};
use crate::message::Message;
use crate::style::TransitionTiming;

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
};

pub struct TabPane {
    title: String,
    pane_id: Option<String>,
    child: Box<dyn Widget>,
    disabled: bool,
    hidden: bool,
}

impl TabPane {
    pub fn new(title: impl Into<String>, child: impl Widget + 'static) -> Self {
        Self {
            title: title.into(),
            pane_id: None,
            child: Box::new(child),
            disabled: false,
            hidden: false,
        }
    }

    pub fn id(mut self, pane_id: impl Into<String>) -> Self {
        self.pane_id = Some(pane_id.into());
        self
    }

    fn component_selector_id(&self) -> Option<String> {
        self.pane_id
            .as_ref()
            .map(|pane_id| format!("--content-tab-{pane_id}"))
    }
}

pub struct TabbedContent {
    id: WidgetId,
    panes: Vec<TabPane>,
    active: Option<usize>,
    initial: Option<String>,
    focused: bool,
    hovered: bool,
    hovered_tab: Option<usize>,
    layout_width: usize,
    tab_row_height: usize,
    underline_start: f32,
    underline_end: f32,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

impl TabbedContent {
    const UNDERLINE_START_ATTR: &'static str = "tabbed_content.underline_start";
    const UNDERLINE_END_ATTR: &'static str = "tabbed_content.underline_end";
    const UNDERLINE_ANIMATION_DURATION: Duration = Duration::from_millis(300);
    const UNDERLINE_ANIMATION_DELAY: Duration = Duration::ZERO;

    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            panes: Vec::new(),
            active: None,
            initial: None,
            focused: false,
            hovered: false,
            hovered_tab: None,
            layout_width: 1,
            tab_row_height: 2,
            underline_start: 0.0,
            underline_end: 0.0,
            classes: vec!["tabbed-content".to_string()],
            focused_classes: vec!["tabbed-content".to_string(), "focused".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    pub fn initial(mut self, pane_id: impl Into<String>) -> Self {
        self.initial = Some(pane_id.into());
        self
    }

    pub fn with_pane(mut self, pane: TabPane) -> Self {
        self.panes.push(pane);
        if self.active.is_none() {
            self.active = Some(self.panes.len() - 1);
        }
        self
    }

    pub fn add_pane(&mut self, pane: TabPane) {
        self.panes.push(pane);
        if self.active.is_none() {
            self.active = Some(self.panes.len() - 1);
        }
    }

    pub fn active(&self) -> usize {
        self.active.unwrap_or(0)
    }

    pub fn active_id(&self) -> Option<&str> {
        self.panes
            .get(self.active?)
            .and_then(|pane| pane.pane_id.as_deref())
    }

    pub fn set_active(&mut self, index: usize) {
        let _ = self.activate(index, None);
    }

    pub fn set_active_id(&mut self, pane_id: &str) -> bool {
        let target = self
            .panes
            .iter()
            .position(|pane| pane.pane_id.as_deref() == Some(pane_id));
        if let Some(index) = target {
            return self.activate(index, None);
        }
        false
    }

    pub fn set_pane_disabled(&mut self, pane_id: &str, disabled: bool) -> bool {
        let Some(index) = self
            .panes
            .iter()
            .position(|pane| pane.pane_id.as_deref() == Some(pane_id))
        else {
            return false;
        };
        self.set_pane_disabled_index(index, disabled)
    }

    pub fn disable_pane(&mut self, pane_id: &str) -> bool {
        self.set_pane_disabled(pane_id, true)
    }

    pub fn enable_pane(&mut self, pane_id: &str) -> bool {
        self.set_pane_disabled(pane_id, false)
    }

    pub fn set_pane_hidden(&mut self, pane_id: &str, hidden: bool) -> bool {
        let Some(index) = self
            .panes
            .iter()
            .position(|pane| pane.pane_id.as_deref() == Some(pane_id))
        else {
            return false;
        };
        self.set_pane_hidden_index(index, hidden)
    }

    pub fn hide_pane(&mut self, pane_id: &str) -> bool {
        self.set_pane_hidden(pane_id, true)
    }

    pub fn show_pane(&mut self, pane_id: &str) -> bool {
        self.set_pane_hidden(pane_id, false)
    }

    pub fn is_pane_disabled(&self, pane_id: &str) -> bool {
        self.panes
            .iter()
            .find(|pane| pane.pane_id.as_deref() == Some(pane_id))
            .map(|pane| pane.disabled)
            .unwrap_or(false)
    }

    pub fn is_pane_hidden(&self, pane_id: &str) -> bool {
        self.panes
            .iter()
            .find(|pane| pane.pane_id.as_deref() == Some(pane_id))
            .map(|pane| pane.hidden)
            .unwrap_or(false)
    }

    fn set_pane_disabled_index(&mut self, index: usize, disabled: bool) -> bool {
        let Some(pane) = self.panes.get_mut(index) else {
            return false;
        };
        if pane.disabled == disabled {
            return true;
        }
        pane.disabled = disabled;
        if self.active.is_none() {
            self.ensure_active_exists();
        }
        self.sync_underline_to_active();
        true
    }

    fn set_pane_hidden_index(&mut self, index: usize, hidden: bool) -> bool {
        if index >= self.panes.len() {
            return false;
        }
        let was_hidden = self.panes[index].hidden;
        if was_hidden == hidden {
            return true;
        }
        let replacement = if hidden && self.active == Some(index) {
            self.replacement_after_deactivation(index)
        } else {
            None
        };
        self.panes[index].hidden = hidden;
        if hidden && self.active == Some(index) {
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
        if self.panes.is_empty() {
            self.clear_active();
            return false;
        }
        let next = index.min(self.panes.len() - 1);
        if !self.is_activatable(next) {
            return false;
        }
        let previous_active = self.active;
        if Some(next) != self.active {
            if let Some(prev) = previous_active.and_then(|idx| self.panes.get_mut(idx)) {
                prev.child.set_focus(false);
            }
            self.active = Some(next);
            if let Some(pane) = self.panes.get_mut(next) {
                pane.child.set_focus(self.focused);
            }
            let target_span = self.span_for_index(next);
            if let Some(ctx) = ctx.as_mut() {
                if let Some((target_start, target_end)) = target_span {
                    let (duration, delay, ease) = self.underline_animation_params();
                    let fallback_source = previous_active
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
                    ctx.request_animation(
                        AnimationRequest::new(
                            self.id,
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
                            self.id,
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
                let title = self.panes[next].title.clone();
                ctx.post_message(self.id, Message::TabActivated { index: next, title });
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

    fn activate_prev_with_ctx(&mut self, ctx: Option<&mut EventCtx>) {
        self.move_active(-1, ctx);
    }

    fn activate_next_with_ctx(&mut self, ctx: Option<&mut EventCtx>) {
        self.move_active(1, ctx);
    }

    fn clear_active(&mut self) {
        if let Some(active) = self.active.and_then(|idx| self.panes.get_mut(idx)) {
            active.child.set_focus(false);
        }
        self.active = None;
        self.underline_start = 0.0;
        self.underline_end = 0.0;
    }

    fn ensure_active_exists(&mut self) {
        if let Some(active) = self.active {
            if self.is_visible(active) {
                return;
            }
        }
        if let Some(next) = self.first_activatable() {
            self.active = Some(next);
        } else {
            self.active = None;
        }
    }

    fn is_visible(&self, index: usize) -> bool {
        self.panes
            .get(index)
            .map(|pane| !pane.hidden)
            .unwrap_or(false)
    }

    fn is_activatable(&self, index: usize) -> bool {
        self.panes
            .get(index)
            .map(|pane| !pane.hidden && !pane.disabled)
            .unwrap_or(false)
    }

    fn potential_active_indices(&self) -> Vec<usize> {
        self.panes
            .iter()
            .enumerate()
            .filter_map(|(index, pane)| {
                if pane.hidden {
                    return None;
                }
                if pane.disabled && Some(index) != self.active {
                    return None;
                }
                Some(index)
            })
            .collect()
    }

    fn first_activatable(&self) -> Option<usize> {
        self.panes
            .iter()
            .enumerate()
            .find(|(_, pane)| !pane.hidden && !pane.disabled)
            .map(|(index, _)| index)
    }

    fn last_activatable(&self) -> Option<usize> {
        self.panes
            .iter()
            .enumerate()
            .rev()
            .find(|(_, pane)| !pane.hidden && !pane.disabled)
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
        let target = match self.active {
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
        for (index, pane) in self.panes.iter().enumerate() {
            if pane.hidden {
                continue;
            }
            if cursor >= width {
                break;
            }
            let label = format!(" {} ", pane.title);
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
        if let Some(active) = self.active
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
        let style =
            crate::css::resolve_component_style(self, &["tabbed-content--underline", "-active"]);
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

impl Widget for TabbedContent {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        if let Some(pane) = self.active.and_then(|idx| self.panes.get_mut(idx)) {
            pane.child.set_focus(focused);
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
        if let Some(initial) = self.initial.clone() {
            let _ = self.set_active_id(&initial);
        }
        for pane in &mut self.panes {
            pane.child.on_mount();
        }
        self.sync_underline_to_active();
    }

    fn on_unmount(&mut self) {
        for pane in &mut self.panes {
            pane.child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if let Some(pane) = self.active.and_then(|idx| self.panes.get_mut(idx)) {
            pane.child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        if let Some(pane) = self.active.and_then(|idx| self.panes.get_mut(idx)) {
            pane.child.on_resize(width, height);
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        let next_layout_width = usize::from(width).max(1);
        if next_layout_width != self.layout_width {
            self.layout_width = next_layout_width;
            self.sync_underline_to_active();
        } else {
            self.layout_width = next_layout_width;
        }
        if let Some(pane) = self.active.and_then(|idx| self.panes.get_mut(idx)) {
            pane.child
                .on_layout(width, height.saturating_sub(self.tab_row_height as u16));
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Some(pane) = self.active.and_then(|idx| self.panes.get_mut(idx)) {
            pane.child.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::AnimationValue(AnimationValueEvent {
            target,
            attribute,
            value,
            ..
        }) = event
        {
            if *target == self.id {
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
                    KeyCode::Left | KeyCode::Char('h') => {
                        self.activate_prev_with_ctx(Some(ctx));
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        self.activate_next_with_ctx(Some(ctx));
                        ctx.set_handled();
                        return;
                    }
                    _ => {}
                }
            }
        }
        if let Event::MouseDown(mouse) = event {
            if mouse.target == self.id {
                if let Some(index) = self.hit_tab(mouse.x as usize, mouse.y as usize) {
                    if self.activate(index, Some(ctx)) {
                        ctx.set_handled();
                        return;
                    }
                }
            }
        }
        if let Some(pane) = self.active.and_then(|idx| self.panes.get_mut(idx)) {
            pane.child.on_event(event, ctx);
        }
    }

    fn on_message(&mut self, message: &crate::message::MessageEvent, ctx: &mut EventCtx) {
        if let Some(pane) = self.active.and_then(|idx| self.panes.get_mut(idx)) {
            pane.child.on_message(message, ctx);
        }
    }

    fn binding_hints(&self) -> Vec<BindingHint> {
        if self.panes.len() <= 1 {
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

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        for pane in &mut self.panes {
            f(pane.child.as_mut());
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let bar_style = crate::css::resolve_component_style(self, &["tabbed-content--bar"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);
        let mut base_underline_classes = vec!["tabbed-content--underline"];
        let mut active_underline_classes = vec!["tabbed-content--underline", "-active"];
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
        if self.panes.is_empty() {
            header_line.push(Segment::styled(" no panes ".to_string(), bar_style));
        } else {
            for (idx, pane) in self.panes.iter().enumerate() {
                if pane.hidden {
                    continue;
                }
                let mut classes = vec!["tabbed-content--tab"];
                if pane.disabled {
                    classes.push("-disabled");
                }
                if self.active == Some(idx) {
                    classes.push("-active");
                    if self.focused {
                        classes.push("-focus");
                    }
                }
                if self.hovered_tab == Some(idx) {
                    classes.push("-hover");
                }
                let selector_id = pane.component_selector_id();
                let style = crate::css::resolve_component_style_with_id(
                    self,
                    selector_id.as_deref(),
                    &classes,
                )
                .to_rich()
                .unwrap_or(bar_style);
                let tab_text = format!(" {} ", pane.title);
                header_line.push(Segment::styled(tab_text, style));
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
            if let Some(pane) = self.active.and_then(|idx| self.panes.get(idx)) {
                let mut child_options = options.clone();
                child_options.size = (width, height - self.tab_row_height);
                child_options.max_width = width;
                child_options.max_height = height - self.tab_row_height;
                let child_segments = pane.child.render_styled(console, &child_options);
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
            .active
            .and_then(|idx| self.panes.get(idx))
            .and_then(|pane| pane.child.layout_height());
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

impl Renderable for TabbedContent {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
