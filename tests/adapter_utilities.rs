use textual::event::EventCtx;
use textual::node_id_from_ffi;
use textual::prelude::OverlayScreenStack;

#[test]
fn overlay_screen_stack_models_push_pop_navigation_on_message_bus() {
    let sender = node_id_from_ffi(10);
    let first = node_id_from_ffi(100);
    let second = node_id_from_ffi(200);

    let mut stack = OverlayScreenStack::new();
    let mut ctx = EventCtx::default();

    assert!(stack.push(sender, first, &mut ctx));
    assert!(stack.push(sender, second, &mut ctx));
    assert_eq!(stack.len(), 2);
    assert_eq!(stack.current(), Some(second));
    assert_eq!(stack.pop(sender, &mut ctx), Some(second));
    assert_eq!(stack.current(), Some(first));
    stack.clear(sender, &mut ctx);
    assert!(stack.is_empty());
}
