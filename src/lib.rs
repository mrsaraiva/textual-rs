//! Textual-inspired reactive TUI framework built on rich-rs.

mod error;
pub mod driver;
pub mod debug;
pub mod event;
pub mod render;
pub mod runtime;
pub mod widget;

pub use error::{Error, Result};
pub use runtime::App;

pub mod prelude {
    pub use crate::runtime::App;
    pub use crate::debug::DebugLayout;
    pub use crate::event::{Action, ActionMap, Event, EventCtx, KeyBind};
    pub use crate::widget::{
        AppRoot, Button, Checkbox, Container, DataTable, Dock, Frame, Grid, Input, Label, ListView,
        Markdown, Overlay, Row, ScrollView, Tabs, Tree, TreeNode, Widget, WidgetId, WidgetRenderable,
    };
    pub use crate::{Error, Result};
}
