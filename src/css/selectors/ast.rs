use crate::style::Style;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    FocusWithin,
    Hover,
    Active,
    Dark,
    Light,
    Even,
    Odd,
    FirstChild,
    LastChild,
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

    pub(crate) fn type_name(&self) -> Option<&str> {
        self.type_name.as_deref()
    }

    pub(crate) fn id_name(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub(crate) fn classes(&self) -> &[String] {
        &self.classes
    }

    pub(crate) fn pseudos(&self) -> &[PseudoClass] {
        &self.pseudos
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Combinator {
    Descendant,
    Child,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectorChain {
    pub(crate) parts: Vec<StyleSelector>,
    pub(crate) combinators: Vec<Combinator>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StyleRule {
    pub(super) selector_chain: SelectorChain,
    pub(super) style: Style,
}

#[derive(Debug, Clone, Default, PartialEq)]
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

    pub(crate) fn rules(&self) -> &[StyleRule] {
        &self.rules
    }
}

impl StyleRule {
    pub(crate) fn selector_chain(&self) -> &SelectorChain {
        &self.selector_chain
    }

    pub(crate) fn style(&self) -> Style {
        self.style.clone()
    }
}

impl SelectorChain {
    pub(crate) fn parts(&self) -> &[StyleSelector] {
        &self.parts
    }

    pub(crate) fn combinators(&self) -> &[Combinator] {
        &self.combinators
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectorMeta {
    pub(super) type_name: String,
    pub(super) id: Option<String>,
    pub(super) classes: Vec<String>,
    pub(super) states: SelectorStates,
}

impl SelectorMeta {
    /// Create a `SelectorMeta` with default (inactive) pseudo-class states.
    ///
    /// Used by `WidgetTree::query*` to build lightweight match targets without
    /// requiring the full render-time style stack.
    pub(crate) fn new(type_name: String, id: Option<String>, classes: Vec<String>) -> Self {
        Self {
            type_name,
            id,
            classes,
            states: SelectorStates::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct SelectorStates {
    pub(super) disabled: bool,
    pub(super) focused: bool,
    pub(super) focus_within: bool,
    pub(super) hovered: bool,
    pub(super) active: bool,
    pub(super) dark: bool,
    pub(super) child_index: Option<usize>,
    pub(super) sibling_count: Option<usize>,
}
