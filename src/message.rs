use crate::widgets::WidgetId;
use crate::validation::ValidationResult;

#[derive(Debug, Clone)]
pub enum Message {
    InputChanged { value: String, validation: ValidationResult },
    InputSubmitted { value: String },
    CheckboxChanged { checked: bool },
}

#[derive(Debug, Clone)]
pub struct MessageEvent {
    pub sender: WidgetId,
    pub message: Message,
}
