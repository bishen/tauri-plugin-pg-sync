//! Tauri Plugin: pg-sync
//!
//! 离线优先的 PostgreSQL 同步插件，支持全平台 (Windows, macOS, Linux, Android, iOS)。
//!
//! # 功能特性
//!
//! - **离线优先**: 本地 SQLite 数据库，支持完全离线使用
//! - **PostgreSQL 同步**: 自动与远程 PostgreSQL 数据库同步
//! - **冲突解决**: 基于 HLC (Hybrid Logical Clock) 的冲突解决
//! - **GIS 支持**: 可选的空间数据处理能力
//! - **跨平台**: 支持所有 Tauri 支持的平台
//!
//! # 使用方法
//!
//! ## Rust 端 (src-tauri)
//!
//! ```rust,ignore
//! // main.rs 或 lib.rs
//! fn main() {
//!     tauri::Builder::default()
//!         .plugin(tauri_plugin_pg_sync::init())
//!         .run(tauri::generate_context!())
//!         .expect("error while running tauri application");
//! }
//! ```
//!
//! ## JavaScript/TypeScript 端
//!
//! ```typescript
//! import { initDatabase, insert, findAll, syncNow } from '@bishen/tauri-plugin-pg-sync';
//!
//! // 初始化数据库
//! await initDatabase();
//!
//! // CRUD 操作
//! const id = await insert('users', { name: 'Alice', email: 'alice@example.com' });
//! const users = await findAll('users');
//!
//! // 同步到远程
//! await syncNow();
//! ```

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

/// 插件状态
pub struct PgSyncState {
    pub engine: Arc<RwLock<Option<SyncEngine>>>,
}

impl Default for PgSyncState {
    fn default() -> Self {
        Self {
            engine: Arc::new(RwLock::new(None)),
        }
    }
}

/// 初始化 pg-sync 插件
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

/// 扩展 trait，便于直接从 AppHandle 访问插件状态
pub trait PgSyncExt<R: Runtime> {
    fn pg_sync_state(&self) -> tauri::State<'_, PgSyncState>;
}

impl<R: Runtime, T: Manager<R>> PgSyncExt<R> for T {
    fn pg_sync_state(&self) -> tauri::State<'_, PgSyncState> {
        self.state::<PgSyncState>()
    }
}
