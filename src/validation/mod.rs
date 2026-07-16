//! Input validation framework, ported from Python Textual's `validation.py`.
//!
//! Mirrors Python's model: a [`Validator`] produces a [`ValidationResult`]
//! holding a list of [`Failure`]s. Each failure carries a human-readable
//! description resolved through a priority ladder (Python
//! `Failure.__post_init__`):
//!
//! 1. an explicit description set inside `validate()` on the `Failure` itself,
//! 2. else the validator's `failure_description` (constructor override),
//! 3. else the validator's `describe_failure(failure)` hook.

use std::sync::Arc;

/// Categorizes why a validation failed.
///
/// Mirrors Python Textual's `Failure` subclasses (`Number.NotANumber`,
/// `Length.Incorrect`, `Regex.NoResults`, ...) so `describe_failure` can
/// produce a different message per failure reason.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FailureKind {
    /// A generic failure (Python's base `Failure`).
    #[default]
    Generic,
    /// Python `Number.NotANumber`: the value is not a valid number.
    NotANumber,
    /// Python `Number.NotInRange`: the number is outside `[minimum, maximum]`.
    NotInRange,
    /// Python `Integer.NotAnInteger`: the value is a number but not an integer.
    NotAnInteger,
    /// Python `Length.Incorrect`: the value's length is outside the range.
    IncorrectLength,
    /// Python `Regex.NoResults`: the regex did not match the value.
    NoResults,
    /// Python `Function.ReturnedFalse`: the supplied function returned false.
    ReturnedFalse,
    /// Python `URL.InvalidURL`: the URL is not valid.
    InvalidUrl,
    /// A custom failure kind for user-defined validators.
    Custom(String),
}

/// Information about a validation failure. Port of Python's `Failure`.
///
/// The description follows the priority ladder documented at the module level;
/// it is resolved when the failure passes through
/// [`Validator::resolve_failure`] (called by [`Validator::failure`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Failure {
    /// Why the validation failed (used by `describe_failure` dispatch).
    pub kind: FailureKind,
    /// The value which resulted in validation failing.
    pub value: Option<String>,
    /// The human-readable description of this failure. An explicit value set
    /// before the ladder runs takes precedence over any validator messaging.
    pub description: Option<String>,
}

impl Failure {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_kind(mut self, kind: FailureKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// The result of calling a `Validator::validate` method.
///
/// Port of Python's `ValidationResult`: a list of failures, empty on success.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationResult {
    /// The reasons why the value was invalid. Empty if the value was valid.
    pub failures: Vec<Failure>,
}

impl ValidationResult {
    /// Construct a successful ValidationResult.
    pub fn success() -> Self {
        Self::default()
    }

    /// Construct a failure ValidationResult from a list of failures.
    pub fn failure(failures: Vec<Failure>) -> Self {
        Self { failures }
    }

    /// Merge multiple ValidationResult objects into one (Python
    /// `ValidationResult.merge`): valid only if all inputs are valid, with
    /// all failures aggregated in order.
    pub fn merge(results: impl IntoIterator<Item = ValidationResult>) -> Self {
        Self {
            failures: results
                .into_iter()
                .flat_map(|result| result.failures)
                .collect(),
        }
    }

    /// True if the validation was successful.
    pub fn is_valid(&self) -> bool {
        self.failures.is_empty()
    }

    /// Utility for extracting failure descriptions as strings, skipping
    /// failures without a description (Python
    /// `ValidationResult.failure_descriptions`).
    pub fn failure_descriptions(&self) -> Vec<String> {
        self.failures
            .iter()
            .filter_map(|failure| failure.description.clone())
            .collect()
    }
}

/// Base trait for the validation of string values.
///
/// Port of Python's `Validator` ABC. Implement `validate`, returning
/// `self.success()` or `self.failure(...)`; the latter resolves the failure
/// description through the priority ladder.
pub trait Validator {
    /// Validate the value and return a ValidationResult describing the outcome.
    fn validate(&self, value: &str) -> ValidationResult;

    /// An explicit, user-supplied description of why validation failed
    /// (Python's `failure_description` constructor argument). Takes priority
    /// over `describe_failure`, but not over a description set on the
    /// `Failure` inside `validate()`.
    fn failure_description(&self) -> Option<String> {
        None
    }

    /// Return a string description of the Failure (Python's
    /// `describe_failure` hook). Only consulted if no explicit description was
    /// supplied on the `Failure` or via `failure_description`.
    fn describe_failure(&self, _failure: &Failure) -> Option<String> {
        None
    }

    /// Shorthand for a successful result.
    fn success(&self) -> ValidationResult {
        ValidationResult::success()
    }

    /// Shorthand for a failed result carrying one failure, with its
    /// description resolved through the priority ladder.
    fn failure(&self, failure: Failure) -> ValidationResult {
        ValidationResult::failure(vec![self.resolve_failure(failure)])
    }

    /// Resolve the failure's description through the priority ladder
    /// (Python `Failure.__post_init__`): explicit description on the failure,
    /// else `failure_description`, else `describe_failure`.
    fn resolve_failure(&self, mut failure: Failure) -> Failure {
        if failure.description.is_none() {
            failure.description = self
                .failure_description()
                .or_else(|| self.describe_failure(&failure));
        }
        failure
    }
}

pub type ValidatorRef = Arc<dyn Validator + Send + Sync>;

/// A flexible validator which allows you to provide custom validation logic.
///
/// Mirrors Python Textual's `Function` validator: the failure message is the
/// `failure_description` supplied at construction (there is no built-in
/// default description).
pub struct Function {
    predicate: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    failure_description: Option<String>,
}

impl Function {
    pub fn new(
        predicate: impl Fn(&str) -> bool + Send + Sync + 'static,
        message: impl Into<String>,
    ) -> Self {
        Self {
            predicate: Arc::new(predicate),
            failure_description: Some(message.into()),
        }
    }
}

impl Validator for Function {
    fn validate(&self, value: &str) -> ValidationResult {
        if (self.predicate)(value) {
            self.success()
        } else {
            self.failure(
                Failure::new()
                    .with_kind(FailureKind::ReturnedFalse)
                    .with_value(value),
            )
        }
    }

    fn failure_description(&self) -> Option<String> {
        self.failure_description.clone()
    }

    fn describe_failure(&self, _failure: &Failure) -> Option<String> {
        // Python parity: `Function.describe_failure` returns the
        // `failure_description` (there is no other default message).
        self.failure_description.clone()
    }
}

/// Formats Python Textual's shared out-of-range messages.
fn describe_not_in_range<T: std::fmt::Display>(
    minimum: Option<T>,
    maximum: Option<T>,
) -> Option<String> {
    match (minimum, maximum) {
        (None, Some(max)) => Some(format!("Must be less than or equal to {max}.")),
        (Some(min), None) => Some(format!("Must be greater than or equal to {min}.")),
        (Some(min), Some(max)) => Some(format!("Must be between {min} and {max}.")),
        (None, None) => None,
    }
}

/// Validator that ensures the value is a number, with an optional range check.
///
/// Mirrors Python Textual's `Number` validator.
#[derive(Debug, Clone, Default)]
pub struct Number {
    minimum: Option<f64>,
    maximum: Option<f64>,
    failure_description: Option<String>,
}

impl Number {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn minimum(mut self, value: f64) -> Self {
        self.minimum = Some(value);
        self
    }

    pub fn maximum(mut self, value: f64) -> Self {
        self.maximum = Some(value);
        self
    }

    /// Set an explicit failure description (Python's `failure_description`
    /// constructor argument). Takes priority over the default messages.
    pub fn with_failure_description(mut self, description: impl Into<String>) -> Self {
        self.failure_description = Some(description.into());
        self
    }

    fn validate_range(&self, value: f64) -> bool {
        if self.minimum.is_some_and(|min| value < min) {
            return false;
        }
        if self.maximum.is_some_and(|max| value > max) {
            return false;
        }
        true
    }
}

impl Validator for Number {
    fn validate(&self, value: &str) -> ValidationResult {
        let Some(num) = value.trim().parse::<f64>().ok().filter(|n| n.is_finite()) else {
            return self.failure(
                Failure::new()
                    .with_kind(FailureKind::NotANumber)
                    .with_value(value),
            );
        };
        if !self.validate_range(num) {
            return self.failure(
                Failure::new()
                    .with_kind(FailureKind::NotInRange)
                    .with_value(value),
            );
        }
        self.success()
    }

    fn failure_description(&self) -> Option<String> {
        self.failure_description.clone()
    }

    fn describe_failure(&self, failure: &Failure) -> Option<String> {
        match failure.kind {
            FailureKind::NotANumber => Some("Must be a valid number.".to_string()),
            FailureKind::NotInRange => describe_not_in_range(self.minimum, self.maximum),
            _ => None,
        }
    }
}

/// Validates that a string is a valid integer, optionally within a range.
///
/// Mirrors Python Textual's `Integer` validator: first validates as a number
/// (with optional min/max range), then ensures the value is an integer.
#[derive(Debug, Clone, Default)]
pub struct Integer {
    minimum: Option<i64>,
    maximum: Option<i64>,
    failure_description: Option<String>,
}

impl Integer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn minimum(mut self, value: i64) -> Self {
        self.minimum = Some(value);
        self
    }

    pub fn maximum(mut self, value: i64) -> Self {
        self.maximum = Some(value);
        self
    }

    /// Set an explicit failure description (Python's `failure_description`
    /// constructor argument). Takes priority over the default messages.
    pub fn with_failure_description(mut self, description: impl Into<String>) -> Self {
        self.failure_description = Some(description.into());
        self
    }
}

impl Validator for Integer {
    fn validate(&self, value: &str) -> ValidationResult {
        let trimmed = value.trim();
        // Python parity: `Integer.validate` first runs the `Number` checks
        // (float parse + range on the float), then checks integer-ness.
        let Some(num) = trimmed.parse::<f64>().ok().filter(|n| n.is_finite()) else {
            return self.failure(
                Failure::new()
                    .with_kind(FailureKind::NotANumber)
                    .with_value(value),
            );
        };
        let below = self.minimum.is_some_and(|min| num < min as f64);
        let above = self.maximum.is_some_and(|max| num > max as f64);
        if below || above {
            return self.failure(
                Failure::new()
                    .with_kind(FailureKind::NotInRange)
                    .with_value(value),
            );
        }
        if trimmed.parse::<i64>().is_err() {
            return self.failure(
                Failure::new()
                    .with_kind(FailureKind::NotAnInteger)
                    .with_value(value),
            );
        }
        self.success()
    }

    fn failure_description(&self) -> Option<String> {
        self.failure_description.clone()
    }

    fn describe_failure(&self, failure: &Failure) -> Option<String> {
        match failure.kind {
            FailureKind::NotANumber | FailureKind::NotAnInteger => {
                Some("Must be a valid integer.".to_string())
            }
            FailureKind::NotInRange => describe_not_in_range(self.minimum, self.maximum),
            _ => None,
        }
    }
}

/// Validates that a string's length falls within a range (inclusive).
///
/// Mirrors Python Textual's `Length` validator (length is counted in
/// characters, matching Python's `len(str)`).
#[derive(Debug, Clone, Default)]
pub struct Length {
    minimum: Option<usize>,
    maximum: Option<usize>,
    failure_description: Option<String>,
}

impl Length {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn minimum(mut self, value: usize) -> Self {
        self.minimum = Some(value);
        self
    }

    pub fn maximum(mut self, value: usize) -> Self {
        self.maximum = Some(value);
        self
    }

    /// Set an explicit failure description (Python's `failure_description`
    /// constructor argument). Takes priority over the default messages.
    pub fn with_failure_description(mut self, description: impl Into<String>) -> Self {
        self.failure_description = Some(description.into());
        self
    }
}

impl Validator for Length {
    fn validate(&self, value: &str) -> ValidationResult {
        let len = value.chars().count();
        let too_short = self.minimum.is_some_and(|min| len < min);
        let too_long = self.maximum.is_some_and(|max| len > max);

        if too_short || too_long {
            return self.failure(
                Failure::new()
                    .with_kind(FailureKind::IncorrectLength)
                    .with_value(value),
            );
        }
        self.success()
    }

    fn failure_description(&self) -> Option<String> {
        self.failure_description.clone()
    }

    fn describe_failure(&self, failure: &Failure) -> Option<String> {
        if failure.kind != FailureKind::IncorrectLength {
            return None;
        }
        match (self.minimum, self.maximum) {
            (None, Some(max)) => Some(format!("Must be shorter than {max} characters.")),
            (Some(min), None) => Some(format!("Must be longer than {min} characters.")),
            (Some(min), Some(max)) => Some(format!("Must be between {min} and {max} characters.")),
            (None, None) => None,
        }
    }
}

/// Validates that a string is a valid URL (has both a scheme and host).
///
/// Mirrors Python Textual's `URL` validator. Uses basic parsing without
/// external crates: checks for a `scheme://host` pattern.
#[derive(Debug, Clone, Default)]
pub struct Url {
    failure_description: Option<String>,
}

impl Url {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set an explicit failure description (Python's `failure_description`
    /// constructor argument). Takes priority over the default message.
    pub fn with_failure_description(mut self, description: impl Into<String>) -> Self {
        self.failure_description = Some(description.into());
        self
    }
}

impl Validator for Url {
    fn validate(&self, value: &str) -> ValidationResult {
        let invalid_url = || {
            self.failure(
                Failure::new()
                    .with_kind(FailureKind::InvalidUrl)
                    .with_value(value),
            )
        };
        let value = value.trim();
        // Check for scheme://netloc pattern (mirrors Python's urlparse check)
        let Some((scheme, rest)) = value.split_once("://") else {
            return invalid_url();
        };
        if scheme.is_empty() {
            return invalid_url();
        }
        // netloc must be non-empty (could be followed by /, ?, #, or end)
        let netloc = rest.split(&['/', '?', '#'][..]).next().unwrap_or("");
        if netloc.is_empty() {
            return invalid_url();
        }
        self.success()
    }

    fn failure_description(&self) -> Option<String> {
        self.failure_description.clone()
    }

    fn describe_failure(&self, _failure: &Failure) -> Option<String> {
        Some("Must be a valid URL.".to_string())
    }
}

/// Validates that a string fully matches a regular expression.
///
/// Mirrors Python Textual's `Regex` validator (uses `re.fullmatch` semantics).
/// The pattern must match the entire string.
pub struct Regex {
    pattern: regex::Regex,
    failure_description: Option<String>,
}

impl Regex {
    pub fn new(pattern: regex::Regex) -> Self {
        Self {
            pattern,
            failure_description: None,
        }
    }

    /// Create from a pattern string. Panics if the pattern is invalid.
    pub fn compile(pattern: &str) -> Self {
        Self::new(regex::Regex::new(pattern).expect("invalid regex pattern"))
    }

    /// Try to create from a pattern string, returning `None` if invalid.
    pub fn try_compile(pattern: &str) -> Option<Self> {
        regex::Regex::new(pattern).ok().map(Self::new)
    }

    /// Set an explicit failure description (Python's `failure_description`
    /// constructor argument). Takes priority over the default message.
    pub fn with_failure_description(mut self, description: impl Into<String>) -> Self {
        self.failure_description = Some(description.into());
        self
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
            self.success()
        } else {
            self.failure(
                Failure::new()
                    .with_kind(FailureKind::NoResults)
                    .with_value(value),
            )
        }
    }

    fn failure_description(&self) -> Option<String> {
        self.failure_description.clone()
    }

    fn describe_failure(&self, _failure: &Failure) -> Option<String> {
        // Python: f"Must match regular expression {self.regex!r} (flags={self.flags})."
        // With a string pattern and default flags, Python renders the repr with
        // single quotes and escaped backslashes, and flags=0.
        Some(format!(
            "Must match regular expression '{}' (flags=0).",
            self.pattern.as_str().escape_debug()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_valid() {
        let v = Integer::new();
        assert!(v.validate("42").is_valid());
        assert!(v.validate("-7").is_valid());
        assert!(v.validate("0").is_valid());
    }

    #[test]
    fn integer_invalid() {
        let v = Integer::new();
        assert!(!v.validate("3.5").is_valid());
        assert!(!v.validate("abc").is_valid());
        assert!(!v.validate("").is_valid());
    }

    #[test]
    fn integer_range() {
        let v = Integer::new().minimum(0).maximum(100);
        assert!(v.validate("50").is_valid());
        assert!(!v.validate("-1").is_valid());
        assert!(!v.validate("101").is_valid());
    }

    #[test]
    fn length_valid() {
        let v = Length::new().minimum(2).maximum(5);
        assert!(v.validate("ab").is_valid());
        assert!(v.validate("abcde").is_valid());
    }

    #[test]
    fn length_invalid() {
        let v = Length::new().minimum(2).maximum(5);
        assert!(!v.validate("a").is_valid());
        assert!(!v.validate("abcdef").is_valid());
    }

    #[test]
    fn length_unbounded_min() {
        let v = Length::new().maximum(3);
        assert!(v.validate("").is_valid());
        assert!(v.validate("abc").is_valid());
        assert!(!v.validate("abcd").is_valid());
    }

    #[test]
    fn length_unbounded_max() {
        let v = Length::new().minimum(2);
        assert!(!v.validate("a").is_valid());
        assert!(v.validate("ab").is_valid());
        assert!(v.validate("abcdef").is_valid());
    }

    #[test]
    fn length_counts_characters_not_bytes() {
        // Python parity: len("héé") == 3
        let v = Length::new().maximum(3);
        assert!(v.validate("héé").is_valid());
        assert!(!v.validate("hééé").is_valid());
    }

    #[test]
    fn url_valid() {
        let v = Url::new();
        assert!(v.validate("https://example.com").is_valid());
        assert!(v.validate("http://foo.bar/path").is_valid());
        assert!(v.validate("ftp://files.example.com").is_valid());
    }

    #[test]
    fn url_invalid() {
        let v = Url::new();
        assert!(!v.validate("not-a-url").is_valid());
        assert!(!v.validate("://missing-scheme").is_valid());
        assert!(!v.validate("http://").is_valid());
        assert!(!v.validate("").is_valid());
    }

    #[test]
    fn regex_valid() {
        let v = Regex::compile(r"\d{3}-\d{4}");
        assert!(v.validate("123-4567").is_valid());
    }

    #[test]
    fn regex_invalid() {
        let v = Regex::compile(r"\d{3}-\d{4}");
        assert!(!v.validate("12-4567").is_valid());
        assert!(!v.validate("abc").is_valid());
    }

    #[test]
    fn regex_fullmatch_semantics() {
        // Pattern should match the entire string, not just a substring
        let v = Regex::compile(r"\d+");
        assert!(v.validate("123").is_valid());
        assert!(!v.validate("123abc").is_valid());
        assert!(!v.validate("abc123").is_valid());
    }

    #[test]
    fn regex_try_compile_invalid() {
        assert!(Regex::try_compile("[invalid").is_none());
    }

    #[test]
    fn number_rejects_nan_inf() {
        let v = Number::new();
        assert!(!v.validate("NaN").is_valid());
        assert!(!v.validate("inf").is_valid());
        assert!(!v.validate("-inf").is_valid());
        assert!(!v.validate("infinity").is_valid());
    }

    // ── Ported from Python tests/test_validation.py ─────────────────────────

    fn generic_failure(value: &str) -> Failure {
        Failure::new().with_value(value)
    }

    /// Python: test_ValidationResult_merge_successes
    #[test]
    fn validation_result_merge_successes() {
        let results = vec![ValidationResult::success(), ValidationResult::success()];
        assert_eq!(
            ValidationResult::merge(results),
            ValidationResult::success()
        );
    }

    /// Python: test_ValidationResult_merge_failures
    #[test]
    fn validation_result_merge_failures() {
        let failure_one = generic_failure("1");
        let failure_two = generic_failure("2");
        let results = vec![
            ValidationResult::failure(vec![failure_one.clone()]),
            ValidationResult::failure(vec![failure_two.clone()]),
            ValidationResult::success(),
        ];
        let expected = ValidationResult::failure(vec![failure_one, failure_two]);
        let merged = ValidationResult::merge(results);
        assert_eq!(merged, expected);
        assert!(!merged.is_valid());
    }

    /// Python: test_ValidationResult_failure_descriptions
    #[test]
    fn validation_result_failure_descriptions() {
        let result = ValidationResult::failure(vec![
            Failure::new().with_description("One"),
            Failure::new().with_description("Two"),
            Failure::new().with_description("Three"),
        ]);
        assert_eq!(result.failure_descriptions(), vec!["One", "Two", "Three"]);
    }

    /// Failures without a resolved description are skipped (Python filters
    /// `description is not None`).
    #[test]
    fn failure_descriptions_skip_missing() {
        let result =
            ValidationResult::failure(vec![Failure::new(), Failure::new().with_description("A")]);
        assert_eq!(result.failure_descriptions(), vec!["A"]);
    }

    /// Python: ValidatorWithDescribeFailure
    struct ValidatorWithDescribeFailure {
        failure_description: Option<String>,
    }

    impl Validator for ValidatorWithDescribeFailure {
        fn validate(&self, _value: &str) -> ValidationResult {
            self.failure(Failure::new())
        }

        fn failure_description(&self) -> Option<String> {
            self.failure_description.clone()
        }

        fn describe_failure(&self, _failure: &Failure) -> Option<String> {
            Some("describe_failure".to_string())
        }
    }

    /// Python: test_Failure_description_priorities_parameter_only
    #[test]
    fn failure_description_priorities_parameter_only() {
        let number_validator = Number::new().with_failure_description("ABC");
        let result = number_validator.validate("x");
        // The constructor-supplied value takes priority over describe_failure.
        assert_eq!(result.failures[0].description.as_deref(), Some("ABC"));
    }

    /// Python: test_Failure_description_priorities_parameter_and_describe_failure
    #[test]
    fn failure_description_priorities_parameter_and_describe_failure() {
        let validator = ValidatorWithDescribeFailure {
            failure_description: Some("ABC".to_string()),
        };
        let result = validator.validate("x");
        // Even though the validator has a describe_failure, the explicit
        // failure_description takes priority.
        assert_eq!(result.failures[0].description.as_deref(), Some("ABC"));
    }

    /// Python: test_Failure_description_priorities_describe_failure_only
    #[test]
    fn failure_description_priorities_describe_failure_only() {
        let validator = ValidatorWithDescribeFailure {
            failure_description: None,
        };
        let result = validator.validate("x");
        assert_eq!(
            result.failures[0].description.as_deref(),
            Some("describe_failure")
        );
    }

    /// Python: ValidatorWithFailureMessageAndNoDescribe
    struct ValidatorWithFailureMessageAndNoDescribe;

    impl Validator for ValidatorWithFailureMessageAndNoDescribe {
        fn validate(&self, _value: &str) -> ValidationResult {
            self.failure(Failure::new().with_description("ABC"))
        }
    }

    /// Python: test_Failure_description_parameter_and_description_inside_validate
    #[test]
    fn failure_description_parameter_and_description_inside_validate() {
        let validator = ValidatorWithFailureMessageAndNoDescribe;
        let result = validator.validate("x");
        assert_eq!(result.failures[0].description.as_deref(), Some("ABC"));
    }

    /// Python: ValidatorWithFailureMessageAndDescribe
    struct ValidatorWithFailureMessageAndDescribe;

    impl Validator for ValidatorWithFailureMessageAndDescribe {
        fn validate(&self, value: &str) -> ValidationResult {
            self.failure(Failure::new().with_value(value).with_description("ABC"))
        }

        fn describe_failure(&self, _failure: &Failure) -> Option<String> {
            Some("describe_failure".to_string())
        }
    }

    /// Python: test_Failure_description_describe_and_description_inside_validate
    #[test]
    fn failure_description_describe_and_description_inside_validate() {
        let validator = ValidatorWithFailureMessageAndDescribe;
        let result = validator.validate("x");
        assert_eq!(
            result.failures,
            vec![Failure::new().with_value("x").with_description("ABC")]
        );
    }

    /// Python: test_Integer_failure_description_when_NotANumber
    /// (regression test for Textualize/textual#4413)
    #[test]
    fn integer_failure_description_when_not_a_number() {
        let validator = Integer::new();
        let result = validator.validate("x");
        assert!(!result.is_valid());
        assert_eq!(result.failure_descriptions()[0], "Must be a valid integer.");
    }

    // ── Python-exact default descriptions per built-in validator ────────────

    #[test]
    fn number_default_descriptions() {
        assert_eq!(
            Number::new().validate("x").failure_descriptions(),
            vec!["Must be a valid number."]
        );
        // Python float("") is a ValueError -> NotANumber, not a bespoke message.
        assert_eq!(
            Number::new().validate("").failure_descriptions(),
            vec!["Must be a valid number."]
        );
        assert_eq!(
            Number::new()
                .minimum(1.0)
                .validate("0")
                .failure_descriptions(),
            vec!["Must be greater than or equal to 1."]
        );
        assert_eq!(
            Number::new()
                .maximum(100.0)
                .validate("101")
                .failure_descriptions(),
            vec!["Must be less than or equal to 100."]
        );
        assert_eq!(
            Number::new()
                .minimum(1.0)
                .maximum(100.0)
                .validate("101")
                .failure_descriptions(),
            vec!["Must be between 1 and 100."]
        );
    }

    #[test]
    fn integer_default_descriptions() {
        assert_eq!(
            Integer::new().validate("abc").failure_descriptions(),
            vec!["Must be a valid integer."]
        );
        // A number which is not an integer is NotAnInteger -> same message.
        assert_eq!(
            Integer::new().validate("3.5").failure_descriptions(),
            vec!["Must be a valid integer."]
        );
        assert_eq!(
            Integer::new()
                .minimum(100)
                .validate("99")
                .failure_descriptions(),
            vec!["Must be greater than or equal to 100."]
        );
        assert_eq!(
            Integer::new()
                .maximum(200)
                .validate("201")
                .failure_descriptions(),
            vec!["Must be less than or equal to 200."]
        );
        assert_eq!(
            Integer::new()
                .minimum(100)
                .maximum(200)
                .validate("201")
                .failure_descriptions(),
            vec!["Must be between 100 and 200."]
        );
    }

    /// Python parity: a non-integer number outside the range fails the range
    /// check first (Integer inherits Number's validate), so the description is
    /// the range message, not "Must be a valid integer.".
    #[test]
    fn integer_range_checked_on_float_first() {
        assert_eq!(
            Integer::new()
                .minimum(100)
                .validate("99.5")
                .failure_descriptions(),
            vec!["Must be greater than or equal to 100."]
        );
    }

    #[test]
    fn length_default_descriptions() {
        assert_eq!(
            Length::new()
                .minimum(5)
                .validate("abc")
                .failure_descriptions(),
            vec!["Must be longer than 5 characters."]
        );
        assert_eq!(
            Length::new()
                .maximum(3)
                .validate("abcd")
                .failure_descriptions(),
            vec!["Must be shorter than 3 characters."]
        );
        assert_eq!(
            Length::new()
                .minimum(2)
                .maximum(3)
                .validate("abcd")
                .failure_descriptions(),
            vec!["Must be between 2 and 3 characters."]
        );
    }

    #[test]
    fn url_default_description() {
        assert_eq!(
            Url::new().validate("not-a-url").failure_descriptions(),
            vec!["Must be a valid URL."]
        );
    }

    #[test]
    fn regex_default_description() {
        // Python: f"Must match regular expression {self.regex!r} (flags={self.flags})."
        // Python's repr of the pattern string escapes the backslash: '\\d+'.
        assert_eq!(
            Regex::compile(r"\d+")
                .validate("abc")
                .failure_descriptions(),
            vec![r"Must match regular expression '\\d+' (flags=0)."]
        );
    }

    #[test]
    fn function_uses_failure_description() {
        let v = Function::new(|_| false, "failure!");
        let result = v.validate("x");
        assert!(!result.is_valid());
        assert_eq!(result.failure_descriptions(), vec!["failure!"]);
    }

    #[test]
    fn builtin_explicit_failure_description_overrides_default() {
        let v = Integer::new().with_failure_description("Please enter a whole number.");
        assert_eq!(
            v.validate("x").failure_descriptions(),
            vec!["Please enter a whole number."]
        );
    }
}
