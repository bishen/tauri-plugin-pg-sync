//! # Tauri Plugin: pg-sync
//!
//! An offline-first PostgreSQL sync plugin for Tauri apps.
//! Supports all platforms: Windows, macOS, Linux, Android, and iOS.
//!
//! ## Features
//!
//! - **Offline-First**: Local SQLite database, fully functional offline
//! - **PostgreSQL Sync**: Automatic synchronization with remote PostgreSQL
//! - **Conflict Resolution**: HLC (Hybrid Logical Clock) based conflict resolution
//! - **GIS Support**: Optional spatial data processing (enable with `geo` feature)
//! - **Cross-Platform**: Supports all Tauri-supported platforms
//!
//! ## Supported Platforms
//!
//! | Platform | Architecture | TLS Backend |
//! |----------|--------------|-------------|
//! | Windows  | x86_64, aarch64 | native-tls |
//! | macOS    | x86_64, aarch64 | native-tls |
//! | Linux    | x86_64, aarch64 | native-tls |
//! | Android  | arm64-v8a, armeabi-v7a, x86_64 | rustls |
//! | iOS      | arm64, x86_64 (Simulator) | rustls |
//!
//! ## Usage
//!
//! ### Rust Side (src-tauri)
//!
//! ```rust,ignore
//! // main.rs or lib.rs
//! fn main() {
//!     tauri::Builder::default()
//!         .plugin(tauri_plugin_pg_sync::init())
//!         .run(tauri::generate_context!())
//!         .expect("error while running tauri application");
//! }
//! ```
//!
//! ### JavaScript/TypeScript Side
//!
//! ```typescript
//! import { initDatabase, insert, findAll, syncNow } from '@bishen/tauri-plugin-pg-sync';
//!
//! // Initialize database
//! await initDatabase();
//!
//! // CRUD operations
//! const id = await insert('users', { name: 'Alice', email: 'alice@example.com' });
//! const users = await findAll('users');
//!
//! // Sync to remote
//! await syncNow();
//! ```
//!
//! ## Modules
//!
//! - [`db`]: Database operations (local SQLite and remote PostgreSQL)
//! - [`sync`]: Synchronization engine and conflict resolution
//! - [`error`]: Error types and result aliases
//! - [`geo`]: GIS/spatial data support (requires `geo` feature)

use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};

mod commands;
pub mod db;
pub mod error;
#[cfg(feature = "geo")]
pub mod geo;
pub mod sync;

pub use commands::*;
pub use error::*;

use std::sync::Arc;
use tokio::sync::RwLock;

use sync::SyncEngine;

/// Plugin state managed by Tauri.
///
/// This struct holds the synchronization engine and is automatically
/// managed by Tauri's state management system.
pub struct PgSyncState {
    /// The synchronization engine instance.
    pub engine: Arc<RwLock<Option<SyncEngine>>>,
}

impl Default for PgSyncState {
    fn default() -> Self {
        Self {
            engine: Arc::new(RwLock::new(None)),
        }
    }
}

/// Initialize the pg-sync plugin.
///
/// This function creates and configures the Tauri plugin with all
/// necessary command handlers for database operations and synchronization.
///
/// # Example
///
/// ```rust,ignore
/// tauri::Builder::default()
///     .plugin(tauri_plugin_pg_sync::init())
///     .run(tauri::generate_context!())
///     .unwrap();
/// ```
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("pg-sync")
        .setup(|app, _api| {
            app.manage(PgSyncState::default());
            log::info!("[PgSync] Plugin initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::init_database,
            commands::init_database_mobile,
            commands::get_db_path,
            commands::connect_remote,
            commands::connect_remote_with_retry,
            commands::start_auto_reconnect,
            commands::disconnect_remote,
            commands::sync_now,
            commands::is_online,
            commands::ensure_table,
            commands::insert,
            commands::update,
            commands::delete,
            commands::find_by_id,
            commands::find_all,
            commands::find_where,
            commands::query,
            commands::count,
            commands::insert_many,
            commands::update_many,
            commands::delete_many,
            commands::clear_table,
            commands::get_local_schema,
            commands::list_local_tables,
            commands::list_remote_tables,
            commands::push_table_schema,
            commands::pull_table_schema,
            commands::purge_deleted,
            commands::purge_all_deleted,
            commands::get_deleted_stats,
        ])
        .build()
}

/// Extension trait for accessing the plugin state from Tauri's AppHandle.
///
/// This trait provides a convenient method to access the pg-sync plugin state
/// from any type that implements `Manager<R>` (like `AppHandle` or `Window`).
///
/// # Example
///
/// ```rust,ignore
/// use tauri_plugin_pg_sync::PgSyncExt;
///
/// fn some_command(app: tauri::AppHandle) {
///     let state = app.pg_sync_state();
///     // Use state...
/// }
/// ```
pub trait PgSyncExt<R: Runtime> {
    /// Get the pg-sync plugin state.
    fn pg_sync_state(&self) -> tauri::State<'_, PgSyncState>;
}

impl<R: Runtime, T: Manager<R>> PgSyncExt<R> for T {
    fn pg_sync_state(&self) -> tauri::State<'_, PgSyncState> {
        self.state::<PgSyncState>()
    }
}
