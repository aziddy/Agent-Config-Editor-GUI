use serde::Serialize;
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    Io,
    Parse,
    ExternalChange,
    ReadOnly,
    NotFound,
    InvalidInput,
    Conflict,
}

/// Error type returned to the webview. Serialized as `{ code, message, path? }`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            path: None,
        }
    }

    pub fn with_path(mut self, path: impl AsRef<Path>) -> Self {
        self.path = Some(path.as_ref().to_string_lossy().into_owned());
        self
    }

    pub fn io(err: std::io::Error, path: impl AsRef<Path>) -> Self {
        let code = if err.kind() == std::io::ErrorKind::NotFound {
            ErrorCode::NotFound
        } else {
            ErrorCode::Io
        };
        Self::new(code, err.to_string()).with_path(path)
    }

    pub fn parse(message: impl Into<String>, path: impl AsRef<Path>) -> Self {
        Self::new(ErrorCode::Parse, message).with_path(path)
    }

    pub fn read_only(message: impl Into<String>, path: impl AsRef<Path>) -> Self {
        Self::new(ErrorCode::ReadOnly, message).with_path(path)
    }

    pub fn external_change(path: impl AsRef<Path>) -> Self {
        Self::new(
            ErrorCode::ExternalChange,
            "the file changed on disk since it was loaded; reload and try again",
        )
        .with_path(path)
    }

    pub fn not_found(message: impl Into<String>, path: impl AsRef<Path>) -> Self {
        Self::new(ErrorCode::NotFound, message).with_path(path)
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidInput, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, message)
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.path {
            Some(p) => write!(f, "{}: {} ({p})", serde_code(self.code), self.message),
            None => write!(f, "{}: {}", serde_code(self.code), self.message),
        }
    }
}

fn serde_code(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::Io => "io",
        ErrorCode::Parse => "parse",
        ErrorCode::ExternalChange => "externalChange",
        ErrorCode::ReadOnly => "readOnly",
        ErrorCode::NotFound => "notFound",
        ErrorCode::InvalidInput => "invalidInput",
        ErrorCode::Conflict => "conflict",
    }
}

impl std::error::Error for AppError {}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        Self::new(ErrorCode::Parse, err.to_string())
    }
}

impl From<toml_edit::TomlError> for AppError {
    fn from(err: toml_edit::TomlError) -> Self {
        Self::new(ErrorCode::Parse, err.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
