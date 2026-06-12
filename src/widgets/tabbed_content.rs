use crate::action::{ActionDecl, ParsedAction};
use crate::compose::{ChildDecl, ComposeResult};
use crate::event::{BindingHint, Event, EventCtx};
use crate::message::{TabActivated, TabsCleared};
use crate::reactive::ReactiveCtx;
use crate::widgets::delegate::{delegate_renderable, delegate_widget_method};
use crate::widgets::{Container, Widget, WidgetStyles, helpers::empty_classes};
use rich_rs::{Console, ConsoleOptions, Renderable, Segments};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use super::tabs::{Tab, Tabs};

struct ContentTabs {
    inner: Tabs,
}

impl ContentTabs {
    fn new(inner: Tabs) -> Self {
        Self { inner }
    }
}

impl Widget for ContentTabs {
    fn style_type(&self) -> &'static str {
        "Tabs"
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(&self.inner, console, options)
    }

    delegate_widget_method!(
        inner,
        [
            render_with_debug,
            compose,
            focusable,
            set_focus,
            has_focus,
            on_mount,
            on_unmount,
            on_tick,
            on_resize,
            on_layout,
            on_event_capture,
            on_event,
            on_message,
            layout_height,
            styles,
            styles_mut,
            style_classes,
            is_hovered,
            set_hovered,
        ]
    );
}

delegate_renderable!(ContentTabs);

#[derive(Debug, Clone)]
pub struct TabPaneMeta {
    pub id: String,
    pub title: String,
    pub disabled: bool,
    pub hidden: bool,
}

pub struct TabPane {
    title: String,
    pane_id: Option<String>,
    inner: Container,
    styles: WidgetStyles,
    children_extracted: bool,
    disabled: bool,
    hidden: bool,
}

impl TabPane {
    pub fn new(title: impl Into<String>, child: impl Widget + 'static) -> Self {
        let mut inner = Container::new();
        inner.push(child);
        Self {
            title: title.into(),
            pane_id: None,
            inner,
            styles: WidgetStyles::default(),
            children_extracted: false,
            disabled: false,
            hidden: false,
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.inner.push(child);
        self
    }

    pub fn id(mut self, pane_id: impl Into<String>) -> Self {
        let id = pane_id.into();
        self.pane_id = Some(id.clone());
        self.styles.style_id = Some(id);
        self
    }

    pub fn title(&self) -> &str {
        self.title.as_str()
    }

    pub fn pane_id(&self) -> Option<&str> {
        self.pane_id.as_deref()
    }

    pub fn disabled(&self) -> bool {
        self.disabled
    }

    pub fn hidden(&self) -> bool {
        self.hidden
    }

    fn is_tree_mode(&self) -> bool {
        self.children_extracted
    }

    fn assign_id(&mut self, id: String) {
        if self.pane_id.is_none() {
            self.pane_id = Some(id.clone());
            self.styles.style_id = Some(id);
        }
    }
}

impl Widget for TabPane {
    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.children_extracted = true;
        self.inner.take_composed_children()
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        if self.is_tree_mode() {
            return Segments::new();
        }
        Widget::render(&self.inner, console, options)
    }

    fn on_mount(&mut self) {
        if !self.is_tree_mode() {
            self.inner.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        if !self.is_tree_mode() {
            self.inner.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if !self.is_tree_mode() {
            self.inner.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        if !self.is_tree_mode() {
            self.inner.on_resize(width, height);
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        if !self.is_tree_mode() {
            self.inner.on_layout(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.is_tree_mode() {
            self.inner.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.is_tree_mode() {
            self.inner.on_event(event, ctx);
        }
    }

    fn on_message(&mut self, message: &crate::message::MessageEvent, ctx: &mut EventCtx) {
        if !self.is_tree_mode() {
            self.inner.on_message(message, ctx);
        }
    }

    fn focusable(&self) -> bool {
        false
    }

    fn style_type(&self) -> &'static str {
        "TabPane"
    }

    fn is_disabled(&self) -> bool {
        self.disabled
    }

    fn set_disabled_state(&mut self, disabled: bool) {
        self.disabled = disabled;
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for TabPane {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct TabbedContent {
    panes: Mutex<Vec<TabPane>>,
    pane_meta: Mutex<Vec<TabPaneMeta>>,
    pane_counter: usize,
    active: Option<String>,
    initial: Option<String>,
    focused: bool,
    hovered: bool,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
    children_extracted: AtomicBool,
    tabs_handle: Mutex<Option<Tabs>>,
}

impl TabbedContent {
    const CONTENT_TAB_PREFIX: &'static str = "--content-tab-";

    pub fn new() -> Self {
        Self {
            panes: Mutex::new(Vec::new()),
            pane_meta: Mutex::new(Vec::new()),
            pane_counter: 0,
            active: None,
            initial: None,
            focused: false,
            hovered: false,
            classes: vec!["tabbed-content".to_string()],
            focused_classes: vec!["tabbed-content".to_string(), "focused".to_string()],
            styles: WidgetStyles::default(),
            children_extracted: AtomicBool::new(false),
            tabs_handle: Mutex::new(None),
        }
    }

    pub fn initial(mut self, pane_id: impl Into<String>) -> Self {
        self.initial = Some(pane_id.into());
        self
    }

    pub fn with_pane(mut self, mut pane: TabPane) -> Self {
        let id = self.ensure_pane_id(&mut pane);
        self.push_meta(&pane, id);
        self.panes.lock().expect("tabbed panes lock").push(pane);
        self.sync_active_to_initial_or_first();
        self
    }

    pub fn add_pane(&mut self, mut pane: TabPane) {
        let id = self.ensure_pane_id(&mut pane);
        self.push_meta(&pane, id);
        self.panes.lock().expect("tabbed panes lock").push(pane);
        self.sync_active_to_initial_or_first();
    }

    pub fn active_id(&self) -> Option<&str> {
        self.active.as_deref()
    }

    pub fn set_active_id(&mut self, pane_id: &str, ctx: Option<&mut EventCtx>) -> bool {
        if self.active.as_deref() == Some(pane_id) {
            return false;
        }
        if !self
            .pane_meta
            .lock()
            .expect("tabbed meta lock")
            .iter()
            .any(|m| m.id == pane_id && !m.disabled && !m.hidden)
        {
            return false;
        }
        self.active = Some(pane_id.to_string());
        let mut ctx_opt = ctx;
        if let Some(tabs) = self
            .tabs_handle
            .lock()
            .expect("tabbed tabs handle lock")
            .as_mut()
        {
            let _ = tabs.set_active_id(&Self::content_tab_id(pane_id), ctx_opt.as_deref_mut());
        }
        if let Some(ctx) = ctx_opt {
            ctx.request_layout_invalidation();
            ctx.request_repaint();
        }
        true
    }

    pub fn set_active(&mut self, index: usize) {
        let pane_id = self
            .pane_meta
            .lock()
            .expect("tabbed meta lock")
            .get(index)
            .map(|meta| meta.id.clone());
        if let Some(pane_id) = pane_id {
            let _ = self.set_active_id(&pane_id, None);
        }
    }

    pub fn get_pane(&self, pane_id: &str) -> Option<TabPaneMeta> {
        self.pane_meta
            .lock()
            .expect("tabbed meta lock")
            .iter()
            .find(|m| m.id == pane_id)
            .cloned()
    }

    pub fn get_tab(&self, tab_id: &str) -> Option<TabPaneMeta> {
        self.get_pane(tab_id)
    }

    pub fn disable_pane(&mut self, pane_id: &str) -> bool {
        self.set_pane_disabled(pane_id, true)
    }

    pub fn enable_pane(&mut self, pane_id: &str) -> bool {
        self.set_pane_disabled(pane_id, false)
    }

    pub fn hide_pane(&mut self, pane_id: &str) -> bool {
        self.set_pane_hidden(pane_id, true)
    }

    pub fn show_pane(&mut self, pane_id: &str) -> bool {
        self.set_pane_hidden(pane_id, false)
    }

    pub fn disable_tab(&mut self, tab_id: &str) -> bool {
        self.disable_pane(tab_id)
    }

    pub fn enable_tab(&mut self, tab_id: &str) -> bool {
        self.enable_pane(tab_id)
    }

    pub fn hide_tab(&mut self, tab_id: &str) -> bool {
        self.hide_pane(tab_id)
    }

    pub fn show_tab(&mut self, tab_id: &str) -> bool {
        self.show_pane(tab_id)
    }

    fn set_pane_disabled(&mut self, pane_id: &str, disabled: bool) -> bool {
        let mut meta = self.pane_meta.lock().expect("tabbed meta lock");
        let Some(pane) = meta.iter_mut().find(|m| m.id == pane_id) else {
            return false;
        };
        pane.disabled = disabled;
        drop(meta);
        if let Some(tabs) = self
            .tabs_handle
            .lock()
            .expect("tabbed tabs handle lock")
            .as_mut()
        {
            let mut rctx = ReactiveCtx::new(tabs.node_id());
            if disabled {
                let _ = tabs.disable_tab(&Self::content_tab_id(pane_id), &mut rctx);
            } else {
                let _ = tabs.enable_tab(&Self::content_tab_id(pane_id), &mut rctx);
            }
        }
        self.promote_active_if_needed();
        true
    }

    fn set_pane_hidden(&mut self, pane_id: &str, hidden: bool) -> bool {
        let mut meta = self.pane_meta.lock().expect("tabbed meta lock");
        let Some(pane) = meta.iter_mut().find(|m| m.id == pane_id) else {
            return false;
        };
        pane.hidden = hidden;
        drop(meta);
        if let Some(tabs) = self
            .tabs_handle
            .lock()
            .expect("tabbed tabs handle lock")
            .as_mut()
        {
            let mut rctx = ReactiveCtx::new(tabs.node_id());
            if hidden {
                let _ = tabs.hide_tab(&Self::content_tab_id(pane_id), &mut rctx);
            } else {
                let _ = tabs.show_tab(&Self::content_tab_id(pane_id), &mut rctx);
            }
        }
        self.promote_active_if_needed();
        true
    }

    fn ensure_pane_id(&mut self, pane: &mut TabPane) -> String {
        if let Some(id) = pane.pane_id() {
            return id.to_string();
        }
        self.pane_counter += 1;
        let id = format!("tab-{}", self.pane_counter);
        pane.assign_id(id.clone());
        id
    }

    fn push_meta(&mut self, pane: &TabPane, id: String) {
        self.pane_meta
            .lock()
            .expect("tabbed meta lock")
            .push(TabPaneMeta {
                id,
                title: pane.title().to_string(),
                disabled: pane.disabled(),
                hidden: pane.hidden(),
            });
    }

    fn sync_active_to_initial_or_first(&mut self) {
        if let Some(initial) = self.initial.as_deref() {
            let has_initial = self
                .pane_meta
                .lock()
                .expect("tabbed meta lock")
                .iter()
                .any(|meta| meta.id == initial);
            if has_initial {
                self.active = Some(initial.to_string());
                return;
            }
        }

        if self.active.is_none() {
            let panes = self.panes.lock().expect("tabbed panes lock");
            self.active = panes
                .last()
                .and_then(|p| p.pane_id())
                .map(|id| id.to_string());
        }
    }

    fn content_tab_id(pane_id: &str) -> String {
        format!("{}{}", Self::CONTENT_TAB_PREFIX, pane_id)
    }

    fn sans_content_tab_id(tab_id: &str) -> Option<String> {
        if tab_id.starts_with(Self::CONTENT_TAB_PREFIX) {
            Some(tab_id[Self::CONTENT_TAB_PREFIX.len()..].to_string())
        } else {
            None
        }
    }

    fn build_tabs(&self) -> Tabs {
        let mut tabs = Tabs::new();
        for meta in self.pane_meta.lock().expect("tabbed meta lock").iter() {
            let tab = Tab::new(meta.title.clone())
                .id(Self::content_tab_id(&meta.id))
                .disabled(meta.disabled);
            tabs.add_tab(tab);
            if meta.hidden {
                let mut rctx = ReactiveCtx::new(tabs.node_id());
                let _ = tabs.hide_tab(&Self::content_tab_id(&meta.id), &mut rctx);
            }
        }
        tabs.set_dock(crate::style::Dock::Top);
        tabs.set_focus(self.focused);
        if let Some(initial) = self.initial.as_deref().or(self.active.as_deref()) {
            let _ = tabs.set_active_id(&Self::content_tab_id(initial), None);
        }
        tabs
    }

    fn selectable_pane_count(&self) -> usize {
        self.pane_meta
            .lock()
            .expect("tabbed meta lock")
            .iter()
            .filter(|pane| !pane.hidden && !pane.disabled)
            .count()
    }

    fn next_selectable_pane_id(&self, current: Option<&str>, forward: bool) -> Option<String> {
        let meta = self.pane_meta.lock().expect("tabbed meta lock");
        if meta.is_empty() {
            return None;
        }
        let selectable = |idx: usize| !meta[idx].hidden && !meta[idx].disabled;
        let start = current
            .and_then(|id| meta.iter().position(|pane| pane.id == id))
            .unwrap_or(0);
        if current.is_none() && selectable(start) {
            return Some(meta[start].id.clone());
        }
        for step in 1..=meta.len() {
            let idx = if forward {
                (start + step) % meta.len()
            } else {
                (start + meta.len() - (step % meta.len())) % meta.len()
            };
            if selectable(idx) {
                return Some(meta[idx].id.clone());
            }
        }
        None
    }

    fn promote_active_if_needed(&mut self) {
        let active_valid = {
            let meta = self.pane_meta.lock().expect("tabbed meta lock");
            self.active
                .as_deref()
                .and_then(|active| meta.iter().find(|pane| pane.id == active))
                .map(|pane| !pane.hidden && !pane.disabled)
                .unwrap_or(false)
        };
        if active_valid {
            return;
        }
        self.active = self.next_selectable_pane_id(self.active.as_deref(), true);
    }

    fn hit_tab_index(&self, x: usize, y: usize) -> Option<usize> {
        if y > 0 {
            return None;
        }
        let meta = self.pane_meta.lock().expect("tabbed meta lock");
        let mut cursor = 0usize;
        for (idx, pane) in meta.iter().enumerate() {
            if pane.hidden {
                continue;
            }
            let tab_width = rich_rs::cell_len(format!(" {} ", pane.title).as_str());
            let start = cursor;
            let end = cursor.saturating_add(tab_width);
            if x >= start && x < end {
                return Some(idx);
            }
            cursor = end;
        }
        None
    }
}

impl Widget for TabbedContent {
    fn compose(&self) -> ComposeResult {
        if self.children_extracted.load(Ordering::SeqCst) {
            return Vec::new();
        }
        self.children_extracted.store(true, Ordering::SeqCst);
        let tabs = self.build_tabs();
        self.tabs_handle
            .lock()
            .expect("tabbed tabs handle lock")
            .replace(tabs.clone());
        let mut panes = self.panes.lock().expect("tabbed panes lock");
        let pane_widgets: Vec<TabPane> = std::mem::take(&mut *panes);
        let mut children = Vec::new();
        children.push(ChildDecl::from(ContentTabs::new(tabs)));
        for pane in pane_widgets {
            children.push(ChildDecl::from(pane));
        }
        children
    }

    fn action_namespace(&self) -> &str {
        "tabbed_content"
    }

    fn action_registry(&self) -> &[ActionDecl] {
        const ACTIONS: &[ActionDecl] = &[ActionDecl {
            name: "show_tab",
            namespace: "tabbed_content",
            description: "Switch to a tab by ID",
            default_binding: None,
        }];
        ACTIONS
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut EventCtx) -> bool {
        if action.name != "show_tab" {
            return false;
        }
        let Some(tab_id) = action.arguments.first() else {
            return false;
        };
        if !self.set_active_id(tab_id, Some(ctx)) {
            return false;
        }
        ctx.set_handled();
        true
    }

    fn focusable(&self) -> bool {
        false
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
    }

    fn on_mount(&mut self) {
        if let Some(initial) = self.initial.clone() {
            let _ = self.set_active_id(&initial, None);
        }
        self.promote_active_if_needed();
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        if let Some(tabs) = self
            .tabs_handle
            .lock()
            .expect("tabbed tabs handle lock")
            .as_mut()
        {
            tabs.set_focus(self.focused);
            tabs.on_layout(width, 2);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.children_extracted.load(Ordering::SeqCst) {
            if matches!(event, Event::AnimationValue(_))
                && let Some(tabs) = self
                    .tabs_handle
                    .lock()
                    .expect("tabbed tabs handle lock")
                    .as_mut()
            {
                tabs.on_event(event, ctx);
            }
            return;
        }
        if self.focused
            && let Event::Key(key) = event
        {
            match key.code {
                crossterm::event::KeyCode::Left | crossterm::event::KeyCode::Char('h') => {
                    if let Some(next) = self.next_selectable_pane_id(self.active.as_deref(), false)
                        && self.set_active_id(&next, Some(ctx))
                    {
                        ctx.set_handled();
                    }
                    return;
                }
                crossterm::event::KeyCode::Right | crossterm::event::KeyCode::Char('l') => {
                    if let Some(next) = self.next_selectable_pane_id(self.active.as_deref(), true)
                        && self.set_active_id(&next, Some(ctx))
                    {
                        ctx.set_handled();
                    }
                    return;
                }
                _ => {}
            }
        }
        if let Event::MouseDown(mouse) = event
            && let Some(index) = self.hit_tab_index(mouse.x as usize, mouse.y as usize)
        {
            let meta = self.pane_meta.lock().expect("tabbed meta lock");
            let Some(pane) = meta.get(index) else {
                return;
            };
            if pane.disabled {
                return;
            }
            let pane_id = pane.id.clone();
            drop(meta);
            if self.active.as_deref() == Some(pane_id.as_str()) {
                ctx.set_handled();
                return;
            }
            if self.set_active_id(&pane_id, Some(ctx)) {
                ctx.set_handled();
            }
        }
    }

    fn on_message(&mut self, message: &crate::message::MessageEvent, ctx: &mut EventCtx) {
        if let Some(m) = message.downcast_ref::<TabActivated>() {
            if let Some(pane_id) = Self::sans_content_tab_id(&m.id) {
                if self.set_active_id(&pane_id, Some(ctx)) {
                    ctx.set_handled();
                }
            }
        } else if message.is::<TabsCleared>() {
            self.active = None;
            ctx.request_layout_invalidation();
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    fn child_display_for_tree(&self, child_index: usize) -> Option<bool> {
        if child_index == 0 {
            return Some(true);
        }
        let pane_index = child_index.saturating_sub(1);
        let meta = self.pane_meta.lock().expect("tabbed meta lock");
        let Some(pane) = meta.get(pane_index) else {
            return None;
        };
        if pane.hidden {
            return Some(false);
        }
        Some(self.active.as_deref() == Some(pane.id.as_str()))
    }

    fn binding_hints(&self) -> Vec<BindingHint> {
        if self.selectable_pane_count() <= 1 {
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

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        if self.children_extracted.load(Ordering::SeqCst) {
            return Segments::new();
        }
        let width = options.size.0.max(1);
        let height = options.size.1.max(1).min(2);
        let mut tabs = self.build_tabs();
        tabs.set_focus(self.focused);
        tabs.on_layout(width as u16, height as u16);
        let mut tab_options = options.clone();
        tab_options.size = (width, height);
        tab_options.max_width = width;
        tab_options.max_height = height;
        Widget::render(&tabs, console, &tab_options)
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

impl Default for TabbedContent {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MouseDownEvent;
    use crate::keys::KeyEventData;
    use crate::message::{Message, MessageEvent};
    use crate::prelude::{Label, Markdown};
    use crate::runtime::{build_widget_tree_from_root, render_tree_to_frame};
    use crate::widget_tree::WidgetTree;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use rich_rs::Console;

    #[test]
    fn keyboard_activation_handles_and_switches_active_pane_in_non_tree_mode() {
        let mut tabs = TabbedContent::new()
            .with_pane(TabPane::new("One", Label::new("first")).id("one"))
            .with_pane(TabPane::new("Two", Label::new("second")).id("two"));
        tabs.set_focus(true);

        let mut ctx = EventCtx::default();
        tabs.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Right,
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );

        assert!(ctx.handled());
        assert_eq!(tabs.active_id(), Some("two"));
    }

    #[test]
    fn clicking_active_tab_is_handled_without_switching_active_pane() {
        let mut tabs = TabbedContent::new()
            .with_pane(TabPane::new("One", Label::new("first")).id("one"))
            .with_pane(TabPane::new("Two", Label::new("second")).id("two"));

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
        assert_eq!(tabs.active_id(), Some("one"));
    }

    fn apply_runtime_class_messages(tree: &mut WidgetTree, messages: Vec<MessageEvent>) {
        for event in messages {
            match event.message {
                Message::AppAddClass(payload) => {
                    let matches = tree
                        .query(&payload.selector)
                        .expect("selector should parse");
                    for node in matches {
                        tree.add_class(node, &payload.class_name);
                    }
                }
                Message::AppRemoveClass(payload) => {
                    let matches = tree
                        .query(&payload.selector)
                        .expect("selector should parse");
                    for node in matches {
                        tree.remove_class(node, &payload.class_name);
                    }
                }
                _ => {}
            }
        }
    }

    fn find_label_column(line: &str, label: &str) -> usize {
        line.find(label)
            .unwrap_or_else(|| panic!("missing label '{label}' in line: {line:?}"))
    }

    #[test]
    fn tree_mode_initial_active_tab_style_and_underline_are_distinct() {
        let mut tabs = TabbedContent::new()
            .initial("jessica")
            .with_pane(TabPane::new("Leto", Label::new("first")).id("leto"))
            .with_pane(TabPane::new("Jessica", Label::new("second")).id("jessica"))
            .with_pane(TabPane::new("Paul", Label::new("third")).id("paul"));

        let mut tree = build_widget_tree_from_root(&mut tabs).expect("tree should exist");
        let console = Console::new();
        let frame = render_tree_to_frame(&mut tree, &mut tabs, &console, 60, 8);
        let lines = frame.as_plain_lines();
        let header = lines.first().expect("tab header row");
        let leto_col = find_label_column(header, "Leto");
        let jessica_col = find_label_column(header, "Jessica");

        let leto_style = frame.get(leto_col, 0).style.expect("inactive tab style");
        let jessica_style = frame.get(jessica_col, 0).style.expect("active tab style");
        assert_ne!(
            leto_style.color, jessica_style.color,
            "active and inactive tabs must render with different colors in tree mode"
        );

        let inactive_underline = frame
            .get(leto_col, 1)
            .style
            .expect("inactive underline style");
        let active_underline = frame
            .get(jessica_col, 1)
            .style
            .expect("active underline style");
        assert_ne!(
            inactive_underline.color, active_underline.color,
            "active underline color must differ from inactive underline in tree mode"
        );
    }

    #[test]
    fn tree_mode_switching_active_tab_moves_active_class_visuals() {
        let mut tabs = TabbedContent::new()
            .initial("jessica")
            .with_pane(TabPane::new("Leto", Label::new("first")).id("leto"))
            .with_pane(TabPane::new("Jessica", Label::new("second")).id("jessica"))
            .with_pane(TabPane::new("Paul", Label::new("third")).id("paul"));

        let mut tree = build_widget_tree_from_root(&mut tabs).expect("tree should exist");
        let console = Console::new();

        let before = render_tree_to_frame(&mut tree, &mut tabs, &console, 60, 8);
        let before_header = before.as_plain_lines()[0].clone();
        let jessica_col = find_label_column(&before_header, "Jessica");
        let paul_col = find_label_column(&before_header, "Paul");
        let before_jessica_color = before
            .get(jessica_col, 0)
            .style
            .expect("before jessica style")
            .color;
        let before_paul_color = before
            .get(paul_col, 0)
            .style
            .expect("before paul style")
            .color;
        assert_ne!(
            before_jessica_color, before_paul_color,
            "sanity: initial active tab must differ from inactive tab"
        );

        let mut ctx = EventCtx::default();
        assert!(tabs.set_active_id("paul", Some(&mut ctx)));
        apply_runtime_class_messages(&mut tree, ctx.take_messages());

        let after = render_tree_to_frame(&mut tree, &mut tabs, &console, 60, 8);
        let after_jessica_color = after
            .get(jessica_col, 0)
            .style
            .expect("after jessica style")
            .color;
        let after_paul_color = after
            .get(paul_col, 0)
            .style
            .expect("after paul style")
            .color;

        assert_ne!(
            after_jessica_color, after_paul_color,
            "active/inactive tab styles must still differ after switching"
        );
        assert_eq!(
            after_paul_color, before_jessica_color,
            "active style color should move from Jessica to Paul when switching tabs"
        );
    }

    #[test]
    fn tree_mode_show_tab_action_moves_active_highlight_style() {
        let mut tabs = TabbedContent::new()
            .initial("jessica")
            .with_pane(TabPane::new("Leto", Label::new("first")).id("leto"))
            .with_pane(TabPane::new("Jessica", Label::new("second")).id("jessica"))
            .with_pane(TabPane::new("Paul", Label::new("third")).id("paul"));

        let mut tree = build_widget_tree_from_root(&mut tabs).expect("tree should exist");
        let tab_nodes = tree.query("Tabs").expect("Tabs selector should parse");
        let tabs_id = *tab_nodes.first().expect("expected top Tabs node");
        tree.get_mut(tabs_id)
            .expect("tabs node should exist")
            .widget
            .set_focus(true);

        let console = Console::new();
        let before = render_tree_to_frame(&mut tree, &mut tabs, &console, 60, 8);
        let before_header = before.as_plain_lines()[0].clone();
        let jessica_col = find_label_column(&before_header, "Jessica");
        let paul_col = find_label_column(&before_header, "Paul");

        let before_jessica_style = before
            .get(jessica_col, 0)
            .style
            .expect("before jessica style");
        let before_paul_style = before.get(paul_col, 0).style.expect("before paul style");
        assert_ne!(
            before_jessica_style.bgcolor, before_paul_style.bgcolor,
            "sanity: initial active tab should have distinct background"
        );

        let parsed = ParsedAction {
            namespace: None,
            name: "show_tab".to_string(),
            arguments: vec!["paul".to_string()],
        };
        let mut ctx = EventCtx::default();
        assert!(tabs.execute_action(&parsed, &mut ctx));
        apply_runtime_class_messages(&mut tree, ctx.take_messages());

        let after = render_tree_to_frame(&mut tree, &mut tabs, &console, 60, 8);
        let after_jessica_style = after
            .get(jessica_col, 0)
            .style
            .expect("after jessica style");
        let after_paul_style = after.get(paul_col, 0).style.expect("after paul style");

        assert_ne!(
            after_jessica_style.bgcolor, after_paul_style.bgcolor,
            "active/inactive tab backgrounds must differ after show_tab action"
        );
        assert_eq!(
            after_paul_style.bgcolor, before_jessica_style.bgcolor,
            "show_tab action should move active background from Jessica to Paul"
        );
    }

    #[test]
    fn tree_mode_show_tab_action_keeps_underline_animation_targets_valid() {
        let mut tabs = TabbedContent::new()
            .initial("jessica")
            .with_pane(TabPane::new("Leto", Label::new("first")).id("leto"))
            .with_pane(TabPane::new("Jessica", Label::new("second")).id("jessica"))
            .with_pane(TabPane::new("Paul", Label::new("third")).id("paul"));

        let mut tree = build_widget_tree_from_root(&mut tabs).expect("tree should exist");
        tabs.on_layout(60, 8);

        let console = Console::new();
        let before = render_tree_to_frame(&mut tree, &mut tabs, &console, 60, 8);
        let header = before.as_plain_lines()[0].clone();
        let jessica_col = find_label_column(&header, "Jessica");
        let paul_col = find_label_column(&header, "Paul");
        let before_active_underline_color = before
            .get(jessica_col, 1)
            .style
            .expect("before active underline style")
            .color;

        let parsed = ParsedAction {
            namespace: None,
            name: "show_tab".to_string(),
            arguments: vec!["paul".to_string()],
        };
        let mut ctx = EventCtx::default();
        assert!(tabs.execute_action(&parsed, &mut ctx));

        apply_runtime_class_messages(&mut tree, ctx.take_messages());
        let animation_requests = ctx.take_animation_requests();
        assert!(
            animation_requests
                .iter()
                .any(|req| req.attribute == "tabs.underline_start"),
            "show_tab should emit underline-start animation request"
        );
        assert!(
            animation_requests
                .iter()
                .any(|req| req.attribute == "tabs.underline_end"),
            "show_tab should emit underline-end animation request"
        );

        for request in animation_requests {
            let mut anim_ctx = EventCtx::default();
            tabs.on_event(
                &Event::AnimationValue(crate::event::AnimationValueEvent {
                    target: request.target,
                    attribute: request.attribute,
                    value: request.end,
                    done: true,
                }),
                &mut anim_ctx,
            );
        }

        let after = render_tree_to_frame(&mut tree, &mut tabs, &console, 60, 8);
        let after_jessica_underline_color = after
            .get(jessica_col, 1)
            .style
            .expect("after inactive underline style")
            .color;
        let after_paul_underline_color = after
            .get(paul_col, 1)
            .style
            .expect("after active underline style")
            .color;

        assert_ne!(
            after_jessica_underline_color, after_paul_underline_color,
            "active underline should move away from Jessica to Paul after show_tab"
        );
        assert_eq!(
            after_paul_underline_color, before_active_underline_color,
            "active underline color should transfer to Paul after show_tab"
        );
    }

    #[test]
    fn tree_mode_markdown_line_does_not_wrap_when_viewport_has_room() {
        let mut tabs = TabbedContent::new()
            .initial("jessica")
            .with_pane(TabPane::new("Leto", Label::new("first")).id("leto"))
            .with_pane(
                TabPane::new(
                    "Jessica",
                    Markdown::new(
                        "# Lady Jessica\n\nBene Gesserit and concubine of Leto, and mother of Paul and Alia.",
                    ),
                )
                .id("jessica"),
            )
            .with_pane(TabPane::new("Paul", Label::new("third")).id("paul"));

        let mut tree = build_widget_tree_from_root(&mut tabs).expect("tree should exist");
        let console = Console::new();
        let frame = render_tree_to_frame(&mut tree, &mut tabs, &console, 80, 10);
        let lines = frame.as_plain_lines();
        let sentence = "Bene Gesserit and concubine of Leto, and mother of Paul and Alia.";
        assert!(
            lines.iter().any(|line| line.contains(sentence)),
            "expected full sentence on one rendered line, got lines: {lines:?}"
        );
    }

    #[test]
    fn tree_mode_focused_tabs_apply_active_highlight_style() {
        let mut tabs = TabbedContent::new()
            .initial("jessica")
            .with_pane(TabPane::new("Leto", Label::new("first")).id("leto"))
            .with_pane(TabPane::new("Jessica", Label::new("second")).id("jessica"))
            .with_pane(TabPane::new("Paul", Label::new("third")).id("paul"));

        let mut tree = build_widget_tree_from_root(&mut tabs).expect("tree should exist");
        let tab_nodes = tree.query("Tabs").expect("Tabs selector should parse");
        let tabs_id = *tab_nodes.first().expect("expected top Tabs node");
        tree.get_mut(tabs_id)
            .expect("tabs node should exist")
            .widget
            .set_focus(true);

        let console = Console::new();
        let frame = render_tree_to_frame(&mut tree, &mut tabs, &console, 60, 8);
        let header = frame.as_plain_lines()[0].clone();
        let active_col = find_label_column(&header, "Jessica");
        let active_style = frame
            .get(active_col, 0)
            .style
            .expect("active tab style should exist");

        let expected_bg = crate::style::parse_color_like("$block-cursor-background")
            .expect("block-cursor bg")
            .to_simple_opaque();
        assert_eq!(
            active_style.bgcolor,
            Some(expected_bg),
            "focused Tabs should apply active tab background highlight"
        );
    }

    #[test]
    fn tab_activated_message_with_unknown_pane_id_is_not_handled() {
        let mut tabs = TabbedContent::new()
            .initial("jessica")
            .with_pane(TabPane::new("Leto", Label::new("first")).id("leto"))
            .with_pane(TabPane::new("Jessica", Label::new("second")).id("jessica"))
            .with_pane(TabPane::new("Paul", Label::new("third")).id("paul"));
        let active_before = tabs.active_id().map(str::to_string);

        let event = MessageEvent::new(
            crate::node_id::NodeId::default(),
            TabActivated {
                id: "--content-tab-child-two".to_string(),
                index: 1,
                title: "Alia".to_string(),
            },
        )
        .with_control(crate::node_id::NodeId::default());
        let mut ctx = EventCtx::default();
        tabs.on_message(&event, &mut ctx);

        assert!(
            !ctx.handled(),
            "unknown tab IDs must not be marked handled; otherwise parent tabbed-content can swallow nested tab activation"
        );
        assert_eq!(
            tabs.active_id().map(str::to_string),
            active_before,
            "unknown tab IDs must not mutate active pane"
        );
    }

    #[test]
    fn initial_id_is_applied_before_mount_in_tree_mode() {
        let tabs = TabbedContent::new()
            .initial("jessica")
            .with_pane(TabPane::new("Leto", Label::new("first")).id("leto"))
            .with_pane(TabPane::new("Jessica", Label::new("second")).id("jessica"));

        assert_eq!(
            tabs.active_id(),
            Some("jessica"),
            "initial pane should be active before on_mount so tree-only first render displays the correct pane"
        );
    }

    #[test]
    fn nested_tabbed_content_is_visible_on_first_tree_render() {
        let nested = TabbedContent::new()
            .with_pane(TabPane::new("Paul", Label::new("First child")))
            .with_pane(TabPane::new("Alia", Label::new("Second child")));

        let mut tabs = TabbedContent::new()
            .initial("jessica")
            .with_pane(TabPane::new("Leto", Label::new("first")).id("leto"))
            .with_pane(
                TabPane::new(
                    "Jessica",
                    Container::new()
                        .with_child(Markdown::new(
                            "# Lady Jessica\n\nBene Gesserit and concubine of Leto, and mother of Paul and Alia.",
                        ))
                        .with_child(nested),
                )
                .id("jessica"),
            )
            .with_pane(TabPane::new("Paul", Label::new("third")).id("paul"));

        let mut tree = build_widget_tree_from_root(&mut tabs).expect("tree should exist");
        let console = Console::new();
        let frame = render_tree_to_frame(&mut tree, &mut tabs, &console, 80, 16);
        let rendered = frame.as_plain_lines().join("\n");

        assert!(
            rendered.contains("First child"),
            "nested tab content should be visible on initial render without requiring a parent tab switch"
        );
    }
}
