use textual::event::EventCtx;
use textual::prelude::OverlayScreenStack;
use textual::widgets::WidgetId;

#[test]
fn overlay_screen_stack_models_push_pop_navigation_on_message_bus() {
    let sender = WidgetId::from_u64(10);
    let first = WidgetId::from_u64(100);
    let second = WidgetId::from_u64(200);

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
