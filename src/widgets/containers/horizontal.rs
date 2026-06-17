use crate::compose::ComposeResult;
use crate::widgets::Widget;

use super::Container;
use crate::widgets::delegate::delegate_widget_to;

pub struct Horizontal {
    inner: Container,
}

impl Horizontal {
    crate::delegate_ident_methods!(inner);

    pub fn new() -> Self {
        Self {
            inner: Container::new(),
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
}

delegate_widget_to!(Horizontal, inner);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::containers::{Center, CenterMiddle, ItemGrid, Middle, Right};

    #[test]
    fn horizontal_id_and_class_are_carried_to_inner_seed() {
        let mut h = Horizontal::new().id("row1").class("buttons");
        let seed = h.take_node_seed();
        assert_eq!(seed.css_id.as_deref(), Some("row1"));
        assert!(seed.classes.iter().any(|c| c == "buttons"));
    }

    #[test]
    fn center_id_and_class_are_carried_to_inner_seed() {
        let mut c = Center::new().id("ctr").class("centered");
        let seed = c.take_node_seed();
        assert_eq!(seed.css_id.as_deref(), Some("ctr"));
        assert!(seed.classes.iter().any(|c| c == "centered"));
    }

    #[test]
    fn middle_id_and_class_are_carried_to_inner_seed() {
        let mut m = Middle::new().id("mid").class("middle-wrap");
        let seed = m.take_node_seed();
        assert_eq!(seed.css_id.as_deref(), Some("mid"));
        assert!(seed.classes.iter().any(|c| c == "middle-wrap"));
    }

    #[test]
    fn center_middle_id_and_class_are_carried_to_inner_seed() {
        let mut cm = CenterMiddle::new().id("cm1").class("center-mid");
        let seed = cm.take_node_seed();
        assert_eq!(seed.css_id.as_deref(), Some("cm1"));
        assert!(seed.classes.iter().any(|c| c == "center-mid"));
    }

    #[test]
    fn right_id_and_class_are_carried_to_inner_seed() {
        let mut r = Right::new().id("rgt").class("right-align");
        let seed = r.take_node_seed();
        assert_eq!(seed.css_id.as_deref(), Some("rgt"));
        assert!(seed.classes.iter().any(|c| c == "right-align"));
    }

    #[test]
    fn item_grid_id_and_class_are_carried_to_inner_seed() {
        let mut ig = ItemGrid::new().id("grid1").class("item-grid");
        let seed = ig.take_node_seed();
        assert_eq!(seed.css_id.as_deref(), Some("grid1"));
        assert!(seed.classes.iter().any(|c| c == "item-grid"));
    }
}
