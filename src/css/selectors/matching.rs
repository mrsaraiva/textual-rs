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
                    super::ast::PseudoClass::Hover => meta.states.hovered,
                    super::ast::PseudoClass::Active => meta.states.active,
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
