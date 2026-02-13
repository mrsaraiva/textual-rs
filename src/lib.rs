//! Textual-inspired reactive TUI framework built on rich-rs.

pub mod action;
pub mod animation;
pub mod compose;
pub mod css;
pub mod debug;
pub mod layout;
pub mod demo_snapshot;
pub mod driver;
mod error;
pub mod event;
pub mod keys;
pub mod message;
pub mod node_id;
pub mod reactive;
pub mod render;
pub mod runtime;
pub mod screen;
pub mod signal;
pub mod style;
pub mod textual_app;
pub mod validation;
pub mod widget_tree;
pub mod widgets;
pub mod worker;

pub use error::{Error, Result};
pub use event::BindingHint;
pub use widgets::BindingDecl;
pub use keys::KeyEventData;
pub use node_id::{NodeId, node_id_from_ffi, node_id_to_ffi};
pub use runtime::App;
pub use screen::{Screen, ScreenResult, ScreenResultCallback, ScreenStack};
pub use style::{Color, Style, Theme};
pub use textual_app::{
    OverlayScreenStack, TextualApp, run, run_snapshot, run_snapshot_with_output, run_sync,
    run_sync_snapshot, run_sync_snapshot_with_output, run_sync_with_output, run_textual_app,
    run_textual_app_or_snapshot, run_textual_app_or_snapshot_with_output,
    run_textual_app_with_output,
};
pub use widgets::WidgetStyles;
pub use textual_macros::Reactive;

pub mod prelude {
    pub use crate::animation::{Animator, animation_level_from_env};
    pub use crate::compose::{ChildDecl, ComposeResult, WidgetBuilder};
    pub use crate::css::{StyleSelector, StyleSheet, set_style_context};
    pub use crate::debug::DebugLayout;
    pub use crate::action::{ActionDecl, ActionHandler, ParsedAction, parse_action};
    pub use crate::event::{
        Action, ActionMap, AnimationEase, BindingHint, ClickEvent, Event, EventCtx, KeyBind,
        MouseEnterEvent, MouseLeaveEvent, PasteEvent, WidgetCtx,
    };
    pub use crate::keys::{KeyEventData, format_key_display, key_to_identifier};
    pub use crate::message::*;
    pub use crate::runtime::App;
    pub use crate::style::{Color, Style, Theme};
    pub use crate::textual_app::{
        OverlayScreenStack, TextualApp, run, run_snapshot, run_snapshot_with_output, run_sync,
        run_sync_snapshot, run_sync_snapshot_with_output, run_sync_with_output, run_textual_app,
        run_textual_app_or_snapshot, run_textual_app_or_snapshot_with_output,
        run_textual_app_with_output,
    };
    pub use crate::validation::{
        Function, Integer, Length, Number, Regex, Url, ValidationResult, Validator, ValidatorRef,
    };
    pub use crate::widgets::{
        AppRoot, BindingsTable, Button, ButtonVariant, Center, CenterMiddle, Checkbox, Collapsible,
        CommandPalette, CommandPaletteScreen, Constrained, Container, ContentSwitcher, CursorType, DataTable, Digits,
        DigitsAlign, DirectoryTree, Dock, Footer, FooterBinding, Frame, FuzzyMatcher, Grid, Header, HelpPanel,
        Horizontal, HorizontalGroup, HorizontalScroll, Input, InputType, ItemGrid, KeyPanel, Label, LabelVariant,
        SuggestFromList, Suggester,
        LayoutConstraints, LineStyle, Link, ListView, LoadingIndicator, Log, Markdown, MaskedInput,
        Middle, Node, OptionItem, OptionList, Overlay, PaletteCommand, Panel, Placeholder,
        PlaceholderVariant, Pretty, ProgressBar, RadioButton, RadioSet, RichLog, Right, Row,
        RowAlign, Rule, RuleOrientation, ScrollView, ScrollableContainer, Select, Selection,
        SelectionList, SelectionListString, Spacer, Sparkline, Static, Styled, SummaryFunction,
        Switch, SystemModalScreen, TabPane,
        TabbedContent, Tabs, TextArea, TextAreaCursor, TextAreaSelection, TextAreaTheme, Toast,
        ToastSeverity, Tooltip, Tree, TreeNode, Vertical, VerticalGroup, VerticalScroll, Welcome,
        BindingDecl, StyleChangeKind, Widget, WidgetRenderable, WidgetStyles, classify_style_change,
        preview_root, preview_root_with_bottom,
        preview_root_with_top_bottom, summary_max, summary_mean, summary_min,
    };
    pub use crate::node_id::{NodeId, node_id_from_ffi, node_id_to_ffi};
    pub use crate::screen::{Screen, ScreenResult, ScreenResultCallback, ScreenStack};
    pub use crate::signal::{Signal, SignalResponse};
    pub use crate::widget_tree::{LifecycleEvent, QueryError, WidgetNode, WidgetTree};
    pub use crate::worker::{CancellationToken, WorkerId, WorkerRegistry, WorkerRequest, WorkerState};
    pub use crate::{Error, Result};
}
