use crate::style::Style;

#[derive(Debug, Clone, Default)]
pub struct StyleSelector {
    pub(super) type_name: Option<String>,
    pub(super) id: Option<String>,
    pub(super) classes: Vec<String>,
    pub(super) pseudos: Vec<PseudoClass>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PseudoClass {
    Disabled,
    Focus,
    Hover,
    Active,
}

impl StyleSelector {
    pub fn new(type_name: impl Into<String>) -> Self {
        Self {
            type_name: Some(type_name.into()),
            id: None,
            classes: Vec::new(),
            pseudos: Vec::new(),
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

    pub fn pseudo(mut self, pseudo: PseudoClass) -> Self {
        self.pseudos.push(pseudo);
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum Combinator {
    Descendant,
    Child,
}

#[derive(Debug, Clone)]
pub(super) struct SelectorChain {
    pub(super) parts: Vec<StyleSelector>,
    pub(super) combinators: Vec<Combinator>,
}

#[derive(Debug, Clone)]
pub struct StyleRule {
    pub(super) selector_chain: SelectorChain,
    pub(super) style: Style,
}

#[derive(Debug, Clone, Default)]
pub struct StyleSheet {
    pub(super) rules: Vec<StyleRule>,
}

impl StyleSheet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn extend(&mut self, other: &StyleSheet) {
        self.rules.extend(other.rules.iter().cloned());
    }

    pub fn add_rule(&mut self, selector: StyleSelector, style: Style) {
        self.rules.push(StyleRule {
            selector_chain: SelectorChain {
                parts: vec![selector],
                combinators: Vec::new(),
            },
            style,
        });
    }

    pub fn add_type(&mut self, name: impl Into<String>, style: Style) {
        self.add_rule(StyleSelector::new(name), style);
    }

    pub fn add_id(&mut self, id: impl Into<String>, style: Style) {
        self.add_rule(StyleSelector::default().id(id), style);
    }

    pub fn add_class(&mut self, class: impl Into<String>, style: Style) {
        self.add_rule(StyleSelector::default().class(class), style);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SelectorMeta {
    pub(super) type_name: String,
    pub(super) id: Option<String>,
    pub(super) classes: Vec<String>,
    pub(super) states: SelectorStates,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct SelectorStates {
    pub(super) disabled: bool,
    pub(super) focused: bool,
    pub(super) hovered: bool,
    pub(super) active: bool,
}
