use super::ast::{Combinator, SelectorMeta, StyleRule, StyleSelector};
use super::context::SELECTOR_STACK;

impl StyleSelector {
    pub(crate) fn matches(&self, meta: &SelectorMeta) -> bool {
        if let Some(type_name) = &self.type_name {
            if meta.type_name != *type_name {
                return false;
            }
        }
        if let Some(id) = &self.id {
            if meta.id.as_deref() != Some(id.as_str()) {
                return false;
            }
        }
        if !self.classes.is_empty() {
            if !self
                .classes
                .iter()
                .all(|class| meta.classes.iter().any(|value| value == class))
            {
                return false;
            }
        }
        if !self.pseudos.is_empty() {
            for pseudo in &self.pseudos {
                let ok = match pseudo {
                    super::ast::PseudoClass::Disabled => meta.states.disabled,
                    super::ast::PseudoClass::Focus => meta.states.focused,
                    super::ast::PseudoClass::FocusWithin => meta.states.focus_within,
                    super::ast::PseudoClass::Hover => meta.states.hovered,
                    super::ast::PseudoClass::Active => meta.states.active,
                    super::ast::PseudoClass::Dark => meta.states.dark,
                    super::ast::PseudoClass::Light => !meta.states.dark,
                    super::ast::PseudoClass::Even => {
                        meta.states.child_index.map_or(false, |i| i % 2 == 0)
                    }
                    super::ast::PseudoClass::Odd => {
                        meta.states.child_index.map_or(false, |i| i % 2 == 1)
                    }
                    super::ast::PseudoClass::FirstChild => meta.states.child_index == Some(0),
                    super::ast::PseudoClass::LastChild => {
                        match (meta.states.child_index, meta.states.sibling_count) {
                            (Some(idx), Some(count)) if count > 0 => idx == count - 1,
                            _ => false,
                        }
                    }
                };
                if !ok {
                    return false;
                }
            }
        }
        true
    }

    pub(super) fn specificity(&self) -> u8 {
        let mut score = 0u8;
        if self.type_name.is_some() {
            score += 1;
        }
        score = score.saturating_add(self.classes.len().saturating_mul(10) as u8);
        score = score.saturating_add(self.pseudos.len().saturating_mul(10) as u8);
        if self.id.is_some() {
            score = score.saturating_add(100);
        }
        score
    }
}

pub(super) fn rule_specificity(rule: &StyleRule, meta: &SelectorMeta) -> Option<u8> {
    if rule.selector_chain.parts.is_empty() {
        return None;
    }
    let last = rule.selector_chain.parts.last().unwrap();
    if !last.matches(meta) {
        return None;
    }
    if rule.selector_chain.parts.len() == 1 {
        return Some(last.specificity());
    }

    let stack_snapshot = SELECTOR_STACK.with(|stack| stack.borrow().clone());
    // Selector stack contains ancestors only (the current widget meta is not pushed until after
    // style resolution). For child combinators we need to start matching from the immediate parent.
    let mut idx = stack_snapshot.len() as isize - 1;
    if idx < 0 {
        return None;
    }
    let combinators = &rule.selector_chain.combinators;
    let parts = &rule.selector_chain.parts;
    for (part_index, selector) in parts[..parts.len() - 1].iter().rev().enumerate() {
        let comb = combinators[combinators.len() - 1 - part_index];
        match comb {
            Combinator::Child => {
                let meta = &stack_snapshot[idx as usize];
                if !selector.matches(meta) {
                    return None;
                }
                idx -= 1;
            }
            Combinator::Descendant => {
                let mut found = false;
                let mut current = idx;
                while current >= 0 {
                    let meta = &stack_snapshot[current as usize];
                    if selector.matches(meta) {
                        found = true;
                        idx = current - 1;
                        break;
                    }
                    current -= 1;
                }
                if !found {
                    return None;
                }
            }
        }
    }

    let score = parts.iter().map(|part| part.specificity()).sum();
    Some(score)
}

#[cfg(test)]
mod tests {
    use super::super::ast::{PseudoClass, SelectorMeta, SelectorStates, StyleSelector};

    fn meta_with_states(states: SelectorStates) -> SelectorMeta {
        SelectorMeta {
            type_name: "Widget".to_string(),
            id: None,
            classes: Vec::new(),
            states,
        }
    }

    #[test]
    fn dark_matches_when_dark_true() {
        let selector = StyleSelector::new("Widget").pseudo(PseudoClass::Dark);
        let meta = meta_with_states(SelectorStates {
            dark: true,
            ..Default::default()
        });
        assert!(selector.matches(&meta));
    }

    #[test]
    fn dark_does_not_match_when_dark_false() {
        let selector = StyleSelector::new("Widget").pseudo(PseudoClass::Dark);
        let meta = meta_with_states(SelectorStates::default());
        assert!(!selector.matches(&meta));
    }

    #[test]
    fn light_matches_when_dark_false() {
        let selector = StyleSelector::new("Widget").pseudo(PseudoClass::Light);
        let meta = meta_with_states(SelectorStates::default());
        assert!(selector.matches(&meta));
    }

    #[test]
    fn light_does_not_match_when_dark_true() {
        let selector = StyleSelector::new("Widget").pseudo(PseudoClass::Light);
        let meta = meta_with_states(SelectorStates {
            dark: true,
            ..Default::default()
        });
        assert!(!selector.matches(&meta));
    }

    #[test]
    fn even_matches_indices_0_2_4() {
        let selector = StyleSelector::new("Widget").pseudo(PseudoClass::Even);
        for idx in [0, 2, 4] {
            let meta = meta_with_states(SelectorStates {
                child_index: Some(idx),
                sibling_count: Some(5),
                ..Default::default()
            });
            assert!(selector.matches(&meta), "should match child_index={idx}");
        }
    }

    #[test]
    fn even_does_not_match_odd_indices() {
        let selector = StyleSelector::new("Widget").pseudo(PseudoClass::Even);
        for idx in [1, 3, 5] {
            let meta = meta_with_states(SelectorStates {
                child_index: Some(idx),
                sibling_count: Some(6),
                ..Default::default()
            });
            assert!(
                !selector.matches(&meta),
                "should not match child_index={idx}"
            );
        }
    }

    #[test]
    fn even_does_not_match_without_index() {
        let selector = StyleSelector::new("Widget").pseudo(PseudoClass::Even);
        let meta = meta_with_states(SelectorStates::default());
        assert!(!selector.matches(&meta));
    }

    #[test]
    fn odd_matches_indices_1_3_5() {
        let selector = StyleSelector::new("Widget").pseudo(PseudoClass::Odd);
        for idx in [1, 3, 5] {
            let meta = meta_with_states(SelectorStates {
                child_index: Some(idx),
                sibling_count: Some(6),
                ..Default::default()
            });
            assert!(selector.matches(&meta), "should match child_index={idx}");
        }
    }

    #[test]
    fn odd_does_not_match_even_indices() {
        let selector = StyleSelector::new("Widget").pseudo(PseudoClass::Odd);
        for idx in [0, 2, 4] {
            let meta = meta_with_states(SelectorStates {
                child_index: Some(idx),
                sibling_count: Some(5),
                ..Default::default()
            });
            assert!(
                !selector.matches(&meta),
                "should not match child_index={idx}"
            );
        }
    }

    #[test]
    fn first_child_matches_index_0_only() {
        let selector = StyleSelector::new("Widget").pseudo(PseudoClass::FirstChild);
        let meta_first = meta_with_states(SelectorStates {
            child_index: Some(0),
            sibling_count: Some(3),
            ..Default::default()
        });
        assert!(selector.matches(&meta_first));

        let meta_second = meta_with_states(SelectorStates {
            child_index: Some(1),
            sibling_count: Some(3),
            ..Default::default()
        });
        assert!(!selector.matches(&meta_second));
    }

    #[test]
    fn first_child_does_not_match_without_index() {
        let selector = StyleSelector::new("Widget").pseudo(PseudoClass::FirstChild);
        let meta = meta_with_states(SelectorStates::default());
        assert!(!selector.matches(&meta));
    }

    #[test]
    fn last_child_matches_last_index() {
        let selector = StyleSelector::new("Widget").pseudo(PseudoClass::LastChild);
        let meta = meta_with_states(SelectorStates {
            child_index: Some(4),
            sibling_count: Some(5),
            ..Default::default()
        });
        assert!(selector.matches(&meta));
    }

    #[test]
    fn last_child_does_not_match_non_last() {
        let selector = StyleSelector::new("Widget").pseudo(PseudoClass::LastChild);
        let meta = meta_with_states(SelectorStates {
            child_index: Some(2),
            sibling_count: Some(5),
            ..Default::default()
        });
        assert!(!selector.matches(&meta));
    }

    #[test]
    fn last_child_does_not_match_without_count() {
        let selector = StyleSelector::new("Widget").pseudo(PseudoClass::LastChild);
        let meta = meta_with_states(SelectorStates {
            child_index: Some(0),
            ..Default::default()
        });
        assert!(!selector.matches(&meta));
    }

    #[test]
    fn combined_hover_dark() {
        let selector = StyleSelector::new("Widget")
            .pseudo(PseudoClass::Hover)
            .pseudo(PseudoClass::Dark);
        let both = meta_with_states(SelectorStates {
            hovered: true,
            dark: true,
            ..Default::default()
        });
        assert!(selector.matches(&both));

        let hover_only = meta_with_states(SelectorStates {
            hovered: true,
            ..Default::default()
        });
        assert!(!selector.matches(&hover_only));

        let dark_only = meta_with_states(SelectorStates {
            dark: true,
            ..Default::default()
        });
        assert!(!selector.matches(&dark_only));
    }

    #[test]
    fn specificity_new_pseudos_same_weight_as_existing() {
        let existing = StyleSelector::new("Widget").pseudo(PseudoClass::Hover);
        let dark = StyleSelector::new("Widget").pseudo(PseudoClass::Dark);
        let even = StyleSelector::new("Widget").pseudo(PseudoClass::Even);
        let first = StyleSelector::new("Widget").pseudo(PseudoClass::FirstChild);
        let last = StyleSelector::new("Widget").pseudo(PseudoClass::LastChild);

        let base = existing.specificity();
        assert_eq!(dark.specificity(), base);
        assert_eq!(even.specificity(), base);
        assert_eq!(first.specificity(), base);
        assert_eq!(last.specificity(), base);

        // Two pseudos should have double the pseudo weight
        let two = StyleSelector::new("Widget")
            .pseudo(PseudoClass::Dark)
            .pseudo(PseudoClass::Even);
        assert_eq!(two.specificity(), base + 10);
    }

    #[test]
    fn single_child_is_both_first_and_last() {
        let first = StyleSelector::new("Widget").pseudo(PseudoClass::FirstChild);
        let last = StyleSelector::new("Widget").pseudo(PseudoClass::LastChild);
        let meta = meta_with_states(SelectorStates {
            child_index: Some(0),
            sibling_count: Some(1),
            ..Default::default()
        });
        assert!(first.matches(&meta));
        assert!(last.matches(&meta));
    }

    // -- :focus-within -------------------------------------------------------

    #[test]
    fn focus_within_matches_when_element_itself_has_focus() {
        let selector = StyleSelector::new("Widget").pseudo(PseudoClass::FocusWithin);
        let meta = meta_with_states(SelectorStates {
            focused: true,
            focus_within: true,
            ..Default::default()
        });
        assert!(selector.matches(&meta));
    }

    #[test]
    fn focus_within_matches_when_descendant_has_focus() {
        let selector = StyleSelector::new("Widget").pseudo(PseudoClass::FocusWithin);
        // The element itself doesn't have focus, but a descendant does.
        let meta = meta_with_states(SelectorStates {
            focused: false,
            focus_within: true,
            ..Default::default()
        });
        assert!(selector.matches(&meta));
    }

    #[test]
    fn focus_within_does_not_match_when_nothing_focused() {
        let selector = StyleSelector::new("Widget").pseudo(PseudoClass::FocusWithin);
        let meta = meta_with_states(SelectorStates::default());
        assert!(!selector.matches(&meta));
    }

    #[test]
    fn focus_within_does_not_match_unrelated_focus() {
        let selector = StyleSelector::new("Widget").pseudo(PseudoClass::FocusWithin);
        // Neither focused nor focus_within — unrelated node has focus.
        let meta = meta_with_states(SelectorStates {
            focused: false,
            focus_within: false,
            ..Default::default()
        });
        assert!(!selector.matches(&meta));
    }

    #[test]
    fn focus_within_specificity_same_as_other_pseudos() {
        let focus_within = StyleSelector::new("Widget").pseudo(PseudoClass::FocusWithin);
        let hover = StyleSelector::new("Widget").pseudo(PseudoClass::Hover);
        assert_eq!(focus_within.specificity(), hover.specificity());
    }
}
