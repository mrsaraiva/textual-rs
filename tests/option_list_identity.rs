//! OptionList / SelectionList stable-id identity tests.
//!
//! Ports of the Python Textual test files (adapted to the Rust widget-level
//! API; Python raises become typed `OptionListError` results):
//! - `tests/option_list/test_option_list_id_stability.py`
//! - `tests/option_list/test_option_removal.py`
//! - `tests/option_list/test_option_prompt_replacement.py`
//! - `tests/option_list/test_option_list_disabled.py`
//! - `tests/option_list/test_option_list_create.py` (duplicate-id + core paths)
//!
//! The `test_option_messages.py` / message-field ports live as unit tests in
//! `src/widgets/option_list.rs` and `src/widgets/selection_list.rs` (message
//! capture uses the crate-private `EventCtx::take_messages`).
//! - `tests/selection_list/test_selection_values.py`
//! - `tests/selection_list/test_selection_list_create.py`

use textual::event::{EventCtx, WidgetCtx};
use textual::node_id::NodeId;
use textual::widgets::{
    OptionId, OptionItem, OptionList, OptionListError, Selection, SelectionList, Widget,
};

fn sample_list() -> OptionList {
    // Python fixture: Option("0", id="0"), Option("1", id="1").
    OptionList::with_items(vec![
        OptionItem::with_id("0", "0"),
        OptionItem::with_id("1", "1"),
    ])
}

/// Python `test_option_list_create.py` fixture: "0", "1", "2" (disabled),
/// "3" (id=3), "4" (id=4, disabled). (The Python `None` separators between
/// them are dividers on the previous option, not items, so they do not enter
/// the count; the Rust port omits them.)
fn create_fixture() -> OptionList {
    OptionList::with_items(vec![
        OptionItem::new("0"),
        OptionItem::new("1"),
        OptionItem::disabled("2"),
        OptionItem::with_id("3", "3"),
        OptionItem::disabled_with_id("4", "4"),
    ])
}

// ── test_option_list_id_stability.py ─────────────────────────────────────

#[test]
fn get_after_add() {
    let mut list = OptionList::new();
    list.add_option("0", Some(OptionId::new("0")), false)
        .expect("no duplicate");
    assert_eq!(
        list.get_option_by_id("0").expect("exists").string_id(),
        Some("0")
    );
}

// ── test_option_list_create.py ───────────────────────────────────────────

#[test]
fn all_parameters_become_options() {
    let list = create_fixture();
    assert_eq!(list.option_count(), 5);
    for n in 0..5 {
        assert!(list.get_option_at_index(n).is_ok());
    }
}

#[test]
fn id_capture() {
    let list = create_fixture();
    let with_id = (0..5)
        .filter(|&n| list.get_option_at_index(n).unwrap().string_id().is_some())
        .count();
    assert_eq!(with_id, 2);
}

#[test]
fn get_option_by_id() {
    let list = create_fixture();
    assert_eq!(list.get_option_by_id("3").unwrap().prompt(), Some("3"));
    assert_eq!(list.get_option_by_id("4").unwrap().prompt(), Some("4"));
}

#[test]
fn get_option_with_bad_id() {
    let list = create_fixture();
    assert_eq!(
        list.get_option_by_id("this does not exist"),
        Err(OptionListError::UnknownId(OptionId::new(
            "this does not exist"
        )))
    );
}

#[test]
fn get_option_by_index_and_bad_index() {
    let list = create_fixture();
    for n in 0..5 {
        assert_eq!(
            list.get_option_at_index(n).unwrap().prompt(),
            Some(n.to_string().as_str())
        );
    }
    assert_eq!(
        list.get_option_at_index(42),
        Err(OptionListError::IndexOutOfBounds(42))
    );
}

#[test]
fn clear_option_list() {
    let mut list = create_fixture();
    assert_eq!(list.option_count(), 5);
    list.clear_options();
    assert_eq!(list.option_count(), 0);
    // A cleared registry accepts the previously-used ids again.
    list.add_option("3", Some(OptionId::new("3")), false)
        .expect("registry cleared with items");
}

#[test]
fn add_later() {
    let mut list = create_fixture();
    assert_eq!(list.option_count(), 5);
    list.add_option("more", None, false).unwrap();
    assert_eq!(list.option_count(), 6);
    list.add_item(OptionItem::new("even more")).unwrap();
    assert_eq!(list.option_count(), 7);
    list.add_options(vec![
        OptionItem::new("more still"),
        OptionItem::new("Yet more options"),
        OptionItem::new("so many options!"),
    ])
    .unwrap();
    assert_eq!(list.option_count(), 10);
    list.add_options(Vec::new()).unwrap();
    assert_eq!(list.option_count(), 10);
}

#[test]
fn create_with_duplicate_id() {
    let mut list = create_fixture();
    assert_eq!(list.option_count(), 5);
    assert_eq!(
        list.add_option("dupe", Some(OptionId::new("3")), false),
        Err(OptionListError::DuplicateId(OptionId::new("3")))
    );
    assert_eq!(list.option_count(), 5);
}

#[test]
fn create_with_duplicate_id_and_subsequent_non_dupes() {
    let mut list = create_fixture();
    assert!(list.add_option("dupe", Some(OptionId::new("3")), false).is_err());
    assert_eq!(list.option_count(), 5);
    list.add_option("Not a dupe", Some(OptionId::new("6")), false)
        .unwrap();
    assert_eq!(list.option_count(), 6);
    list.add_option("Not a dupe", Some(OptionId::new("7")), false)
        .unwrap();
    assert_eq!(list.option_count(), 7);
}

/// Residual pin (spec section 6): a failing `add_options` batch mutates
/// nothing (Python whole-batch pre-check).
#[test]
fn adding_multiple_duplicates_at_once_is_atomic() {
    let mut list = create_fixture();
    assert_eq!(
        list.add_options(vec![
            OptionItem::with_id("dupe", "42"),
            OptionItem::with_id("dupe", "42"),
        ]),
        Err(OptionListError::DuplicateId(OptionId::new("42")))
    );
    assert_eq!(list.option_count(), 5);
    // The batch registered nothing: "42" is still free.
    list.add_option("now fine", Some(OptionId::new("42")), false)
        .unwrap();
    // A batch colliding with an EXISTING id is also fully rejected.
    assert_eq!(
        list.add_options(vec![
            OptionItem::with_id("fresh", "43"),
            OptionItem::with_id("dupe", "3"),
        ]),
        Err(OptionListError::DuplicateId(OptionId::new("3")))
    );
    assert!(list.get_option_by_id("43").is_err());
}

#[test]
fn options_are_available_soon() {
    // Regression parity: id lookups work immediately after construction,
    // before any mount/layout.
    let list = OptionList::with_items(vec![OptionItem::with_id("", "some_id")]);
    assert!(list.get_option_by_id("some_id").is_ok());
}

#[test]
fn set_options() {
    let mut list = create_fixture();
    list.set_items(vec![OptionItem::new("foo"), OptionItem::new("bar")]);
    assert_eq!(list.option_count(), 2);
    assert_eq!(list.get_option_at_index(0).unwrap().prompt(), Some("foo"));
    assert_eq!(list.get_option_at_index(1).unwrap().prompt(), Some("bar"));
    // set_items rebuilt the registry: old ids are gone.
    assert!(list.get_option_by_id("3").is_err());
}

/// Constructor policy pin (spec 3.2): `with_items` stays infallible and
/// panics on duplicate ids (Python raises `DuplicateID` out of `__init__`).
#[test]
#[should_panic(expected = "duplicate option id")]
fn with_items_panics_on_duplicate_ids() {
    let _ = OptionList::with_items(vec![
        OptionItem::with_id("a", "dup"),
        OptionItem::with_id("b", "dup"),
    ]);
}

#[test]
#[should_panic(expected = "duplicate option id")]
fn set_items_panics_on_duplicate_ids() {
    let mut list = OptionList::new();
    list.set_items(vec![
        OptionItem::with_id("a", "dup"),
        OptionItem::with_id("b", "dup"),
    ]);
}

// ── test_option_removal.py ───────────────────────────────────────────────

#[test]
fn remove_first_option_via_index() {
    let mut list = sample_list();
    assert_eq!(list.option_count(), 2);
    assert_eq!(list.highlighted(), Some(0));
    list.remove_option_at_index(0).unwrap();
    assert_eq!(list.option_count(), 1);
    assert_eq!(list.highlighted(), Some(0));
}

#[test]
fn remove_first_option_via_id() {
    let mut list = sample_list();
    list.remove_option("0").unwrap();
    assert_eq!(list.option_count(), 1);
    assert_eq!(list.highlighted(), Some(0));
    // The registry shifted: "1" now resolves to index 0.
    assert_eq!(list.get_option_index("1"), Ok(0));
}

#[test]
fn remove_last_option_via_index() {
    let mut list = sample_list();
    list.remove_option_at_index(1).unwrap();
    assert_eq!(list.option_count(), 1);
    assert_eq!(list.highlighted(), Some(0));
}

#[test]
fn remove_last_option_via_id() {
    let mut list = sample_list();
    list.remove_option("1").unwrap();
    assert_eq!(list.option_count(), 1);
    assert_eq!(list.highlighted(), Some(0));
    assert_eq!(list.get_option_index("0"), Ok(0));
}

#[test]
fn remove_all_options_via_index() {
    let mut list = sample_list();
    list.remove_option_at_index(0).unwrap();
    list.remove_option_at_index(0).unwrap();
    assert_eq!(list.option_count(), 0);
    assert_eq!(list.highlighted(), None);
}

#[test]
fn remove_all_options_via_id() {
    let mut list = sample_list();
    list.remove_option("0").unwrap();
    list.remove_option("1").unwrap();
    assert_eq!(list.option_count(), 0);
    assert_eq!(list.highlighted(), None);
}

#[test]
fn remove_invalid_id() {
    let mut list = sample_list();
    assert_eq!(
        list.remove_option("does-not-exist"),
        Err(OptionListError::UnknownId(OptionId::new("does-not-exist")))
    );
}

#[test]
fn remove_invalid_index() {
    let mut list = sample_list();
    assert_eq!(
        list.remove_option_at_index(23),
        Err(OptionListError::IndexOutOfBounds(23))
    );
}

#[test]
fn remove_with_hover_on_last_option() {
    // Python issue #3270: removal drops the mouse-hover state.
    let mut list = sample_list();
    Widget::on_layout(&mut list, 40, 10);
    assert!(list.on_mouse_move(2, 1));
    assert_eq!(list.hovered_index(), Some(1));
    list.remove_option_at_index(0).unwrap();
    assert_eq!(list.hovered_index(), None);
}

// ── test_option_prompt_replacement.py ────────────────────────────────────

#[test]
fn replace_option_prompt_with_invalid_id() {
    let mut list = sample_list();
    assert_eq!(
        list.replace_option_prompt("does-not-exist", "new-prompt"),
        Err(OptionListError::UnknownId(OptionId::new("does-not-exist")))
    );
}

#[test]
fn replace_option_prompt_with_invalid_index() {
    let mut list = sample_list();
    assert_eq!(
        list.replace_option_prompt_at_index(23, "new-prompt"),
        Err(OptionListError::IndexOutOfBounds(23))
    );
}

#[test]
fn replace_option_prompt_with_valid_id() {
    let mut list = sample_list();
    list.replace_option_prompt("0", "new-prompt").unwrap();
    assert_eq!(
        list.get_option_by_id("0").unwrap().prompt(),
        Some("new-prompt")
    );
}

#[test]
fn replace_option_prompt_with_valid_index() {
    let mut list = sample_list();
    list.replace_option_prompt_at_index(1, "new-prompt").unwrap();
    assert_eq!(
        list.get_option_at_index(1).unwrap().prompt(),
        Some("new-prompt")
    );
}

#[test]
fn replace_single_line_option_prompt_with_multiple() {
    let mut list = sample_list();
    list.replace_option_prompt("0", "new-prompt\nsecond line")
        .unwrap();
    assert_eq!(
        list.get_option_by_id("0").unwrap().prompt(),
        Some("new-prompt\nsecond line")
    );
}

#[test]
fn replace_multiple_line_option_prompt_with_single() {
    let mut list = OptionList::with_items(vec![
        OptionItem::with_id("0", "0"),
        OptionItem::new("line1\nline2"),
    ]);
    list.replace_option_prompt_at_index(1, "new-prompt").unwrap();
    assert_eq!(
        list.get_option_at_index(1).unwrap().prompt(),
        Some("new-prompt")
    );
}

#[test]
fn replace_prompt_clears_rich_content() {
    // Python `_set_prompt` replaces the whole visual; the Rust equivalent
    // clears the rich content so the new prompt is what renders.
    let mut list =
        OptionList::with_items(vec![OptionItem::rich_with_id("label", rich_rs::Text::plain("x"), "r")]);
    list.replace_option_prompt("r", "plain now").unwrap();
    let item = list.get_option_by_id("r").unwrap();
    assert_eq!(item.prompt(), Some("plain now"));
    assert!(item.content().is_none());
}

// ── test_option_list_disabled.py ─────────────────────────────────────────

fn disabled_fixture(disabled: bool) -> OptionList {
    OptionList::with_items(
        (0..100)
            .map(|n| {
                if disabled {
                    OptionItem::disabled_with_id(n.to_string(), n.to_string())
                } else {
                    OptionItem::with_id(n.to_string(), n.to_string())
                }
            })
            .collect(),
    )
}

#[test]
fn default_enabled_and_disabled() {
    let list = disabled_fixture(false);
    for n in 0..list.option_count() {
        assert!(!list.get_option_at_index(n).unwrap().is_disabled());
    }
    let list = disabled_fixture(true);
    for n in 0..list.option_count() {
        assert!(list.get_option_at_index(n).unwrap().is_disabled());
    }
}

#[test]
fn enabled_to_disabled_via_index_and_back() {
    let mut list = disabled_fixture(false);
    for n in 0..list.option_count() {
        assert!(!list.get_option_at_index(n).unwrap().is_disabled());
        list.disable_option_at_index(n).unwrap();
        assert!(list.get_option_at_index(n).unwrap().is_disabled());
    }
    for n in 0..list.option_count() {
        list.enable_option_at_index(n).unwrap();
        assert!(!list.get_option_at_index(n).unwrap().is_disabled());
    }
}

#[test]
fn enabled_to_disabled_via_id_and_back() {
    let mut list = disabled_fixture(false);
    for n in 0..list.option_count() {
        let id = n.to_string();
        assert!(!list.get_option_by_id(&id).unwrap().is_disabled());
        list.disable_option(&id).unwrap();
        assert!(list.get_option_by_id(&id).unwrap().is_disabled());
        list.enable_option(&id).unwrap();
        assert!(!list.get_option_by_id(&id).unwrap().is_disabled());
    }
}

#[test]
fn enable_disable_invalid_id_and_index() {
    let mut list = disabled_fixture(true);
    assert_eq!(
        list.disable_option("does-not-exist"),
        Err(OptionListError::UnknownId(OptionId::new("does-not-exist")))
    );
    assert_eq!(
        list.enable_option("does-not-exist"),
        Err(OptionListError::UnknownId(OptionId::new("does-not-exist")))
    );
    assert_eq!(
        list.disable_option_at_index(4242),
        Err(OptionListError::IndexOutOfBounds(4242))
    );
    assert_eq!(
        list.enable_option_at_index(4242),
        Err(OptionListError::IndexOutOfBounds(4242))
    );
}

#[test]
fn disabling_highlighted_option_moves_highlight_to_next_enabled() {
    // Python `_set_option_disabled`: disabling the highlighted option moves
    // the highlight via `find_next_enabled` (wrapping).
    let mut list = OptionList::with_items(vec![
        OptionItem::with_id("a", "a"),
        OptionItem::with_id("b", "b"),
    ]);
    assert_eq!(list.highlighted(), Some(0));
    list.disable_option("a").unwrap();
    assert_eq!(list.highlighted(), Some(1));
}

// ── selection_list/test_selection_values.py ──────────────────────────────

#[test]
fn selection_empty_selected() {
    let list: SelectionList<i32> =
        SelectionList::with_selections((0..50).map(|n| Selection::new(n.to_string(), n)).collect());
    assert!(list.selected_values().is_empty());
}

#[test]
fn selection_removal_of_selected_item() {
    let mut list: SelectionList<i32> =
        SelectionList::with_selections((0..50).map(|n| Selection::new(n.to_string(), n)).collect());
    let mut ctx = EventCtx::default();
    {
        let mut w = WidgetCtx::__from_dispatch(NodeId::default(), &mut ctx);
        list.toggle(0, &mut w);
    }
    assert_eq!(list.selected_values(), vec![&0]);
    list.remove_option_at_index(0).unwrap();
    assert!(list.selected_values().is_empty());
}

#[test]
fn selection_removal_reindexes_selected_order() {
    let mut list: SelectionList<i32> =
        SelectionList::with_selections((0..5).map(|n| Selection::new(n.to_string(), n)).collect());
    let mut ctx = EventCtx::default();
    {
        let mut w = WidgetCtx::__from_dispatch(NodeId::default(), &mut ctx);
        list.toggle(2, &mut w);
        list.toggle(4, &mut w);
    }
    assert_eq!(list.selected_values(), vec![&2, &4]);
    list.remove_option_at_index(0).unwrap();
    // Indices shifted down by one; selected values are unchanged.
    assert_eq!(list.selected_values(), vec![&2, &4]);
    assert_eq!(list.selected(), vec![1, 3]);
}

// ── selection_list/test_selection_list_create.py ─────────────────────────

fn selection_fixture() -> SelectionList<i32> {
    SelectionList::with_selections(vec![
        Selection::new("0", 0),
        Selection::new("1", 1),
        Selection::selected("2", 2),
        Selection::new("3", 3).with_id("3"),
        Selection::selected("4", 4).with_id("4"),
    ])
}

#[test]
fn all_parameters_become_selections() {
    let list = selection_fixture();
    assert_eq!(list.item_count(), 5);
    for n in 0..5 {
        assert!(list.get_option_at_index(n).is_ok());
    }
}

#[test]
fn get_selection_by_index() {
    let list = selection_fixture();
    for n in 0..5 {
        assert_eq!(
            list.get_option_at_index(n).unwrap().prompt(),
            Some(n.to_string().as_str())
        );
    }
}

#[test]
fn get_selection_by_id() {
    let list = selection_fixture();
    assert_eq!(list.get_option_by_id("3").unwrap().prompt(), Some("3"));
    assert_eq!(list.get_option_by_id("4").unwrap().prompt(), Some("4"));
}

#[test]
fn selection_add_later() {
    let mut list = selection_fixture();
    assert_eq!(list.item_count(), 5);
    list.add_selection(Selection::new("5", 5)).unwrap();
    assert_eq!(list.item_count(), 6);
    list.add_selection(Selection::new("6", 6)).unwrap();
    assert_eq!(list.item_count(), 7);
    list.add_selections(vec![
        Selection::new("7", 7),
        Selection::selected("8", 8),
        Selection::new("9", 9),
        Selection::selected("10", 10),
    ])
    .unwrap();
    assert_eq!(list.item_count(), 11);
    list.add_selections(Vec::new()).unwrap();
    assert_eq!(list.item_count(), 11);
}

#[test]
fn selection_add_later_selected_state() {
    let mut list = selection_fixture();
    assert_eq!(list.selected_values(), vec![&2, &4]);
    list.add_selection(Selection::selected("5", 5)).unwrap();
    assert_eq!(list.selected_values(), vec![&2, &4, &5]);
    list.add_selection(Selection::selected("6", 6)).unwrap();
    assert_eq!(list.selected_values(), vec![&2, &4, &5, &6]);
}

#[test]
fn selection_add_duplicate_id_is_rejected() {
    let mut list = selection_fixture();
    assert_eq!(
        list.add_selection(Selection::new("dupe", 99).with_id("3")),
        Err(OptionListError::DuplicateId(OptionId::new("3")))
    );
    assert_eq!(list.item_count(), 5);
    // Batch atomicity holds at the wrapper level too: values stay in sync.
    assert!(list
        .add_selections(vec![
            Selection::new("x", 100).with_id("x"),
            Selection::new("dupe", 101).with_id("4"),
        ])
        .is_err());
    assert_eq!(list.item_count(), 5);
    assert_eq!(list.value_at(4), Some(&4));
    assert!(list.value_at(5).is_none());
}

#[test]
fn selection_clear_options() {
    let mut list = selection_fixture();
    list.clear_options();
    assert!(list.selected_values().is_empty());
    assert_eq!(list.item_count(), 0);
    assert!(list.value_at(0).is_none());
}

#[test]
fn selection_options_are_available_soon() {
    let list: SelectionList<i32> =
        SelectionList::with_selections(vec![Selection::new("", 0).with_id("some_id")]);
    assert!(list.get_option_by_id("some_id").is_ok());
}

#[test]
fn selection_removing_option_updates_indexes() {
    let mut list = selection_fixture();
    for n in 0..5 {
        assert_eq!(list.value_at(n), Some(&(n as i32)));
    }
    list.remove_option_at_index(0).unwrap();
    for n in 0..4 {
        assert_eq!(list.value_at(n), Some(&(n as i32 + 1)));
    }
    // Registry follows: id "3" now resolves to index 2.
    assert_eq!(list.get_option_index("3"), Ok(2));
}

#[test]
fn selection_remove_by_id_repairs_values() {
    let mut list = selection_fixture();
    list.remove_option("3").unwrap();
    assert_eq!(list.item_count(), 4);
    assert_eq!(list.value_at(3), Some(&4));
    assert_eq!(list.get_option_index("4"), Ok(3));
}
