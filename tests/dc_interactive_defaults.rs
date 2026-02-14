// DC interactive defaults parity tests
// Tests for DC-13 (Button), DC-14 (Input), DC-15 (TextArea), DC-16 (DataTable),
// DC-27 (Header), DC-28 (Footer), DC-29 (OptionList), DC-30 (SelectionList),
// DC-31 (Tabs), DC-35 (Checkbox/ToggleButton)
//
// Tests via the combined default stylesheet using the public selector API
// to find bare type rules and verify parsed properties.

use textual::css::default_widget_stylesheet;
use textual::style::{Pointer, Scalar, Spacing, TextAlign};

/// Find a bare type rule (single selector part, no classes, no pseudos) by type name
/// using the public selector API.
fn default_style_for_type(type_name: &str) -> textual::style::Style {
    let sheet = default_widget_stylesheet();
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
    panic!("no bare-type rule found for `{type_name}` in default stylesheet");
}

// ==== DC-13: Button ====

#[test]
fn dc_13_button_has_pointer_pointer() {
    let style = default_style_for_type("Button");
    assert_eq!(style.pointer, Some(Pointer::Pointer));
}

#[test]
fn dc_13_button_has_content_align_center_middle() {
    let style = default_style_for_type("Button");
    let ca = style.content_align.expect("Button should have content-align");
    assert_eq!(ca.horizontal, textual::style::HorizontalAlign::Center);
    assert_eq!(ca.vertical, textual::style::VerticalAlign::Middle);
}

#[test]
fn dc_13_button_has_text_align_center() {
    let style = default_style_for_type("Button");
    assert_eq!(style.text_align, Some(TextAlign::Center));
}

#[test]
fn dc_13_button_has_width_auto_min_width_16() {
    let style = default_style_for_type("Button");
    assert_eq!(style.width, Some(Scalar::Auto));
    assert_eq!(style.min_width, Some(Scalar::Cells(16)));
}

// ==== DC-14: Input ====

#[test]
fn dc_14_input_has_pointer_text() {
    let style = default_style_for_type("Input");
    assert_eq!(style.pointer, Some(Pointer::Text));
}

#[test]
fn dc_14_input_has_width_100_height_3() {
    let style = default_style_for_type("Input");
    assert_eq!(style.width, Some(Scalar::Percent(100.0)));
    assert_eq!(style.height, Some(Scalar::Cells(3)));
}

#[test]
fn dc_14_input_has_padding_0_2() {
    let style = default_style_for_type("Input");
    let p = style.padding.expect("Input should have padding");
    assert_eq!(p, Spacing::new(0, 2, 0, 2));
}

#[test]
fn dc_14_masked_input_has_pointer_text() {
    let style = default_style_for_type("MaskedInput");
    assert_eq!(style.pointer, Some(Pointer::Text));
}

// ==== DC-15: TextArea ====

#[test]
fn dc_15_text_area_has_pointer_text() {
    let style = default_style_for_type("TextArea");
    assert_eq!(style.pointer, Some(Pointer::Text));
}

#[test]
fn dc_15_text_area_has_1fr_dimensions() {
    let style = default_style_for_type("TextArea");
    assert_eq!(style.width, Some(Scalar::Fraction(1.0)));
    assert_eq!(style.height, Some(Scalar::Fraction(1.0)));
}

#[test]
fn dc_15_text_area_has_padding_0_1() {
    let style = default_style_for_type("TextArea");
    let p = style.padding.expect("TextArea should have padding");
    assert_eq!(p, Spacing::new(0, 1, 0, 1));
}

// ==== DC-16: DataTable ====

#[test]
fn dc_16_data_table_has_height_auto() {
    let style = default_style_for_type("DataTable");
    assert_eq!(style.height, Some(Scalar::Auto));
}

#[test]
fn dc_16_data_table_has_max_height_100() {
    let style = default_style_for_type("DataTable");
    assert_eq!(style.max_height, Some(Scalar::Percent(100.0)));
}

// ==== DC-27: Header ====

#[test]
fn dc_27_header_has_height_1() {
    let style = default_style_for_type("Header");
    assert_eq!(style.height, Some(Scalar::Cells(1)));
}

#[test]
fn dc_27_header_has_dock_top() {
    let style = default_style_for_type("Header");
    assert_eq!(style.dock, Some(textual::style::Dock::Top));
}

#[test]
fn dc_27_header_has_width_100() {
    let style = default_style_for_type("Header");
    assert_eq!(style.width, Some(Scalar::Percent(100.0)));
}

#[test]
fn dc_27_header_icon_has_dock_left() {
    let style = default_style_for_type("HeaderIcon");
    assert_eq!(style.dock, Some(textual::style::Dock::Left));
}

#[test]
fn dc_27_header_title_has_width_100() {
    let style = default_style_for_type("HeaderTitle");
    assert_eq!(style.width, Some(Scalar::Percent(100.0)));
}

#[test]
fn dc_27_header_clock_space_has_dock_right() {
    let style = default_style_for_type("HeaderClockSpace");
    assert_eq!(style.dock, Some(textual::style::Dock::Right));
}

// ==== DC-28: Footer ====

#[test]
fn dc_28_footer_has_height_1() {
    let style = default_style_for_type("Footer");
    assert_eq!(style.height, Some(Scalar::Cells(1)));
}

#[test]
fn dc_28_footer_has_dock_bottom() {
    let style = default_style_for_type("Footer");
    assert_eq!(style.dock, Some(textual::style::Dock::Bottom));
}

#[test]
fn dc_28_footer_has_layout_horizontal() {
    let style = default_style_for_type("Footer");
    assert_eq!(style.layout, Some(textual::style::Layout::Horizontal));
}

#[test]
fn dc_28_footer_key_has_height_1() {
    let style = default_style_for_type("FooterKey");
    assert_eq!(style.height, Some(Scalar::Cells(1)));
}

#[test]
fn dc_28_key_group_has_width_auto() {
    let style = default_style_for_type("KeyGroup");
    assert_eq!(style.width, Some(Scalar::Auto));
}

// ==== DC-29: OptionList ====

#[test]
fn dc_29_option_list_has_max_height_100() {
    let style = default_style_for_type("OptionList");
    assert_eq!(style.max_height, Some(Scalar::Percent(100.0)));
}

#[test]
fn dc_29_option_list_has_padding_0_1() {
    let style = default_style_for_type("OptionList");
    let p = style.padding.expect("OptionList should have padding");
    assert_eq!(p, Spacing::new(0, 1, 0, 1));
}

#[test]
fn dc_29_option_list_has_overflow_x_hidden() {
    let style = default_style_for_type("OptionList");
    assert_eq!(
        style.overflow_x,
        Some(textual::style::Overflow::Hidden)
    );
}

// ==== DC-30: SelectionList ====

#[test]
fn dc_30_selection_list_has_height_auto() {
    let style = default_style_for_type("SelectionList");
    assert_eq!(style.height, Some(Scalar::Auto));
}

// ==== DC-31: Tabs ====

#[test]
fn dc_31_tab_has_pointer_pointer() {
    let style = default_style_for_type("Tab");
    assert_eq!(style.pointer, Some(Pointer::Pointer));
}

#[test]
fn dc_31_tab_has_width_auto_height_1() {
    let style = default_style_for_type("Tab");
    assert_eq!(style.width, Some(Scalar::Auto));
    assert_eq!(style.height, Some(Scalar::Cells(1)));
}

#[test]
fn dc_31_tab_has_text_align_center() {
    let style = default_style_for_type("Tab");
    assert_eq!(style.text_align, Some(TextAlign::Center));
}

#[test]
fn dc_31_tabs_has_width_100_height_2() {
    let style = default_style_for_type("Tabs");
    assert_eq!(style.width, Some(Scalar::Percent(100.0)));
    assert_eq!(style.height, Some(Scalar::Cells(2)));
}

#[test]
fn dc_31_underline_has_1fr_width_height_1() {
    let style = default_style_for_type("Underline");
    assert_eq!(style.width, Some(Scalar::Fraction(1.0)));
    assert_eq!(style.height, Some(Scalar::Cells(1)));
}

#[test]
fn dc_31_tab_pane_has_height_auto() {
    let style = default_style_for_type("TabPane");
    assert_eq!(style.height, Some(Scalar::Auto));
}

#[test]
fn dc_31_tabbed_content_has_height_auto() {
    let style = default_style_for_type("TabbedContent");
    assert_eq!(style.height, Some(Scalar::Auto));
}

// ==== DC-35: Checkbox / ToggleButton ====

#[test]
fn dc_35_checkbox_has_pointer_pointer() {
    let style = default_style_for_type("Checkbox");
    assert_eq!(style.pointer, Some(Pointer::Pointer));
}

#[test]
fn dc_35_checkbox_has_width_auto() {
    let style = default_style_for_type("Checkbox");
    assert_eq!(style.width, Some(Scalar::Auto));
}

#[test]
fn dc_35_checkbox_has_padding_0_1() {
    let style = default_style_for_type("Checkbox");
    let p = style.padding.expect("Checkbox should have padding");
    assert_eq!(p, Spacing::new(0, 1, 0, 1));
}

#[test]
fn dc_35_toggle_button_has_pointer_pointer() {
    let style = default_style_for_type("ToggleButton");
    assert_eq!(style.pointer, Some(Pointer::Pointer));
}

#[test]
fn dc_35_toggle_button_has_padding_0_1() {
    let style = default_style_for_type("ToggleButton");
    let p = style.padding.expect("ToggleButton should have padding");
    assert_eq!(p, Spacing::new(0, 1, 0, 1));
}

// ==== Combined parse test ====

#[test]
fn dc_all_interactive_defaults_parse_without_panic() {
    let sheet = default_widget_stylesheet();
    assert!(
        !sheet.rules().is_empty(),
        "default stylesheet should have rules"
    );
}
