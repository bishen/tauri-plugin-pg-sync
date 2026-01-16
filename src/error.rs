//! 插件错误类型定义

use serde::{Serialize, Serializer};

/// 插件错误类型
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Database not initialized")]
    NotInitialized,

    #[error("Remote database not connected")]
    RemoteNotConnected,

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("SQLx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Table not found: {0}")]
    TableNotFound(String),

    #[error("Record not found: {0}")]
    RecordNotFound(String),

    #[error("Sync error: {0}")]
    Sync(String),

    #[error("{0}")]
    Custom(String),

    #[error("Internal error: {0}")]
    Anyhow(String),
}

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Error::Anyhow(err.to_string())
    }
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
