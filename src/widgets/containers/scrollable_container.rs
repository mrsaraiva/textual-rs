use crate::compose::ComposeResult;
use crate::widgets::{BindingDecl, Container, Widget};
use textual_macros::widget;

use super::ScrollView;

// Delegation container: `#[widget(base = ScrollView, field = inner)]` forwards the
// full Widget surface to `inner`; the `override(..)` list below is supplied by the
// inherent methods. `style_type` keeps the trait default so this node reports its
// OWN CSS identity, "ScrollableContainer" — matching Python, where
// `ScrollableContainer(Widget)` matches `ScrollableContainer` (and the universal
// `Widget`) type selectors. The inner ScrollView is an implementation detail, NOT
// a CSS supertype (in Python the subclass relation is the reverse:
// `ScrollView(ScrollableContainer)`), so no `ScrollView` alias is carried.
#[widget(
    base = ScrollView,
    field = inner,
    override(
        compose,
        focusable,
        can_focus,
        can_focus_children,
        bindings,
        execute_action
    )
)]
pub struct ScrollableContainer {
    inner: ScrollView,
    can_focus: bool,
    can_focus_children: bool,
    can_maximize: Option<bool>,
}

impl ScrollableContainer {
    crate::delegate_ident_methods!(inner);
    crate::delegate_border_title_methods!(inner);

    pub fn new() -> Self {
        Self {
            inner: ScrollView::new(Container::new()),
            can_focus: true,
            can_focus_children: true,
            // Python default for ScrollableContainer.
            can_maximize: Some(false),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.inner = self.inner.with_child(child);
        self
    }

    pub fn with_compose(mut self, children: ComposeResult) -> Self {
        self.inner = self.inner.with_compose(children);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.inner.push(child);
    }

    pub fn height(mut self, height: usize) -> Self {
        self.inner = self.inner.height(height);
        self
    }

    pub fn scroll_step(mut self, step: usize) -> Self {
        self.inner = self.inner.scroll_step(step);
        self
    }

    pub fn scroll_step_x(mut self, step: usize) -> Self {
        self.inner = self.inner.scroll_step_x(step);
        self
    }

    pub fn set_scroll_step(&mut self, step: usize) {
        self.inner.set_scroll_step(step);
    }

    pub fn set_scroll_step_x(&mut self, step: usize) {
        self.inner.set_scroll_step_x(step);
    }

    pub fn scroll_by(&mut self, delta: i32) {
        self.inner.scroll_by(delta);
    }

    pub fn scroll_by_x(&mut self, delta: i32) {
        self.inner.scroll_by_x(delta);
    }

    pub fn set_virtual_content_size(&self, width: usize, height: usize) {
        self.inner.set_virtual_content_size(width, height);
    }

    pub fn scroll_to(&mut self, offset_y: usize) {
        self.inner.scroll_to(offset_y);
    }

    pub fn scroll_home(&mut self) {
        self.inner.scroll_home();
    }

    pub fn with_can_focus(mut self, can_focus: bool) -> Self {
        self.can_focus = can_focus;
        self
    }

    pub fn with_can_focus_children(mut self, can_focus_children: bool) -> Self {
        self.can_focus_children = can_focus_children;
        self
    }

    pub fn with_can_maximize(mut self, can_maximize: Option<bool>) -> Self {
        self.can_maximize = can_maximize;
        self
    }

    pub fn with_overflow_x(mut self, overflow: crate::style::Overflow) -> Self {
        self.inner = self.inner.with_overflow_x(overflow);
        self
    }

    pub fn with_overflow_y(mut self, overflow: crate::style::Overflow) -> Self {
        self.inner = self.inner.with_overflow_y(overflow);
        self
    }

    pub fn can_maximize(&self) -> bool {
        self.can_maximize.unwrap_or(self.can_focus)
    }

    // ── Widget-surface overrides (wired via #[widget(.., override(..))]) ──

    fn compose(&mut self) -> ComposeResult {
        // The inner ScrollView composes `[Container, vscrollbar, hscrollbar,
        // corner]`. ScrollableContainer flattens the single content Container out
        // of the tree: its user children are hoisted to become direct children of
        // the scroll host (so this widget IS the scroll viewport, Python parity),
        // while the dedicated scrollbar lanes pass through untouched.
        let inner_decls = self.inner.compose();
        let mut out: ComposeResult = Vec::new();
        let mut flattened_container = false;

        for mut decl in inner_decls {
            if !flattened_container {
                let ty = decl.widget().style_type();
                let is_scrollbar_lane = ty == "ScrollBar" || ty == "ScrollBarCorner";
                if !is_scrollbar_lane {
                    let any = decl.widget_mut() as &mut dyn std::any::Any;
                    if let Some(container) = any.downcast_mut::<Container>() {
                        out.extend(container.compose());
                        flattened_container = true;
                        continue;
                    }
                }
            }
            out.push(decl);
        }

        out
    }

    fn focusable(&self) -> bool {
        self.can_focus
    }

    fn can_focus(&self) -> bool {
        self.can_focus
    }

    fn can_focus_children(&self) -> bool {
        self.can_focus_children
    }

    fn bindings(&self) -> Vec<crate::widgets::BindingDecl> {
        let mut bindings = self.inner.bindings();
        bindings.push(BindingDecl::new("ctrl+pageup", "page_left", "Page left").hidden());
        bindings.push(BindingDecl::new("ctrl+pagedown", "page_right", "Page right").hidden());
        bindings
    }

    fn execute_action(&mut self, action: &crate::action::ParsedAction, ctx: &mut crate::event::WidgetCtx) -> bool {
        match action.name.as_str() {
            "page_left" => {
                let before = self.inner.offset_x();
                let page = self.inner.layout_height().unwrap_or(1).max(1);
                self.inner.scroll_by_x(-(page as i32));
                if self.inner.offset_x() != before {
                    ctx.request_repaint();
                }
                ctx.set_handled();
                true
            }
            "page_right" => {
                let before = self.inner.offset_x();
                let page = self.inner.layout_height().unwrap_or(1).max(1);
                self.inner.scroll_by_x(page as i32);
                if self.inner.offset_x() != before {
                    ctx.request_repaint();
                }
                ctx.set_handled();
                true
            }
            _ => self.inner.execute_action(action, ctx),
        }
    }
}

impl Default for ScrollableContainer {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::message::{MessageEvent, ScrollbarAxis, ScrollbarScrollTo};
    use crate::prelude::Label;

    #[test]
    fn scrollable_container_defaults_match_python_policies() {
        let sc = ScrollableContainer::new();
        assert!(sc.focusable());
        assert!(sc.can_focus_children());
        assert!(!sc.can_maximize());
    }

    // Python MRO parity: `ScrollableContainer(Widget)` matches the
    // `ScrollableContainer` type selector (its own name, NOT the inner
    // ScrollView's); `VerticalScroll(ScrollableContainer)` and
    // `HorizontalScroll(ScrollableContainer)` match BOTH their own name and
    // `ScrollableContainer` (via alias).
    #[test]
    fn scroll_family_css_identity_matches_python_mro() {
        let sc = ScrollableContainer::new();
        assert_eq!(sc.style_type(), "ScrollableContainer");
        assert!(sc.style_type_aliases().is_empty());

        let vs = crate::widgets::VerticalScroll::new();
        assert_eq!(vs.style_type(), "VerticalScroll");
        assert_eq!(vs.style_type_aliases(), ["ScrollableContainer"]);

        let hs = crate::widgets::HorizontalScroll::new();
        assert_eq!(hs.style_type(), "HorizontalScroll");
        assert_eq!(hs.style_type_aliases(), ["ScrollableContainer"]);
    }

    // Python `containers.py` DEFAULT_CSS parity: the whole scroll family
    // resolves `width: 1fr; height: 1fr` (VerticalScroll/HorizontalScroll
    // inherit it from the ScrollableContainer rule via the alias), while each
    // subclass's own later rule overrides the overflow axes.
    #[test]
    fn scroll_family_defaults_resolve_width_height_1fr() {
        use crate::style::{Overflow, Scalar};
        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let sc = ScrollableContainer::new();
        let style = crate::css::resolve_style_for_meta(&crate::css::selector_meta_generic(&sc));
        assert_eq!(style.width, Some(Scalar::Fraction(1.0)));
        assert_eq!(style.height, Some(Scalar::Fraction(1.0)));
        assert_eq!(style.overflow_x, Some(Overflow::Auto));
        assert_eq!(style.overflow_y, Some(Overflow::Auto));

        let vs = crate::widgets::VerticalScroll::new();
        let style = crate::css::resolve_style_for_meta(&crate::css::selector_meta_generic(&vs));
        assert_eq!(style.width, Some(Scalar::Fraction(1.0)));
        assert_eq!(style.height, Some(Scalar::Fraction(1.0)));
        assert_eq!(style.overflow_x, Some(Overflow::Hidden));
        assert_eq!(style.overflow_y, Some(Overflow::Auto));

        let hs = crate::widgets::HorizontalScroll::new();
        let style = crate::css::resolve_style_for_meta(&crate::css::selector_meta_generic(&hs));
        assert_eq!(style.width, Some(Scalar::Fraction(1.0)));
        assert_eq!(style.height, Some(Scalar::Fraction(1.0)));
        assert_eq!(style.overflow_x, Some(Overflow::Auto));
        assert_eq!(style.overflow_y, Some(Overflow::Hidden));
    }

    #[test]
    fn scrollable_container_forwards_scroll_offset() {
        let mut sc = ScrollableContainer::new().with_child(Label::new("a"));
        let _ = sc.compose();
        assert_eq!(sc.scroll_offset(), (0, 0));
        assert!(sc.clips_descendants_to_content());
    }

    #[test]
    fn scrollable_container_forwards_scrollbar_messages_to_inner_scrollview() {
        let mut sc = ScrollableContainer::new().with_child(Label::new("line\n".repeat(20)));
        sc.set_virtual_content_size(20, 100);
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            sc.on_message(
            &MessageEvent::new(
                crate::node_id::NodeId::default(),
                ScrollbarScrollTo {
                    axis: ScrollbarAxis::Vertical,
                    offset: 6.0,
                    animate: false,
                    scroll_duration: None,
                },
            ),
            &mut __w);
        }
        assert_eq!(sc.scroll_offset().1, 6);
        assert!(
            ctx.handled(),
            "message should be handled by inner ScrollView"
        );
    }
}
