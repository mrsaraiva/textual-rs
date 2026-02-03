use textual::event::{Action, ActionMap, KeyBind};
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn action_map_binds_and_resolves() {
    let mut map = ActionMap::new();
    let bind = KeyBind::new(KeyCode::Char('j'), KeyModifiers::empty());
    map.bind(bind, Action::ScrollDown);
    assert_eq!(map.lookup(&bind), Some(Action::ScrollDown));
}
