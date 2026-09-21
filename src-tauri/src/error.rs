use rusqlite::Error as SqliteError;
use serde::Serialize;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

#[derive(Debug)]
pub enum AppError {
    InvalidInput(&'static str),
    NotFound,
    UnsupportedDatabase(i64),
    Database(SqliteError),
    Io(std::io::Error),
}

impl Display for AppError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(message) => formatter.write_str(message),
            Self::NotFound => formatter.write_str("Character not found"),
            Self::UnsupportedDatabase(version) => {
                write!(formatter, "Database version {version} is not supported")
            }
            Self::Database(error) => write!(formatter, "Database operation failed: {error}"),
            Self::Io(error) => write!(formatter, "File operation failed: {error}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<SqliteError> for AppError {
    fn from(error: SqliteError) -> Self {
        Self::Database(error)
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<AppError> for CommandError {
    fn from(error: AppError) -> Self {
        match error {
            AppError::InvalidInput(message) => Self {
                code: "invalidInput".to_owned(),
                message: message.to_owned(),
            },
            AppError::NotFound => Self {
                code: "notFound".to_owned(),
                message: "Character not found".to_owned(),
            },
            AppError::UnsupportedDatabase(_) => Self {
                code: "unsupportedDatabase".to_owned(),
                message: "This library was created by a newer Furfolio version".to_owned(),
            },
            AppError::Database(_) | AppError::Io(_) => Self {
                code: "storageError".to_owned(),
                message: "Furfolio could not access the local library".to_owned(),
            },
        }
    }
}
