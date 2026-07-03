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

    assert!({ let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx); stack.push(sender, first, &mut __w) });
    assert!({ let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx); stack.push(sender, second, &mut __w) });
    assert_eq!(stack.len(), 2);
    assert_eq!(stack.current(), Some(second));
    assert_eq!({ let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx); stack.pop(sender, &mut __w) }, Some(second));
    assert_eq!(stack.current(), Some(first));
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx); stack.clear(sender, &mut __w) };
    assert!(stack.is_empty());
}
