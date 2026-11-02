use rusqlite::Error as SqliteError;
use serde::Serialize;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory_name: Option<String>,
}

/// Export failures deliberately carry no source path, SQL or OS diagnostic.
#[derive(Debug)]
pub(crate) enum BackupError {
    Busy,
    Pending,
    Integrity,
    Limit,
    Timeout,
    NoSpace,
    Destination,
    Unsupported,
    Failed,
    DurabilityUncertain(String),
    CleanupPending(String),
}

#[derive(Debug)]
pub enum AppError {
    InvalidInput(&'static str),
    NotFound,
    InvalidMedia,
    MediaTooLarge,
    MediaBusy,
    CleanupPending { applied: bool },
    UnsupportedDatabase(i64),
    Database(SqliteError),
    Io(std::io::Error),
    Backup(BackupError),
}

impl Display for AppError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        // Startup errors must be just as path-free as IPC errors.
        formatter.write_str(&CommandError::from_ref(self).message)
    }
}
impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}
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
impl CommandError {
    fn from_ref(error: &AppError) -> Self {
        let (code, message) = match error {
            AppError::InvalidInput(message) => ("invalidInput", *message),
            AppError::NotFound => ("notFound", "The requested item was not found"),
            AppError::InvalidMedia => (
                "invalidMedia",
                "Select a valid static PNG, JPEG or WebP image",
            ),
            AppError::MediaTooLarge => (
                "mediaTooLarge",
                "The image exceeds the supported media limits",
            ),
            AppError::MediaBusy => ("mediaBusy", "Another media operation is in progress"),
            AppError::CleanupPending { .. } => (
                "cleanupPending",
                "Some managed media files still need safe cleanup",
            ),
            AppError::UnsupportedDatabase(version) => {
                let _ = version;
                (
                    "unsupportedDatabase",
                    "This library was created by a newer Furfolio version",
                )
            }
            AppError::Backup(error) => match error {
                BackupError::Busy => ("backupBusy", "Another library or media export operation is in progress"),
                BackupError::Pending => ("backupPending", "Managed media cleanup must finish before exporting"),
                BackupError::Integrity => ("backupIntegrity", "The library could not pass backup integrity checks"),
                BackupError::Limit => ("backupLimit", "The library exceeds the supported backup limits"),
                BackupError::Timeout => ("backupTimeout", "The backup exceeded its supported time limit"),
                BackupError::NoSpace => ("backupNoSpace", "The destination has insufficient space for this backup"),
                BackupError::Destination => ("backupDestination", "Select a safe existing backup destination outside this library"),
                BackupError::Unsupported => ("backupUnsupported", "Safe backup export is supported only on macOS"),
                BackupError::Failed => ("backupFailed", "The backup could not be exported"),
                BackupError::DurabilityUncertain(_) => ("backupDurabilityUncertain", "The backup was published but its durability could not be confirmed. Keep and verify the published backup; do not blindly retry"),
                BackupError::CleanupPending(_) => ("backupCleanupPending", "An incomplete backup remains and needs safe manual inspection"),
            },
            AppError::Database(_) | AppError::Io(_) => (
                "storageError",
                "Furfolio could not access the local library",
            ),
        };
        Self {
            code: code.into(),
            message: message.into(),
            applied: matches!(error, AppError::CleanupPending { applied: true }).then_some(true),
            published: matches!(error, AppError::Backup(BackupError::DurabilityUncertain(_)))
                .then_some(true),
            directory_name: match error {
                AppError::Backup(
                    BackupError::DurabilityUncertain(name) | BackupError::CleanupPending(name),
                ) => Some(name.clone()),
                _ => None,
            },
        }
    }
}
impl From<AppError> for CommandError {
    fn from(error: AppError) -> Self {
        Self::from_ref(&error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn errors_never_leak_paths_and_only_committed_cleanup_has_applied() {
        let error = CommandError::from(AppError::Io(std::io::Error::other("/private/source.png")));
        let json = serde_json::to_string(&error).unwrap();
        assert!(!json.contains("private"));
        assert!(!json.contains("applied"));
        let error = CommandError::from(AppError::CleanupPending { applied: false });
        assert!(!serde_json::to_string(&error).unwrap().contains("applied"));
        assert_eq!(
            CommandError::from(AppError::CleanupPending { applied: true }).applied,
            Some(true)
        );
    }
}
