//! TypeId-dispatched message handler registry for app-level typed hooks.
//!
//! This is a convenience layer over the same message bus that drives
//! `Widget::on_message` — not a separate dispatch path.
//! Registration lives on [`crate::textual_app::TextualApp`] via
//! [`TextualApp::register_message_handlers`].

use std::any::{Any, TypeId};

use crate::event::EventCtx;
use crate::message::{MessageEvent, Msg};
use crate::node_id::NodeId;

/// Sender metadata handed to typed handlers alongside the typed payload.
#[derive(Debug, Clone, Copy)]
pub struct MessageContext {
    pub sender: NodeId,
    pub control: Option<NodeId>,
}

type HandlerFn<A> = Box<dyn FnMut(&mut A, &dyn Any, &MessageContext, &mut EventCtx) + Send + Sync>;

/// Registry of TypeId-dispatched message handlers operating on app state `A`.
///
/// Register handlers with [`on`][MessageHandlers::on], then call
/// [`dispatch`][MessageHandlers::dispatch] inside the app adapter's
/// `on_message` hook.
pub struct MessageHandlers<A: ?Sized> {
    entries: Vec<(TypeId, HandlerFn<A>)>,
}

impl<A> Default for MessageHandlers<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A> MessageHandlers<A> {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Register a typed handler for message type `T`.
    ///
    /// All handlers registered for a given type run in registration order
    /// when a matching message is dispatched.
    pub fn on<T, F>(&mut self, mut handler: F)
    where
        T: Msg,
        F: FnMut(&mut A, &T, &MessageContext, &mut EventCtx) + Send + Sync + 'static,
    {
        self.entries.push((
            TypeId::of::<T>(),
            Box::new(move |app, any, mctx, ctx| {
                if let Some(msg) = any.downcast_ref::<T>() {
                    handler(app, msg, mctx, ctx);
                }
            }),
        ));
    }

    /// Dispatch one message event. All handlers registered for the payload's
    /// concrete type run, in registration order. Returns `true` if any ran.
    pub fn dispatch(&mut self, app: &mut A, event: &MessageEvent, ctx: &mut EventCtx) -> bool {
        let type_id = event.payload_type_id();
        let mctx = MessageContext {
            sender: event.sender,
            control: event.control,
        };
        let mut ran = false;
        for (entry_id, f) in &mut self.entries {
            if *entry_id == type_id {
                f(app, event.payload().as_any(), &mctx, ctx);
                ran = true;
            }
        }
        ran
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ButtonPressed, CheckboxChanged};
    use crate::node_id::node_id_from_ffi;

    fn make_event<M: Msg>(msg: M) -> MessageEvent {
        MessageEvent::new(node_id_from_ffi(1), msg)
    }

    struct State {
        button_count: u32,
        checkbox_value: Option<bool>,
        last_mctx_sender: Option<NodeId>,
    }

    impl State {
        fn new() -> Self {
            Self {
                button_count: 0,
                checkbox_value: None,
                last_mctx_sender: None,
            }
        }
    }

    #[test]
    fn handler_runs_for_matching_type() {
        let mut handlers: MessageHandlers<State> = MessageHandlers::new();
        handlers.on::<ButtonPressed, _>(|state, _msg, _mctx, _ctx| {
            state.button_count += 1;
        });

        let mut state = State::new();
        let event = make_event(ButtonPressed {
            description: "ok".into(),
            button_id: None,
        });
        let mut ctx = EventCtx::default();
        let ran = handlers.dispatch(&mut state, &event, &mut ctx);
        assert!(ran);
        assert_eq!(state.button_count, 1);
    }

    #[test]
    fn non_matching_type_not_run() {
        let mut handlers: MessageHandlers<State> = MessageHandlers::new();
        handlers.on::<ButtonPressed, _>(|state, _msg, _mctx, _ctx| {
            state.button_count += 1;
        });

        let mut state = State::new();
        let event = make_event(CheckboxChanged { checked: true });
        let mut ctx = EventCtx::default();
        let ran = handlers.dispatch(&mut state, &event, &mut ctx);
        assert!(!ran);
        assert_eq!(state.button_count, 0);
    }

    #[test]
    fn two_handlers_for_same_type_both_run_in_order() {
        let mut handlers: MessageHandlers<State> = MessageHandlers::new();
        handlers.on::<ButtonPressed, _>(|state, _msg, _mctx, _ctx| {
            state.button_count += 1;
        });
        handlers.on::<ButtonPressed, _>(|state, _msg, _mctx, _ctx| {
            state.button_count += 10;
        });

        let mut state = State::new();
        let event = make_event(ButtonPressed {
            description: "ok".into(),
            button_id: None,
        });
        let mut ctx = EventCtx::default();
        handlers.dispatch(&mut state, &event, &mut ctx);
        // Both ran in registration order: 1 then +10 = 11.
        assert_eq!(state.button_count, 11);
    }

    #[test]
    fn message_context_carries_sender_and_control() {
        let mut handlers: MessageHandlers<State> = MessageHandlers::new();
        handlers.on::<ButtonPressed, _>(|state, _msg, mctx, _ctx| {
            state.last_mctx_sender = Some(mctx.sender);
        });

        let mut state = State::new();
        let sender = node_id_from_ffi(42);
        let event = MessageEvent::new(
            sender,
            ButtonPressed {
                description: "hi".into(),
                button_id: None,
            },
        );
        let mut ctx = EventCtx::default();
        handlers.dispatch(&mut state, &event, &mut ctx);
        assert_eq!(state.last_mctx_sender, Some(sender));
    }

    #[test]
    fn dispatch_returns_false_when_nothing_matches() {
        let mut handlers: MessageHandlers<State> = MessageHandlers::new();
        handlers.on::<CheckboxChanged, _>(|state, msg, _mctx, _ctx| {
            state.checkbox_value = Some(msg.checked);
        });

        let mut state = State::new();
        let event = make_event(ButtonPressed {
            description: "x".into(),
            button_id: None,
        });
        let mut ctx = EventCtx::default();
        let ran = handlers.dispatch(&mut state, &event, &mut ctx);
        assert!(!ran);
        assert!(state.checkbox_value.is_none());
    }
}
