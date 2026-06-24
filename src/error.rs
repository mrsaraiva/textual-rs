use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("terminal driver error: {0}")]
    Terminal(#[from] std::io::Error),
    #[error("text area language error: {0}")]
    TextAreaLanguage(String),
    #[error("app runtime stopped")]
    RuntimeStopped,
    #[error("{0}")]
    Message(String),
}

pub type Result<T> = std::result::Result<T, Error>;
