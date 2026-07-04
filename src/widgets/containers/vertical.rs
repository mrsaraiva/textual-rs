use crate::compose::ComposeResult;
use crate::widgets::Widget;

use super::Container;
use crate::widgets::delegate::delegate_widget_to;

pub struct Vertical {
    container: Container,
}

impl Default for Vertical {
    fn default() -> Self {
        Self::new()
    }
}

impl Vertical {
    crate::delegate_ident_methods!(container);
    crate::delegate_border_title_methods!(container);

    pub fn new() -> Self {
        Self {
            container: Container::new(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.container = self.container.with_child(child);
        self
    }

    pub fn with_compose(mut self, children: ComposeResult) -> Self {
        self.container = self.container.with_compose(children);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.container.push(child);
    }
}

delegate_widget_to!(Vertical, container);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::containers::{HorizontalGroup, VerticalGroup};

    #[test]
    fn vertical_id_and_class_are_carried_to_inner_seed() {
        let mut v = Vertical::new().id("col").class("column");
        let seed = v.take_node_seed();
        assert_eq!(seed.css_id.as_deref(), Some("col"));
        assert!(seed.classes.iter().any(|c| c == "column"));
    }

    #[test]
    fn horizontal_group_id_and_class_are_carried_to_inner_seed() {
        let mut hg = HorizontalGroup::new().id("hgrp").class("h-group");
        let seed = hg.take_node_seed();
        assert_eq!(seed.css_id.as_deref(), Some("hgrp"));
        assert!(seed.classes.iter().any(|c| c == "h-group"));
    }

    #[test]
    fn vertical_group_id_and_class_are_carried_to_inner_seed() {
        let mut vg = VerticalGroup::new().id("vgrp").class("v-group");
        let seed = vg.take_node_seed();
        assert_eq!(seed.css_id.as_deref(), Some("vgrp"));
        assert!(seed.classes.iter().any(|c| c == "v-group"));
    }

    #[test]
    fn classes_plural_are_carried_to_inner_seed() {
        // `.classes()` (plural) is uniform across the seed macro (base Container)
        // and the delegate macro (wrapper groups).
        let mut c = super::Container::new().classes(["a", "b"]);
        let seed = c.take_node_seed();
        assert!(seed.classes.iter().any(|x| x == "a"));
        assert!(seed.classes.iter().any(|x| x == "b"));

        let mut vg = VerticalGroup::new().classes(["x", "y"]);
        let seed = vg.take_node_seed();
        assert!(seed.classes.iter().any(|x| x == "x"));
        assert!(seed.classes.iter().any(|x| x == "y"));
    }

    #[test]
    fn border_title_round_trips_through_wrapper_family() {
        use crate::widgets::containers::VerticalScroll;
        use crate::widgets::{Container, Widget};

        // Base container.
        let c = Container::new().with_border_title("ct").with_border_subtitle("cs");
        assert_eq!(Widget::border_title(&c), Some("ct"));
        assert_eq!(Widget::border_subtitle(&c), Some("cs"));

        // Non-scroll wrapper delegating to Container.
        let v = Vertical::new().with_border_title("vt").with_border_subtitle("vs");
        assert_eq!(Widget::border_title(&v), Some("vt"));
        assert_eq!(Widget::border_subtitle(&v), Some("vs"));

        // `#[widget(base = Vertical)]` derive wrapper.
        let vg = VerticalGroup::new().with_border_title("gt");
        assert_eq!(Widget::border_title(&vg), Some("gt"));

        // Scroll family: reader chain VerticalScroll -> ScrollableContainer ->
        // ScrollView must surface the border title.
        let vsc = VerticalScroll::new().with_border_title("st").with_border_subtitle("ss");
        assert_eq!(Widget::border_title(&vsc), Some("st"));
        assert_eq!(Widget::border_subtitle(&vsc), Some("ss"));
    }
}
