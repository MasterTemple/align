use std::fmt;

/// An error from parsing a pattern string or compiling one of its regexes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub message: String,
    /// 0-based character offset into the pattern string, when the error
    /// can be attributed to a specific position.
    pub col: Option<usize>,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Error {
            message: message.into(),
            col: None,
        }
    }

    pub fn at(col: usize, message: impl Into<String>) -> Self {
        Error {
            message: message.into(),
            col: Some(col),
        }
    }

    pub(crate) fn with_col(mut self, col: usize) -> Self {
        self.col.get_or_insert(col);
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.col {
            Some(c) => write!(f, "{} (at column {})", self.message, c + 1),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
