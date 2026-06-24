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
}
