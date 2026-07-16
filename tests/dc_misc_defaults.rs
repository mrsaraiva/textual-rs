// DC-17..25, DC-32..34, DC-37, DC-38: Misc/presentation widget CSS default parity tests

use textual::css::PseudoClass;
use textual::css::StyleSheet;
use textual::css::default_widget_stylesheet;
use textual::style::{
    BoxSizing, Dock, HorizontalAlign, Layout, Overflow, Pointer, Scalar, Spacing, Split, TextAlign,
    VerticalAlign, Visibility,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn find_type_style(sheet: &StyleSheet, type_name: &str) -> textual::style::Style {
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

/// Find the style of a two-part descendant rule `Ancestor Type { ... }`
/// (used for scoped sub-widget defaults, e.g. `ProgressBar Bar { width: 32 }`).
fn find_scoped_type_style(
    sheet: &StyleSheet,
    ancestor: &str,
    type_name: &str,
) -> textual::style::Style {
    for rule in sheet.rules() {
        let parts = rule.selector_chain().parts();
        if parts.len() == 2
            && parts[0].type_name() == Some(ancestor)
            && parts[1].type_name() == Some(type_name)
            && parts[1].classes().is_empty()
            && parts[1].pseudos().is_empty()
        {
            return rule.style();
        }
    }
    panic!("no rule found for `{ancestor} {type_name}`");
}

fn find_type_class_style(
    sheet: &StyleSheet,
    type_name: &str,
    class: &str,
) -> textual::style::Style {
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

fn find_type_pseudo_style(
    sheet: &StyleSheet,
    type_name: &str,
    pseudo: PseudoClass,
) -> textual::style::Style {
    for rule in sheet.rules() {
        let parts = rule.selector_chain().parts();
        if parts.len() == 1 {
            let sel = &parts[0];
            if sel.type_name() == Some(type_name)
                && sel.pseudos().contains(&pseudo)
                && sel.classes().is_empty()
            {
                return rule.style();
            }
        }
    }
    panic!("no rule found for type `{type_name}` with pseudo `{pseudo:?}`");
}

fn find_child_style(
    sheet: &StyleSheet,
    parent_type: &str,
    child_class: &str,
) -> textual::style::Style {
    for rule in sheet.rules() {
        let parts = rule.selector_chain().parts();
        if parts.len() == 2 {
            let parent = &parts[0];
            let child = &parts[1];
            if parent.type_name() == Some(parent_type)
                && child.classes().iter().any(|c| c == child_class)
            {
                return rule.style();
            }
        }
    }
    panic!("no child rule found for `{parent_type} > .{child_class}`");
}

// ===========================================================================
// DC-17: Switch — pointer: pointer + :light variant
// ===========================================================================

#[test]
fn dc_17_switch_has_pointer() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "Switch");
    assert_eq!(style.pointer, Some(Pointer::Pointer));
}

#[test]
fn dc_17_switch_has_padding_0_2() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "Switch");
    assert_eq!(style.padding, Some(Spacing::new(0, 2, 0, 2)));
}

#[test]
fn dc_17_switch_light_slider_exists() {
    let sheet = default_widget_stylesheet();
    let mut found = false;
    for rule in sheet.rules() {
        let parts = rule.selector_chain().parts();
        if parts.len() == 2 {
            let parent = &parts[0];
            let child = &parts[1];
            if parent.type_name() == Some("Switch")
                && parent.pseudos().contains(&PseudoClass::Light)
                && child.classes().iter().any(|c| c == "switch--slider")
            {
                found = true;
                break;
            }
        }
    }
    assert!(found, "Switch:light > .switch--slider rule should exist");
}

// ===========================================================================
// DC-18: RadioSet — pointer: pointer + compact + child RadioButton rules
// ===========================================================================

#[test]
fn dc_18_radioset_has_pointer() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "RadioSet");
    assert_eq!(style.pointer, Some(Pointer::Pointer));
}

#[test]
fn dc_18_radioset_compact_has_padding_0() {
    let sheet = default_widget_stylesheet();
    let style = find_type_class_style(&sheet, "RadioSet", "-textual-compact");
    assert_eq!(style.padding, Some(Spacing::all(0)));
}

#[test]
fn dc_18_radioset_child_radiobutton_exists() {
    let sheet = default_widget_stylesheet();
    let mut found = false;
    for rule in sheet.rules() {
        let parts = rule.selector_chain().parts();
        if parts.len() == 2 {
            let parent = &parts[0];
            let child = &parts[1];
            if parent.type_name() == Some("RadioSet")
                && child.type_name() == Some("RadioButton")
                && child.classes().is_empty()
            {
                found = true;
                break;
            }
        }
    }
    assert!(found, "RadioSet > RadioButton rule should exist");
}

// ===========================================================================
// DC-19: Placeholder — color: $text
// ===========================================================================

#[test]
fn dc_19_placeholder_has_fg_text() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "Placeholder");
    // $text is an AutoColor token (contrast-based), so it sets fg_auto, not fg
    assert!(
        style.fg_auto.is_some(),
        "Placeholder should have fg_auto ($text is an auto color)"
    );
}

#[test]
fn dc_19_placeholder_has_overflow_hidden() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "Placeholder");
    assert_eq!(style.overflow, Some(Overflow::Hidden));
}

// ===========================================================================
// DC-20: Toast — margin-top + visibility + link-* + ToastHolder + ToastRack
// ===========================================================================

#[test]
fn dc_20_toast_has_margin_top() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "Toast");
    // margin-top is parsed as a per-side override, not the shorthand margin field
    assert_eq!(
        style.margin_top,
        Some(1),
        "Toast should have margin_top = 1"
    );
}

#[test]
fn dc_20_toast_has_visibility_visible() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "Toast");
    assert_eq!(style.visibility, Some(Visibility::Visible));
}

#[test]
fn dc_20_toast_has_link_color() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "Toast");
    assert!(style.link_color.is_some(), "Toast should have link-color");
}

#[test]
fn dc_20_toast_has_link_style() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "Toast");
    assert!(style.link_style.is_some(), "Toast should have link-style");
}

#[test]
fn dc_20_toast_holder_has_visibility_hidden() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "ToastHolder");
    assert_eq!(style.visibility, Some(Visibility::Hidden));
}

#[test]
fn dc_20_toast_holder_has_width_1fr() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "ToastHolder");
    assert_eq!(style.width, Some(Scalar::Fraction(1.0)));
}

#[test]
fn dc_20_toast_rack_has_layer() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "ToastRack");
    assert_eq!(style.layer.as_deref(), Some("_toastrack"));
}

#[test]
fn dc_20_toast_rack_has_dock_bottom() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "ToastRack");
    assert_eq!(style.dock, Some(Dock::Bottom));
}

#[test]
fn dc_20_toast_rack_has_overflow_y_scroll() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "ToastRack");
    assert_eq!(style.overflow_y, Some(Overflow::Scroll));
}

// ===========================================================================
// DC-21: Collapsible — border-top: hkey $background + :focus-within + Contents padding
// ===========================================================================

#[test]
fn dc_21_collapsible_has_border_top() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "Collapsible");
    assert!(
        style.border_top.is_set(),
        "Collapsible should have border-top (hkey)"
    );
}

#[test]
fn dc_21_collapsible_focus_within_exists() {
    let sheet = default_widget_stylesheet();
    let style = find_type_pseudo_style(&sheet, "Collapsible", PseudoClass::FocusWithin);
    assert!(
        style.background_tint.is_some(),
        "Collapsible:focus-within should have background-tint"
    );
}

#[test]
fn dc_21_contents_has_padding() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "Contents");
    let p = style.padding.expect("Contents should have padding");
    assert_eq!(p, Spacing::new(1, 0, 0, 3));
}

#[test]
fn dc_21_collapsible_title_has_pointer() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "CollapsibleTitle");
    assert_eq!(style.pointer, Some(Pointer::Pointer));
}

// ===========================================================================
// DC-22: ProgressBar — width: auto + PercentageStatus + ETAStatus
// ===========================================================================

#[test]
fn dc_22_progressbar_has_width_auto() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "ProgressBar");
    assert_eq!(style.width, Some(Scalar::Auto));
}

#[test]
fn dc_22_progressbar_has_layout_horizontal() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "ProgressBar");
    assert_eq!(style.layout, Some(Layout::Horizontal));
}

#[test]
fn dc_22_bar_has_width_32() {
    // Scoped under ProgressBar (Python scoped DEFAULT_CSS): a bare `Bar`
    // type rule would leak onto user widgets named `Bar`.
    let sheet = default_widget_stylesheet();
    let style = find_scoped_type_style(&sheet, "ProgressBar", "Bar");
    assert_eq!(style.width, Some(Scalar::Cells(32)));
}

#[test]
fn dc_22_percentage_status_has_width_5() {
    let sheet = default_widget_stylesheet();
    let style = find_scoped_type_style(&sheet, "ProgressBar", "PercentageStatus");
    assert_eq!(style.width, Some(Scalar::Cells(5)));
}

#[test]
fn dc_22_eta_status_has_width_9() {
    let sheet = default_widget_stylesheet();
    let style = find_scoped_type_style(&sheet, "ProgressBar", "ETAStatus");
    assert_eq!(style.width, Some(Scalar::Cells(9)));
}

// ===========================================================================
// DC-23: Link — pointer: pointer + :focus text-style: bold reverse
// ===========================================================================

#[test]
fn dc_23_link_has_pointer() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "Link");
    assert_eq!(style.pointer, Some(Pointer::Pointer));
}

#[test]
fn dc_23_link_focus_has_bold_reverse() {
    let sheet = default_widget_stylesheet();
    let style = find_type_pseudo_style(&sheet, "Link", PseudoClass::Focus);
    assert_eq!(style.bold, Some(true), "Link:focus should have bold");
    assert_eq!(style.reverse, Some(true), "Link:focus should have reverse");
}

// ===========================================================================
// DC-24: LoadingIndicator — width/height: 100% + text-style: not reverse
// ===========================================================================

#[test]
fn dc_24_loading_indicator_has_width_100_pct() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "LoadingIndicator");
    assert_eq!(style.width, Some(Scalar::Percent(100.0)));
}

#[test]
fn dc_24_loading_indicator_has_height_100_pct() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "LoadingIndicator");
    assert_eq!(style.height, Some(Scalar::Percent(100.0)));
}

#[test]
fn dc_24_loading_indicator_textual_class_has_layer() {
    let sheet = default_widget_stylesheet();
    let style = find_type_class_style(&sheet, "LoadingIndicator", "-textual-loading-indicator");
    assert_eq!(style.layer.as_deref(), Some("_loading"));
}

#[test]
fn dc_24_loading_indicator_textual_class_has_dock_top() {
    let sheet = default_widget_stylesheet();
    let style = find_type_class_style(&sheet, "LoadingIndicator", "-textual-loading-indicator");
    assert_eq!(style.dock, Some(Dock::Top));
}

// ===========================================================================
// DC-25: Digits — box-sizing: border-box
// ===========================================================================

#[test]
fn dc_25_digits_has_box_sizing_border_box() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "Digits");
    assert_eq!(style.box_sizing, Some(BoxSizing::BorderBox));
}

#[test]
fn dc_25_digits_has_text_align_left() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "Digits");
    assert_eq!(style.text_align, Some(TextAlign::Left));
}

// ===========================================================================
// DC-32: Tree — cursor, guides, highlight-line, :light, :ansi
// ===========================================================================

#[test]
fn dc_32_tree_has_bg_surface() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "Tree");
    assert!(style.bg.is_some(), "Tree should have bg");
}

#[test]
fn dc_32_tree_cursor_exists() {
    let sheet = default_widget_stylesheet();
    let style = find_child_style(&sheet, "Tree", "tree--cursor");
    assert!(
        style.bg.is_some(),
        "Tree > .tree--cursor should have background"
    );
}

#[test]
fn dc_32_tree_guides_hover_exists() {
    let sheet = default_widget_stylesheet();
    let style = find_child_style(&sheet, "Tree", "tree--guides-hover");
    assert!(
        style.fg.is_some(),
        "Tree > .tree--guides-hover should have fg"
    );
}

#[test]
fn dc_32_tree_guides_selected_exists() {
    let sheet = default_widget_stylesheet();
    let style = find_child_style(&sheet, "Tree", "tree--guides-selected");
    assert!(
        style.fg.is_some(),
        "Tree > .tree--guides-selected should have fg"
    );
}

#[test]
fn dc_32_tree_highlight_line_exists() {
    let sheet = default_widget_stylesheet();
    let style = find_child_style(&sheet, "Tree", "tree--highlight-line");
    assert!(
        style.bg.is_some(),
        "Tree > .tree--highlight-line should have bg"
    );
}

#[test]
fn dc_32_tree_focus_has_background_tint() {
    let sheet = default_widget_stylesheet();
    let style = find_type_pseudo_style(&sheet, "Tree", PseudoClass::Focus);
    assert!(
        style.background_tint.is_some(),
        "Tree:focus should have background-tint"
    );
}

#[test]
fn dc_32_tree_ansi_exists() {
    let sheet = default_widget_stylesheet();
    let style = find_type_pseudo_style(&sheet, "Tree", PseudoClass::Ansi);
    assert!(style.fg.is_some(), "Tree:ansi should have fg");
}

// ===========================================================================
// DC-33: ListView — block-cursor tokens + :focus background-tint
// ===========================================================================

#[test]
fn dc_33_listview_has_bg() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "ListView");
    assert!(style.bg.is_some(), "ListView should have bg");
}

#[test]
fn dc_33_listview_focus_has_background_tint() {
    let sheet = default_widget_stylesheet();
    let style = find_type_pseudo_style(&sheet, "ListView", PseudoClass::Focus);
    assert!(
        style.background_tint.is_some(),
        "ListView:focus should have background-tint"
    );
}

#[test]
fn dc_33_listview_listitem_hovered_exists() {
    let sheet = default_widget_stylesheet();
    let mut found = false;
    for rule in sheet.rules() {
        let parts = rule.selector_chain().parts();
        if parts.len() == 2 {
            let parent = &parts[0];
            let child = &parts[1];
            if parent.type_name() == Some("ListView")
                && child.type_name() == Some("ListItem")
                && child.classes().iter().any(|c| c == "-hovered")
            {
                found = true;
                break;
            }
        }
    }
    assert!(found, "ListView > ListItem.-hovered rule should exist");
}

#[test]
fn dc_33_listview_listitem_highlight_exists() {
    let sheet = default_widget_stylesheet();
    let mut found = false;
    for rule in sheet.rules() {
        let parts = rule.selector_chain().parts();
        if parts.len() == 2 {
            let parent = &parts[0];
            let child = &parts[1];
            if parent.type_name() == Some("ListView")
                && child.type_name() == Some("ListItem")
                && child.classes().iter().any(|c| c == "-highlight")
            {
                found = true;
                break;
            }
        }
    }
    assert!(found, "ListView > ListItem.-highlight rule should exist");
}

// ===========================================================================
// DC-34: RichLog — overflow-y: scroll (not overflow: scroll)
// ===========================================================================

#[test]
fn dc_34_richlog_has_overflow_y_scroll() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "RichLog");
    assert_eq!(style.overflow_y, Some(Overflow::Scroll));
}

#[test]
fn dc_34_richlog_overflow_shorthand_is_none() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "RichLog");
    assert_eq!(
        style.overflow, None,
        "RichLog should use overflow-y, not shorthand overflow"
    );
}

// ===========================================================================
// DC-37: Markdown — full parity with Python
// ===========================================================================

#[test]
fn dc_37_markdown_has_layout_vertical() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "Markdown");
    assert_eq!(style.layout, Some(Layout::Vertical));
}

#[test]
fn dc_37_markdown_has_height_auto() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "Markdown");
    assert_eq!(style.height, Some(Scalar::Auto));
}

#[test]
fn dc_37_markdown_has_padding() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "Markdown");
    let p = style.padding.expect("Markdown should have padding");
    assert_eq!(p, Spacing::new(0, 2, 0, 2));
}

#[test]
fn dc_37_markdown_block_has_width_1fr() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "MarkdownBlock");
    assert_eq!(style.width, Some(Scalar::Fraction(1.0)));
}

#[test]
fn dc_37_markdown_h1_has_content_align_center_middle() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "MarkdownH1");
    let ca = style
        .content_align
        .expect("MarkdownH1 should have content-align");
    assert_eq!(ca.horizontal, HorizontalAlign::Center);
    assert_eq!(ca.vertical, VerticalAlign::Middle);
}

#[test]
fn dc_37_markdown_header_has_margin() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "MarkdownHeader");
    let m = style.margin.expect("MarkdownHeader should have margin");
    assert_eq!(m, Spacing::new(2, 0, 1, 0));
}

#[test]
fn dc_37_markdown_horizontal_rule_has_border_bottom() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "MarkdownHorizontalRule");
    assert!(
        style.border_bottom.is_set(),
        "MarkdownHorizontalRule should have border-bottom"
    );
}

#[test]
fn dc_37_markdown_blockquote_has_border_left() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "MarkdownBlockQuote");
    assert!(
        style.border_left.is_set(),
        "MarkdownBlockQuote should have border-left"
    );
}

#[test]
fn dc_37_markdown_fence_has_overflow() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "MarkdownFence");
    assert_eq!(style.overflow_x, Some(Overflow::Scroll));
    assert_eq!(style.overflow_y, Some(Overflow::Hidden));
}

#[test]
fn dc_37_markdown_table_content_has_layout_grid() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "MarkdownTableContent");
    assert_eq!(style.layout, Some(Layout::Grid));
}

#[test]
fn dc_37_markdown_table_content_has_keyline() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "MarkdownTableContent");
    assert!(
        style.keyline.is_some(),
        "MarkdownTableContent should have keyline"
    );
}

#[test]
fn dc_37_markdown_list_has_width_1fr() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "MarkdownList");
    assert_eq!(style.width, Some(Scalar::Fraction(1.0)));
}

#[test]
fn dc_37_markdown_bullet_list_has_margin() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "MarkdownBulletList");
    let m = style.margin.expect("MarkdownBulletList should have margin");
    assert_eq!(m, Spacing::new(0, 0, 1, 0));
}

#[test]
fn dc_37_markdown_list_item_has_layout_horizontal() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "MarkdownListItem");
    assert_eq!(style.layout, Some(Layout::Horizontal));
}

#[test]
fn dc_37_markdown_bullet_has_width_auto() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "MarkdownBullet");
    assert_eq!(style.width, Some(Scalar::Auto));
}

#[test]
fn dc_37_markdown_table_of_contents_has_bg() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "MarkdownTableOfContents");
    assert!(style.bg.is_some(), "MarkdownTableOfContents should have bg");
}

#[test]
fn dc_37_markdown_viewer_has_scrollbar_gutter() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "MarkdownViewer");
    assert!(
        style.scrollbar_gutter.is_some(),
        "MarkdownViewer should have scrollbar-gutter"
    );
}

// ===========================================================================
// DC-38: HelpPanel/KeyPanel — split + border-left + padding + BindingsTable
// ===========================================================================

#[test]
fn dc_38_help_panel_has_split_right() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "HelpPanel");
    assert_eq!(style.split, Some(Split::Right));
}

#[test]
fn dc_38_help_panel_has_width_33_pct() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "HelpPanel");
    assert_eq!(style.width, Some(Scalar::Percent(33.0)));
}

#[test]
fn dc_38_help_panel_has_layout_vertical() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "HelpPanel");
    assert_eq!(style.layout, Some(Layout::Vertical));
}

#[test]
fn dc_38_key_panel_has_split_right() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "KeyPanel");
    assert_eq!(style.split, Some(Split::Right));
}

#[test]
fn dc_38_key_panel_has_width_33_pct() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "KeyPanel");
    assert_eq!(style.width, Some(Scalar::Percent(33.0)));
}

#[test]
fn dc_38_bindings_table_has_width_auto() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "BindingsTable");
    assert_eq!(style.width, Some(Scalar::Auto));
}

#[test]
fn dc_38_bindings_table_has_height_auto() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "BindingsTable");
    assert_eq!(style.height, Some(Scalar::Auto));
}

#[test]
fn dc_38_key_panel_bindings_key_has_padding() {
    let sheet = default_widget_stylesheet();
    let mut found = false;
    for rule in sheet.rules() {
        let parts = rule.selector_chain().parts();
        if parts.len() == 3 {
            let p0 = &parts[0];
            let p2 = &parts[2];
            if p0.type_name() == Some("KeyPanel")
                && p2.classes().iter().any(|c| c == "bindings-table--key")
            {
                let style = rule.style();
                assert!(
                    style.padding.is_some(),
                    "bindings-table--key should have padding"
                );
                found = true;
                break;
            }
        }
    }
    assert!(
        found,
        "KeyPanel > BindingsTable > .bindings-table--key rule should exist"
    );
}

// ===========================================================================
// Codex review fixes — additional coverage
// ===========================================================================

#[test]
fn dc_37_markdown_nested_list_has_margin_0() {
    let sheet = default_widget_stylesheet();
    // MarkdownList MarkdownList { margin: 0; padding-top: 0; }
    let mut found = false;
    for rule in sheet.rules() {
        let parts = rule.selector_chain().parts();
        if parts.len() == 2 {
            let parent = &parts[0];
            let child = &parts[1];
            if parent.type_name() == Some("MarkdownList")
                && child.type_name() == Some("MarkdownList")
            {
                let style = rule.style();
                assert_eq!(style.margin, Some(Spacing::all(0)));
                found = true;
                break;
            }
        }
    }
    assert!(found, "MarkdownList MarkdownList rule should exist");
}

#[test]
fn dc_37_markdown_table_header_has_bold() {
    let sheet = default_widget_stylesheet();
    let style = find_child_style(&sheet, "MarkdownTableContent", "markdown-table--header");
    assert_eq!(
        style.bold,
        Some(true),
        "markdown-table--header should be bold"
    );
}

#[test]
fn dc_37_markdown_blockquote_border_has_tint() {
    let sheet = default_widget_stylesheet();
    let style = find_type_style(&sheet, "MarkdownBlockQuote");
    assert!(
        style.border_left.is_set(),
        "MarkdownBlockQuote should have border-left (outer $text-primary 50%)"
    );
}

#[test]
fn dc_38_help_panel_ansi_has_bg() {
    let sheet = default_widget_stylesheet();
    let style = find_type_pseudo_style(&sheet, "HelpPanel", PseudoClass::Ansi);
    assert!(
        style.bg.is_some(),
        "HelpPanel:ansi should have bg (ansi_default)"
    );
}

#[test]
fn dc_32_directory_tree_ansi_guides_exist() {
    let sheet = default_widget_stylesheet();
    let mut found = false;
    for rule in sheet.rules() {
        let parts = rule.selector_chain().parts();
        if parts.len() == 2 {
            let parent = &parts[0];
            let child = &parts[1];
            if parent.type_name() == Some("DirectoryTree")
                && parent.pseudos().contains(&PseudoClass::Ansi)
                && child.classes().iter().any(|c| c == "tree--guides")
            {
                found = true;
                break;
            }
        }
    }
    assert!(
        found,
        "DirectoryTree:ansi > .tree--guides rule should exist"
    );
}

#[test]
fn dc_20_toast_title_uses_descendant() {
    let sheet = default_widget_stylesheet();
    // Toast .toast--title (descendant, not child)
    let mut found = false;
    for rule in sheet.rules() {
        let parts = rule.selector_chain().parts();
        if parts.len() == 2 {
            let parent = &parts[0];
            let child = &parts[1];
            if parent.type_name() == Some("Toast")
                && parent.classes().is_empty()
                && parent.pseudos().is_empty()
                && child.classes().iter().any(|c| c == "toast--title")
            {
                let style = rule.style();
                assert_eq!(style.bold, Some(true), "toast--title should be bold");
                found = true;
                break;
            }
        }
    }
    assert!(found, "Toast .toast--title rule should exist");
}

// ===========================================================================
// Combined parse test — everything parses without panic
// ===========================================================================

#[test]
fn dc_all_misc_defaults_parse_without_panic() {
    let sheet = default_widget_stylesheet();
    assert!(
        sheet.rules().len() > 100,
        "combined stylesheet should have many rules (got {})",
        sheet.rules().len()
    );
}
