use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("terminal driver error: {0}")]
    Terminal(#[from] std::io::Error),
    #[error("app runtime stopped")]
    RuntimeStopped,
}

pub type Result<T> = std::result::Result<T, Error>;
