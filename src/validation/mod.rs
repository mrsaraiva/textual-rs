use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub failure_descriptions: Vec<String>,
}

impl ValidationResult {
    pub fn success() -> Self {
        Self {
            is_valid: true,
            failure_descriptions: Vec::new(),
        }
    }

    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            is_valid: false,
            failure_descriptions: vec![message.into()],
        }
    }
}

pub trait Validator {
    fn validate(&self, value: &str) -> ValidationResult;
}

pub type ValidatorRef = Arc<dyn Validator + Send + Sync>;

pub struct Function {
    predicate: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    message: String,
}

impl Function {
    pub fn new(
        predicate: impl Fn(&str) -> bool + Send + Sync + 'static,
        message: impl Into<String>,
    ) -> Self {
        Self {
            predicate: Arc::new(predicate),
            message: message.into(),
        }
    }
}

impl Validator for Function {
    fn validate(&self, value: &str) -> ValidationResult {
        if (self.predicate)(value) {
            ValidationResult::success()
        } else {
            ValidationResult::failure(self.message.clone())
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Number {
    minimum: Option<f64>,
    maximum: Option<f64>,
}

impl Number {
    pub fn new() -> Self {
        Self {
            minimum: None,
            maximum: None,
        }
    }

    pub fn minimum(mut self, value: f64) -> Self {
        self.minimum = Some(value);
        self
    }

    pub fn maximum(mut self, value: f64) -> Self {
        self.maximum = Some(value);
        self
    }
}

impl Validator for Number {
    fn validate(&self, value: &str) -> ValidationResult {
        let value = value.trim();
        if value.is_empty() {
            return ValidationResult::failure("Value is required.");
        }

        let Ok(num) = value.parse::<f64>() else {
            return ValidationResult::failure("Value is not a number.");
        };

        if let Some(min) = self.minimum {
            if num < min {
                return ValidationResult::failure(format!("Value must be >= {min}."));
            }
        }
        if let Some(max) = self.maximum {
            if num > max {
                return ValidationResult::failure(format!("Value must be <= {max}."));
            }
        }

        ValidationResult::success()
    }
}

