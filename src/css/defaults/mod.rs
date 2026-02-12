// Minimal built-in widget defaults to help demos look like Textual (Python) without requiring
// demo-specific CSS for core widget visuals.
//
// Note: this is a pragmatic subset of Textual's built-in widget CSS. We intentionally avoid
// full TCSS features (nesting, `&`, `!important`, advanced opacity) until the style engine grows.
//
// Each submodule exports a `DEFAULT_CSS` constant with the CSS fragment for its widget(s).

mod base;
mod button;
mod checkbox;
mod collapsible;
mod data_table;
mod header_footer;
mod input;
mod list_view;
mod misc;
mod select;
mod tabs;
mod text_area;
mod tree;

use super::StyleSheet;

pub fn default_widget_stylesheet() -> StyleSheet {
    let combined = [
        base::DEFAULT_CSS,
        misc::DEFAULT_CSS,
        header_footer::DEFAULT_CSS,
        text_area::DEFAULT_CSS,
        input::DEFAULT_CSS,
        checkbox::DEFAULT_CSS,
        collapsible::DEFAULT_CSS,
        select::DEFAULT_CSS,
        list_view::DEFAULT_CSS,
        tree::DEFAULT_CSS,
        tabs::DEFAULT_CSS,
        button::DEFAULT_CSS,
        data_table::DEFAULT_CSS,
    ]
    .join("\n");
    StyleSheet::parse(&combined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Display, Dock, Overflow, Scalar};

    /// Helper: find the first rule whose primary selector matches the given type name
    /// (with no classes / no pseudo-classes).
    fn find_type_style(sheet: &StyleSheet, type_name: &str) -> crate::style::Style {
        for rule in sheet.rules() {
            let parts = rule.selector_chain().parts();
            if parts.len() == 1 {
                let sel = &parts[0];
                if sel.type_name() == Some(type_name)
                    && sel.classes().is_empty()
                    && sel.pseudos().is_empty()
                {
                    return rule.style();
                }
            }
        }
        panic!("no rule found for type `{type_name}`");
    }

    /// Helper: find the first rule whose primary selector matches the given type name
    /// and has the given class.
    fn find_type_class_style(
        sheet: &StyleSheet,
        type_name: &str,
        class: &str,
    ) -> crate::style::Style {
        for rule in sheet.rules() {
            let parts = rule.selector_chain().parts();
            if parts.len() == 1 {
                let sel = &parts[0];
                if sel.type_name() == Some(type_name) && sel.classes().iter().any(|c| c == class) {
                    return rule.style();
                }
            }
        }
        panic!("no rule found for type `{type_name}` with class `{class}`");
    }

    /// Helper: find a child selector rule (Parent > Child) where only the child part
    /// has the expected type name.
    fn find_child_rule_style(
        sheet: &StyleSheet,
        parent_type: &str,
        parent_class: &str,
        child_type: &str,
    ) -> crate::style::Style {
        for rule in sheet.rules() {
            let parts = rule.selector_chain().parts();
            if parts.len() == 2 {
                let parent = &parts[0];
                let child = &parts[1];
                if parent.type_name() == Some(parent_type)
                    && parent.classes().iter().any(|c| c == parent_class)
                    && child.type_name() == Some(child_type)
                {
                    return rule.style();
                }
            }
        }
        panic!(
            "no child rule found for `{parent_type}.{parent_class} > {child_type}`"
        );
    }

    #[test]
    fn all_default_fragments_parse_without_panic() {
        // Parsing the combined stylesheet should not panic and should produce rules.
        let sheet = default_widget_stylesheet();
        assert!(
            !sheet.rules().is_empty(),
            "default stylesheet should have rules"
        );
    }

    // WP-07: Header dock: top
    #[test]
    fn header_has_dock_top() {
        let sheet = StyleSheet::parse(header_footer::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Header");
        assert_eq!(style.dock, Some(Dock::Top));
    }

    // WP-07: Header width: 100%
    #[test]
    fn header_has_width_100_percent() {
        let sheet = StyleSheet::parse(header_footer::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Header");
        assert_eq!(style.width, Some(Scalar::Percent(100.0)));
    }

    // WP-08: Footer dock: bottom
    #[test]
    fn footer_has_dock_bottom() {
        let sheet = StyleSheet::parse(header_footer::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Footer");
        assert_eq!(style.dock, Some(Dock::Bottom));
    }

    // WP-13: Input width: 100%
    #[test]
    fn input_has_width_100_percent() {
        let sheet = StyleSheet::parse(input::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Input");
        assert_eq!(style.width, Some(Scalar::Percent(100.0)));
    }

    // WP-13: MaskedInput width: 100%
    #[test]
    fn masked_input_has_width_100_percent() {
        let sheet = StyleSheet::parse(input::DEFAULT_CSS);
        let style = find_type_style(&sheet, "MaskedInput");
        assert_eq!(style.width, Some(Scalar::Percent(100.0)));
    }

    // WP-12: Placeholder overflow: hidden
    #[test]
    fn placeholder_has_overflow_hidden() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Placeholder");
        assert_eq!(style.overflow, Some(Overflow::Hidden));
    }

    // WP-14: Collapsible.-collapsed > Contents display: none
    #[test]
    fn collapsible_collapsed_contents_display_none() {
        let sheet = StyleSheet::parse(collapsible::DEFAULT_CSS);
        let style = find_child_rule_style(&sheet, "Collapsible", "-collapsed", "Contents");
        assert_eq!(style.display, Some(Display::None));
    }

    // WP-30: Rule.-horizontal has width 1fr and margin
    #[test]
    fn rule_horizontal_has_width_1fr_and_margin() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_class_style(&sheet, "Rule", "-horizontal");
        assert_eq!(style.width, Some(Scalar::Fraction(1.0)));
        assert_eq!(style.height, Some(Scalar::Cells(1)));
        // margin: 1 0 means top=1, right=0, bottom=1, left=0
        assert!(style.margin.is_some(), "Rule.-horizontal should have margin");
    }

    // WP-30: Rule.-vertical has height 1fr and margin
    #[test]
    fn rule_vertical_has_height_1fr_and_margin() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_class_style(&sheet, "Rule", "-vertical");
        assert_eq!(style.height, Some(Scalar::Fraction(1.0)));
        assert_eq!(style.width, Some(Scalar::Cells(1)));
        assert!(style.margin.is_some(), "Rule.-vertical should have margin");
    }

    // WP-31: TextArea width/height 1fr + padding
    #[test]
    fn text_area_has_1fr_dimensions_and_padding() {
        let sheet = StyleSheet::parse(text_area::DEFAULT_CSS);
        let style = find_type_style(&sheet, "TextArea");
        assert_eq!(style.width, Some(Scalar::Fraction(1.0)));
        assert_eq!(style.height, Some(Scalar::Fraction(1.0)));
        assert!(style.padding.is_some(), "TextArea should have padding");
    }

    // WP-11/12: content-align and text-align are present in the CSS text
    // but NOT yet parsed by the CSS engine (parser support needed).
    // These tests verify the CSS fragments still parse without error.
    #[test]
    fn button_css_fragment_parses_without_error() {
        let sheet = StyleSheet::parse(button::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Button");
        // content-align and text-align are not parsed yet, so they'll be None.
        // Just verify the fragment parsed and other properties are intact.
        assert_eq!(style.width, Some(Scalar::Auto));
        assert_eq!(style.min_width, Some(Scalar::Cells(16)));
    }

    #[test]
    fn placeholder_css_fragment_parses_without_error() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Placeholder");
        // content-align not parsed yet; overflow should be there
        assert_eq!(style.overflow, Some(Overflow::Hidden));
    }
}
