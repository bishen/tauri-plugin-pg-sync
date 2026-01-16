//! Error types for the pg-sync plugin.
//!
//! This module defines the error types used throughout the plugin,
//! providing a unified error handling mechanism for database operations,
//! synchronization, and serialization.
//!
//! # Example
//!
//! ```rust,ignore
//! use tauri_plugin_pg_sync::error::{Error, Result};
//!
//! fn do_something() -> Result<()> {
//!     // If database not initialized
//!     return Err(Error::NotInitialized);
//! }
//! ```

use serde::{Serialize, Serializer};

/// Plugin error type.
///
/// This enum represents all possible errors that can occur in the pg-sync plugin.
/// It supports automatic conversion from common error types like `rusqlite::Error`,
/// `sqlx::Error`, `serde_json::Error`, and `std::io::Error`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The local database has not been initialized.
    /// Call `init_database()` or `init_database_mobile()` first.
    #[error("Database not initialized")]
    NotInitialized,

    /// The remote PostgreSQL database is not connected.
    /// Call `connect_remote()` first.
    #[error("Remote database not connected")]
    RemoteNotConnected,

    /// An error occurred in the local SQLite database.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// An error occurred in the remote PostgreSQL database.
    #[error("SQLx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    /// A JSON serialization or deserialization error occurred.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// An I/O error occurred.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// The specified table was not found in the database.
    #[error("Table not found: {0}")]
    TableNotFound(String),

    /// The specified record was not found in the table.
    #[error("Record not found: {0}")]
    RecordNotFound(String),

    /// A synchronization error occurred.
    #[error("Sync error: {0}")]
    Sync(String),

    /// A custom error with a user-defined message.
    #[error("{0}")]
    Custom(String),

    /// An internal error (wrapped from anyhow::Error).
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

/// A specialized Result type for pg-sync operations.
///
/// This is defined as a convenience alias for `std::result::Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;
