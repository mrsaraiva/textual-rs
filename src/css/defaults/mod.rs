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
mod containers;
mod data_table;
mod header_footer;
mod input;
mod list_view;
mod misc;
mod select;
mod tabs;
mod text_area;
mod tooltip;
mod tree;

use super::StyleSheet;

pub fn default_widget_stylesheet() -> StyleSheet {
    let combined = [
        base::DEFAULT_CSS,
        containers::DEFAULT_CSS,
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
        tooltip::DEFAULT_CSS,
    ]
    .join("\n");
    StyleSheet::parse(&combined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{
        Constrain, Display, Dock, HorizontalAlign, Layout, Overflow, Pointer, Scalar, Spacing,
        TextAlign, VerticalAlign,
    };

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
        panic!("no child rule found for `{parent_type}.{parent_class} > {child_type}`");
    }

    #[test]
    fn all_default_fragments_parse_without_panic() {
        let sheet = default_widget_stylesheet();
        assert!(
            !sheet.rules().is_empty(),
            "default stylesheet should have rules"
        );
    }

    // ---- base.rs: Screen + ScrollView ----

    #[test]
    fn screen_has_layout_vertical() {
        let sheet = StyleSheet::parse(base::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Screen");
        assert_eq!(style.layout, Some(Layout::Vertical));
    }

    #[test]
    fn screen_has_overflow_y_auto() {
        let sheet = StyleSheet::parse(base::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Screen");
        assert_eq!(style.overflow_y, Some(Overflow::Auto));
    }

    #[test]
    fn scrollview_has_overflow_y_and_x_auto() {
        let sheet = StyleSheet::parse(base::DEFAULT_CSS);
        let style = find_type_style(&sheet, "ScrollView");
        assert_eq!(style.overflow_y, Some(Overflow::Auto));
        assert_eq!(style.overflow_x, Some(Overflow::Auto));
    }

    // ---- button.rs: content-align + text-align now parsed ----

    #[test]
    fn button_has_content_align_center_middle() {
        let sheet = StyleSheet::parse(button::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Button");
        let ca = style
            .content_align
            .expect("Button should have content-align");
        assert_eq!(ca.horizontal, HorizontalAlign::Center);
        assert_eq!(ca.vertical, VerticalAlign::Middle);
    }

    #[test]
    fn button_has_text_align_center() {
        let sheet = StyleSheet::parse(button::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Button");
        assert_eq!(style.text_align, Some(TextAlign::Center));
    }

    #[test]
    fn button_has_width_auto_and_min_width() {
        let sheet = StyleSheet::parse(button::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Button");
        assert_eq!(style.width, Some(Scalar::Auto));
        assert_eq!(style.min_width, Some(Scalar::Cells(16)));
    }

    // ---- collapsible.rs: padding shorthand fix ----

    #[test]
    fn collapsible_has_padding_bottom_and_left() {
        let sheet = StyleSheet::parse(collapsible::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Collapsible");
        assert_eq!(style.padding_bottom, Some(1));
        assert_eq!(style.padding_left, Some(1));
    }

    #[test]
    fn collapsible_collapsed_contents_display_none() {
        let sheet = StyleSheet::parse(collapsible::DEFAULT_CSS);
        let style = find_child_rule_style(&sheet, "Collapsible", "-collapsed", "Contents");
        assert_eq!(style.display, Some(Display::None));
    }

    // ---- data_table.rs: max-height ----

    #[test]
    fn data_table_has_max_height_100_percent() {
        let sheet = StyleSheet::parse(data_table::DEFAULT_CSS);
        let style = find_type_style(&sheet, "DataTable");
        assert_eq!(style.max_height, Some(Scalar::Percent(100.0)));
    }

    // ---- header_footer.rs ----

    #[test]
    fn header_has_dock_top() {
        let sheet = StyleSheet::parse(header_footer::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Header");
        assert_eq!(style.dock, Some(Dock::Top));
    }

    #[test]
    fn header_has_width_100_percent() {
        let sheet = StyleSheet::parse(header_footer::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Header");
        assert_eq!(style.width, Some(Scalar::Percent(100.0)));
    }

    #[test]
    fn footer_has_dock_bottom() {
        let sheet = StyleSheet::parse(header_footer::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Footer");
        assert_eq!(style.dock, Some(Dock::Bottom));
    }

    #[test]
    fn footer_has_layout_horizontal() {
        let sheet = StyleSheet::parse(header_footer::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Footer");
        assert_eq!(style.layout, Some(Layout::Horizontal));
    }

    // ---- input.rs ----

    #[test]
    fn input_has_width_100_percent() {
        let sheet = StyleSheet::parse(input::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Input");
        assert_eq!(style.width, Some(Scalar::Percent(100.0)));
    }

    #[test]
    fn input_has_pointer_text() {
        let sheet = StyleSheet::parse(input::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Input");
        assert_eq!(style.pointer, Some(Pointer::Text));
    }

    #[test]
    fn masked_input_has_pointer_text() {
        let sheet = StyleSheet::parse(input::DEFAULT_CSS);
        let style = find_type_style(&sheet, "MaskedInput");
        assert_eq!(style.pointer, Some(Pointer::Text));
    }

    #[test]
    fn masked_input_has_width_100_percent() {
        let sheet = StyleSheet::parse(input::DEFAULT_CSS);
        let style = find_type_style(&sheet, "MaskedInput");
        assert_eq!(style.width, Some(Scalar::Percent(100.0)));
    }

    // ---- misc.rs ----

    #[test]
    fn placeholder_has_overflow_hidden() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Placeholder");
        assert_eq!(style.overflow, Some(Overflow::Hidden));
    }

    #[test]
    fn placeholder_has_content_align_center_middle() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Placeholder");
        let ca = style
            .content_align
            .expect("Placeholder should have content-align");
        assert_eq!(ca.horizontal, HorizontalAlign::Center);
        assert_eq!(ca.vertical, VerticalAlign::Middle);
    }

    #[test]
    fn rule_horizontal_has_width_1fr_and_margin() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_class_style(&sheet, "Rule", "-horizontal");
        assert_eq!(style.width, Some(Scalar::Fraction(1.0)));
        assert_eq!(style.height, Some(Scalar::Cells(1)));
        assert!(
            style.margin.is_some(),
            "Rule.-horizontal should have margin"
        );
    }

    #[test]
    fn rule_vertical_has_height_1fr_and_margin() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_class_style(&sheet, "Rule", "-vertical");
        assert_eq!(style.height, Some(Scalar::Fraction(1.0)));
        assert_eq!(style.width, Some(Scalar::Cells(1)));
        assert!(style.margin.is_some(), "Rule.-vertical should have margin");
    }

    #[test]
    fn log_has_overflow_scroll() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Log");
        assert_eq!(style.overflow, Some(Overflow::Scroll));
    }

    #[test]
    fn richlog_has_overflow_y_scroll() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_style(&sheet, "RichLog");
        assert_eq!(style.overflow_y, Some(Overflow::Scroll));
    }

    #[test]
    fn switch_has_padding() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Switch");
        let p = style.padding.expect("Switch should have padding");
        assert_eq!(p, Spacing::new(0, 2, 0, 2));
    }

    #[test]
    fn radiobutton_has_padding() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_style(&sheet, "RadioButton");
        let p = style.padding.expect("RadioButton should have padding");
        assert_eq!(p, Spacing::new(0, 1, 0, 1));
    }

    #[test]
    fn radioset_has_padding() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_style(&sheet, "RadioSet");
        let p = style.padding.expect("RadioSet should have padding");
        assert_eq!(p, Spacing::new(0, 1, 0, 1));
    }

    #[test]
    fn progressbar_has_layout_horizontal() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_style(&sheet, "ProgressBar");
        assert_eq!(style.layout, Some(Layout::Horizontal));
    }

    #[test]
    fn link_has_min_height_1() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Link");
        assert_eq!(style.min_height, Some(Scalar::Cells(1)));
    }

    #[test]
    fn toast_has_padding() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Toast");
        let p = style.padding.expect("Toast should have padding");
        assert_eq!(p, Spacing::all(1));
    }

    #[test]
    fn toast_has_max_width_50_percent() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Toast");
        assert_eq!(style.max_width, Some(Scalar::Percent(50.0)));
    }

    #[test]
    fn loading_indicator_has_content_align_center_middle() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_style(&sheet, "LoadingIndicator");
        let ca = style
            .content_align
            .expect("LoadingIndicator should have content-align");
        assert_eq!(ca.horizontal, HorizontalAlign::Center);
        assert_eq!(ca.vertical, VerticalAlign::Middle);
    }

    #[test]
    fn digits_has_text_align_left() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Digits");
        assert_eq!(style.text_align, Some(TextAlign::Left));
    }

    #[test]
    fn command_palette_has_align_horizontal() {
        let sheet = StyleSheet::parse(misc::DEFAULT_CSS);
        let style = find_type_style(&sheet, "CommandPalette");
        let a = style.align.expect("CommandPalette should have align");
        assert_eq!(a.horizontal, HorizontalAlign::Center);
    }

    // ---- select.rs ----

    #[test]
    fn option_list_has_max_height_100_percent() {
        let sheet = StyleSheet::parse(select::DEFAULT_CSS);
        let style = find_type_style(&sheet, "OptionList");
        assert_eq!(style.max_height, Some(Scalar::Percent(100.0)));
    }

    #[test]
    fn option_list_has_padding() {
        let sheet = StyleSheet::parse(select::DEFAULT_CSS);
        let style = find_type_style(&sheet, "OptionList");
        let p = style.padding.expect("OptionList should have padding");
        assert_eq!(p, Spacing::new(0, 1, 0, 1));
    }

    #[test]
    fn option_list_has_overflow_x_hidden() {
        let sheet = StyleSheet::parse(select::DEFAULT_CSS);
        let style = find_type_style(&sheet, "OptionList");
        assert_eq!(style.overflow_x, Some(Overflow::Hidden));
    }

    // ---- text_area.rs ----

    #[test]
    fn text_area_has_1fr_dimensions_and_padding() {
        let sheet = StyleSheet::parse(text_area::DEFAULT_CSS);
        let style = find_type_style(&sheet, "TextArea");
        assert_eq!(style.width, Some(Scalar::Fraction(1.0)));
        assert_eq!(style.height, Some(Scalar::Fraction(1.0)));
        assert!(style.padding.is_some(), "TextArea should have padding");
    }

    // ---- tooltip.rs ----

    #[test]
    fn tooltip_has_constrain_x_inside_y_inflect() {
        let sheet = StyleSheet::parse(tooltip::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Tooltip");
        assert_eq!(style.constrain_x, Some(Constrain::Inside));
        assert_eq!(style.constrain_y, Some(Constrain::Inflect));
    }

    #[test]
    fn tooltip_has_layer() {
        let sheet = StyleSheet::parse(tooltip::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Tooltip");
        assert_eq!(style.layer.as_deref(), Some("_tooltips"));
    }

    #[test]
    fn tooltip_has_margin() {
        let sheet = StyleSheet::parse(tooltip::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Tooltip");
        let m = style.margin.expect("Tooltip should have margin");
        assert_eq!(m, Spacing::new(1, 0, 1, 0));
    }

    #[test]
    fn tooltip_has_padding() {
        let sheet = StyleSheet::parse(tooltip::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Tooltip");
        let p = style.padding.expect("Tooltip should have padding");
        assert_eq!(p, Spacing::new(1, 2, 1, 2));
    }

    #[test]
    fn tooltip_has_max_width_40() {
        let sheet = StyleSheet::parse(tooltip::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Tooltip");
        assert_eq!(style.max_width, Some(Scalar::Cells(40)));
    }

    #[test]
    fn tooltip_has_width_auto() {
        let sheet = StyleSheet::parse(tooltip::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Tooltip");
        assert_eq!(style.width, Some(Scalar::Auto));
    }

    #[test]
    fn tooltip_has_height_auto() {
        let sheet = StyleSheet::parse(tooltip::DEFAULT_CSS);
        let style = find_type_style(&sheet, "Tooltip");
        assert_eq!(style.height, Some(Scalar::Auto));
    }
}
