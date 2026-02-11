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

impl Default for Number {
    fn default() -> Self {
        Self::new()
    }
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

        if num.is_nan() || num.is_infinite() {
            return ValidationResult::failure("Value is not a number.");
        }

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

/// Validates that a string is a valid integer, optionally within a range.
///
/// Mirrors Python Textual's `Integer` validator: first validates as a number
/// (with optional min/max range), then ensures the value is an integer.
#[derive(Debug, Clone, Copy)]
pub struct Integer {
    minimum: Option<i64>,
    maximum: Option<i64>,
}

impl Default for Integer {
    fn default() -> Self {
        Self::new()
    }
}

impl Integer {
    pub fn new() -> Self {
        Self {
            minimum: None,
            maximum: None,
        }
    }

    pub fn minimum(mut self, value: i64) -> Self {
        self.minimum = Some(value);
        self
    }

    pub fn maximum(mut self, value: i64) -> Self {
        self.maximum = Some(value);
        self
    }
}

impl Validator for Integer {
    fn validate(&self, value: &str) -> ValidationResult {
        let value = value.trim();
        if value.is_empty() {
            return ValidationResult::failure("Must be a valid integer.");
        }

        let Ok(num) = value.parse::<i64>() else {
            return ValidationResult::failure("Must be a valid integer.");
        };

        if let Some(min) = self.minimum {
            if num < min {
                return ValidationResult::failure(format!(
                    "Must be greater than or equal to {min}."
                ));
            }
        }
        if let Some(max) = self.maximum {
            if num > max {
                return ValidationResult::failure(format!("Must be less than or equal to {max}."));
            }
        }

        ValidationResult::success()
    }
}

/// Validates that a string's length falls within a range (inclusive).
///
/// Mirrors Python Textual's `Length` validator.
#[derive(Debug, Clone, Copy)]
pub struct Length {
    minimum: Option<usize>,
    maximum: Option<usize>,
}

impl Default for Length {
    fn default() -> Self {
        Self::new()
    }
}

impl Length {
    pub fn new() -> Self {
        Self {
            minimum: None,
            maximum: None,
        }
    }

    pub fn minimum(mut self, value: usize) -> Self {
        self.minimum = Some(value);
        self
    }

    pub fn maximum(mut self, value: usize) -> Self {
        self.maximum = Some(value);
        self
    }
}

impl Validator for Length {
    fn validate(&self, value: &str) -> ValidationResult {
        let len = value.len();
        let too_short = self.minimum.is_some_and(|min| len < min);
        let too_long = self.maximum.is_some_and(|max| len > max);

        if too_short || too_long {
            let msg = match (self.minimum, self.maximum) {
                (Some(min), None) => format!("Must be longer than {min} characters."),
                (None, Some(max)) => format!("Must be shorter than {max} characters."),
                (Some(min), Some(max)) => format!("Must be between {min} and {max} characters."),
                _ => "Invalid length.".to_string(),
            };
            return ValidationResult::failure(msg);
        }

        ValidationResult::success()
    }
}

/// Validates that a string is a valid URL (has both a scheme and host).
///
/// Mirrors Python Textual's `URL` validator. Uses basic parsing without
/// external crates — checks for `scheme://host` pattern.
#[derive(Debug, Clone, Copy)]
pub struct Url;

impl Default for Url {
    fn default() -> Self {
        Self::new()
    }
}

impl Url {
    pub fn new() -> Self {
        Self
    }
}

impl Validator for Url {
    fn validate(&self, value: &str) -> ValidationResult {
        let value = value.trim();
        // Check for scheme://netloc pattern (mirrors Python's urlparse check)
        let Some((scheme, rest)) = value.split_once("://") else {
            return ValidationResult::failure("Must be a valid URL.");
        };
        if scheme.is_empty() {
            return ValidationResult::failure("Must be a valid URL.");
        }
        // netloc must be non-empty (could be followed by /, ?, #, or end)
        let netloc = rest.split(&['/', '?', '#'][..]).next().unwrap_or("");
        if netloc.is_empty() {
            return ValidationResult::failure("Must be a valid URL.");
        }
        ValidationResult::success()
    }
}

/// Validates that a string fully matches a regular expression.
///
/// Mirrors Python Textual's `Regex` validator (uses `re.fullmatch` semantics).
/// The pattern must match the entire string.
pub struct Regex {
    pattern: regex::Regex,
}

impl Regex {
    pub fn new(pattern: regex::Regex) -> Self {
        Self { pattern }
    }

    /// Create from a pattern string. Panics if the pattern is invalid.
    pub fn compile(pattern: &str) -> Self {
        Self {
            pattern: regex::Regex::new(pattern).expect("invalid regex pattern"),
        }
    }

    /// Try to create from a pattern string, returning `None` if invalid.
    pub fn try_compile(pattern: &str) -> Option<Self> {
        regex::Regex::new(pattern).ok().map(|r| Self { pattern: r })
    }
}

impl std::fmt::Debug for Regex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Regex")
            .field("pattern", &self.pattern.as_str())
            .finish()
    }
}

impl Validator for Regex {
    fn validate(&self, value: &str) -> ValidationResult {
        // fullmatch semantics: the pattern must match the entire string
        if self
            .pattern
            .find(value)
            .is_some_and(|m| m.start() == 0 && m.end() == value.len())
        {
            ValidationResult::success()
        } else {
            ValidationResult::failure(format!(
                "Must match regular expression {:?}.",
                self.pattern.as_str()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_valid() {
        let v = Integer::new();
        assert!(v.validate("42").is_valid);
        assert!(v.validate("-7").is_valid);
        assert!(v.validate("0").is_valid);
    }

    #[test]
    fn integer_invalid() {
        let v = Integer::new();
        assert!(!v.validate("3.5").is_valid);
        assert!(!v.validate("abc").is_valid);
        assert!(!v.validate("").is_valid);
    }

    #[test]
    fn integer_range() {
        let v = Integer::new().minimum(0).maximum(100);
        assert!(v.validate("50").is_valid);
        assert!(!v.validate("-1").is_valid);
        assert!(!v.validate("101").is_valid);
    }

    #[test]
    fn length_valid() {
        let v = Length::new().minimum(2).maximum(5);
        assert!(v.validate("ab").is_valid);
        assert!(v.validate("abcde").is_valid);
    }

    #[test]
    fn length_invalid() {
        let v = Length::new().minimum(2).maximum(5);
        assert!(!v.validate("a").is_valid);
        assert!(!v.validate("abcdef").is_valid);
    }

    #[test]
    fn length_unbounded_min() {
        let v = Length::new().maximum(3);
        assert!(v.validate("").is_valid);
        assert!(v.validate("abc").is_valid);
        assert!(!v.validate("abcd").is_valid);
    }

    #[test]
    fn length_unbounded_max() {
        let v = Length::new().minimum(2);
        assert!(!v.validate("a").is_valid);
        assert!(v.validate("ab").is_valid);
        assert!(v.validate("abcdef").is_valid);
    }

    #[test]
    fn url_valid() {
        let v = Url::new();
        assert!(v.validate("https://example.com").is_valid);
        assert!(v.validate("http://foo.bar/path").is_valid);
        assert!(v.validate("ftp://files.example.com").is_valid);
    }

    #[test]
    fn url_invalid() {
        let v = Url::new();
        assert!(!v.validate("not-a-url").is_valid);
        assert!(!v.validate("://missing-scheme").is_valid);
        assert!(!v.validate("http://").is_valid);
        assert!(!v.validate("").is_valid);
    }

    #[test]
    fn regex_valid() {
        let v = Regex::compile(r"\d{3}-\d{4}");
        assert!(v.validate("123-4567").is_valid);
    }

    #[test]
    fn regex_invalid() {
        let v = Regex::compile(r"\d{3}-\d{4}");
        assert!(!v.validate("12-4567").is_valid);
        assert!(!v.validate("abc").is_valid);
    }

    #[test]
    fn regex_fullmatch_semantics() {
        // Pattern should match the entire string, not just a substring
        let v = Regex::compile(r"\d+");
        assert!(v.validate("123").is_valid);
        assert!(!v.validate("123abc").is_valid);
        assert!(!v.validate("abc123").is_valid);
    }

    #[test]
    fn regex_try_compile_invalid() {
        assert!(Regex::try_compile("[invalid").is_none());
    }

    #[test]
    fn number_rejects_nan_inf() {
        let v = Number::new();
        assert!(!v.validate("NaN").is_valid);
        assert!(!v.validate("inf").is_valid);
        assert!(!v.validate("-inf").is_valid);
        assert!(!v.validate("infinity").is_valid);
    }
}
