//! Tauri 命令实现

use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Runtime, State};

use crate::db::{ColumnDef, LocalDb, QueryOptions};
use crate::error::{Error, Result};
use crate::sync::{NetworkConfig, SyncEngine};
use crate::PgSyncState;

/// 获取数据库路径
#[allow(unused_variables)]
fn get_database_path<R: Runtime>(app: &tauri::AppHandle<R>) -> PathBuf {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let app_dir = app
            .path()
            .app_data_dir()
            .expect("failed to get app data dir");
        std::fs::create_dir_all(&app_dir).expect("failed to create app data dir");
        app_dir.join("pg_sync.db")
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let exe_dir = std::env::current_exe()
            .expect("failed to get exe path")
            .parent()
            .expect("failed to get exe dir")
            .to_path_buf();
        let data_dir = exe_dir.join("data");
        std::fs::create_dir_all(&data_dir).expect("failed to create data dir");
        data_dir.join("pg_sync.db")
    }
}

// ============================================================================
// 初始化命令
// ============================================================================

#[tauri::command]
pub async fn init_database<R: Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, PgSyncState>,
) -> Result<String> {
    init_database_with_config(app, state, false).await
}

#[tauri::command]
pub async fn init_database_mobile<R: Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, PgSyncState>,
) -> Result<String> {
    init_database_with_config(app, state, true).await
}

async fn init_database_with_config<R: Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, PgSyncState>,
    is_mobile: bool,
) -> Result<String> {
    let db_path = get_database_path(&app);
    log::info!("[PgSync] Database path: {:?}", db_path);

    let local_db = LocalDb::new(&db_path)?;

    let node_id = local_db.get_or_create_node_id()?;
    log::info!("[PgSync] Node ID: {}", node_id);

    let config = if is_mobile || cfg!(any(target_os = "android", target_os = "ios")) {
        log::info!("[PgSync] Using mobile network configuration");
        NetworkConfig::mobile()
    } else {
        NetworkConfig::default()
    };

    let engine = SyncEngine::with_config(Arc::new(local_db), node_id.clone(), config);
    *state.engine.write().await = Some(engine);

    Ok(node_id)
}

#[tauri::command]
pub async fn get_db_path<R: Runtime>(app: tauri::AppHandle<R>) -> Result<String> {
    let db_path = get_database_path(&app);
    Ok(db_path.to_string_lossy().to_string())
}

// ============================================================================
// 远程连接命令
// ============================================================================

#[tauri::command]
pub async fn connect_remote(
    state: State<'_, PgSyncState>,
    database_url: String,
) -> Result<()> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    engine
        .connect_remote(&database_url)
        .await
        .map_err(|e| Error::Sync(e.to_string()))
}

#[tauri::command]
pub async fn connect_remote_with_retry(
    state: State<'_, PgSyncState>,
    database_url: String,
) -> Result<()> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    engine
        .connect_remote_with_retry(&database_url)
        .await
        .map_err(|e| Error::Sync(e.to_string()))
}

#[tauri::command]
pub async fn start_auto_reconnect(state: State<'_, PgSyncState>) -> Result<()> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    engine.start_auto_reconnect();
    log::info!("[PgSync] Auto-reconnect started");
    Ok(())
}

#[tauri::command]
pub async fn disconnect_remote(state: State<'_, PgSyncState>) -> Result<()> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    engine.disconnect_remote().await;
    Ok(())
}

#[tauri::command]
pub async fn sync_now(state: State<'_, PgSyncState>) -> Result<String> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    let result = engine.sync().await.map_err(|e| Error::Sync(e.to_string()))?;

    Ok(serde_json::json!({
        "pushed": result.pushed,
        "pulled": result.pulled,
        "conflicts": result.conflicts,
        "errors": result.errors
    })
    .to_string())
}

#[tauri::command]
pub async fn is_online(state: State<'_, PgSyncState>) -> Result<bool> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    Ok(engine.is_online())
}

// ============================================================================
// 表操作命令
// ============================================================================

#[derive(serde::Deserialize)]
pub struct TableSchema {
    pub columns: Vec<(String, String)>,
}

#[tauri::command]
pub async fn ensure_table(
    state: State<'_, PgSyncState>,
    table: String,
    schema: TableSchema,
) -> Result<()> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    engine.local_db().ensure_table(&table, &schema.columns)?;
    Ok(())
}

// ============================================================================
// CRUD 命令
// ============================================================================

#[tauri::command]
pub async fn insert(
    state: State<'_, PgSyncState>,
    table: String,
    data: serde_json::Value,
) -> Result<String> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;

    let hlc = engine.generate_hlc().await;
    let node_id = engine.node_id();

    let id = engine.local_db().insert(&table, &data, &hlc, node_id)?;

    let payload = serde_json::to_string(&data).ok();
    engine
        .local_db()
        .record_change(&table, &id, "INSERT", &hlc, payload.as_deref())?;

    Ok(id)
}

#[tauri::command]
pub async fn update(
    state: State<'_, PgSyncState>,
    table: String,
    id: String,
    data: serde_json::Value,
) -> Result<bool> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;

    let hlc = engine.generate_hlc().await;
    let updated = engine.local_db().update(&table, &id, &data, &hlc)?;

    if updated {
        let payload = serde_json::to_string(&data).ok();
        engine
            .local_db()
            .record_change(&table, &id, "UPDATE", &hlc, payload.as_deref())?;
    }

    Ok(updated)
}

#[tauri::command]
pub async fn delete(
    state: State<'_, PgSyncState>,
    table: String,
    id: String,
) -> Result<bool> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;

    let hlc = engine.generate_hlc().await;
    let deleted = engine.local_db().delete(&table, &id, &hlc)?;

    if deleted {
        engine
            .local_db()
            .record_change(&table, &id, "DELETE", &hlc, None)?;
    }

    Ok(deleted)
}

#[tauri::command]
pub async fn find_by_id(
    state: State<'_, PgSyncState>,
    table: String,
    id: String,
) -> Result<Option<serde_json::Value>> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    Ok(engine.local_db().find_by_id(&table, &id)?)
}

#[tauri::command]
pub async fn find_all(
    state: State<'_, PgSyncState>,
    table: String,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<serde_json::Value>> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    Ok(engine.local_db().find_all(&table, limit, offset)?)
}

#[tauri::command]
pub async fn find_where(
    state: State<'_, PgSyncState>,
    table: String,
    conditions: serde_json::Value,
) -> Result<Vec<serde_json::Value>> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    Ok(engine.local_db().find_where(&table, &conditions)?)
}

#[tauri::command]
pub async fn query(
    state: State<'_, PgSyncState>,
    table: String,
    options: QueryOptions,
) -> Result<Vec<serde_json::Value>> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    Ok(engine.local_db().query(&table, &options)?)
}

#[tauri::command]
pub async fn count(
    state: State<'_, PgSyncState>,
    table: String,
    conditions: Option<serde_json::Value>,
) -> Result<i64> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    Ok(engine.local_db().count(&table, conditions.as_ref())?)
}

// ============================================================================
// 批量操作命令
// ============================================================================

#[tauri::command]
pub async fn insert_many(
    state: State<'_, PgSyncState>,
    table: String,
    items: Vec<serde_json::Value>,
) -> Result<Vec<String>> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;

    let hlc = engine.generate_hlc().await;
    let node_id = engine.node_id();

    let ids = engine.local_db().insert_many(&table, &items, &hlc, node_id)?;

    for (idx, id) in ids.iter().enumerate() {
        let item_hlc = format!("{}_{:06}", hlc, idx);
        let payload = items.get(idx).and_then(|v| serde_json::to_string(v).ok());
        engine
            .local_db()
            .record_change(&table, id, "INSERT", &item_hlc, payload.as_deref())?;
    }

    Ok(ids)
}

#[tauri::command]
pub async fn update_many(
    state: State<'_, PgSyncState>,
    table: String,
    updates: Vec<(String, serde_json::Value)>,
) -> Result<usize> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;

    let hlc = engine.generate_hlc().await;
    let updated = engine.local_db().update_many(&table, &updates, &hlc)?;

    for (idx, (id, data)) in updates.iter().enumerate() {
        let item_hlc = format!("{}_{:06}", hlc, idx);
        let payload = serde_json::to_string(data).ok();
        engine
            .local_db()
            .record_change(&table, id, "UPDATE", &item_hlc, payload.as_deref())?;
    }

    Ok(updated)
}

#[tauri::command]
pub async fn delete_many(
    state: State<'_, PgSyncState>,
    table: String,
    ids: Vec<String>,
) -> Result<usize> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;

    let hlc = engine.generate_hlc().await;
    let deleted = engine.local_db().delete_many(&table, &ids, &hlc)?;

    for (idx, id) in ids.iter().enumerate() {
        let item_hlc = format!("{}_{:06}", hlc, idx);
        engine
            .local_db()
            .record_change(&table, id, "DELETE", &item_hlc, None)?;
    }

    Ok(deleted)
}

#[tauri::command]
pub async fn clear_table(
    state: State<'_, PgSyncState>,
    table: String,
) -> Result<usize> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;

    let hlc = engine.generate_hlc().await;
    let all = engine.local_db().find_all(&table, None, None)?;
    let cleared = engine.local_db().clear_table(&table, &hlc)?;

    for (idx, item) in all.iter().enumerate() {
        if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
            let item_hlc = format!("{}_{:06}", hlc, idx);
            engine
                .local_db()
                .record_change(&table, id, "DELETE", &item_hlc, None)?;
        }
    }

    Ok(cleared)
}

// ============================================================================
// 表结构命令
// ============================================================================

#[tauri::command]
pub async fn get_local_schema(
    state: State<'_, PgSyncState>,
    table: String,
) -> Result<Vec<ColumnDef>> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    Ok(engine.local_db().get_table_schema(&table)?)
}

#[tauri::command]
pub async fn list_local_tables(state: State<'_, PgSyncState>) -> Result<Vec<String>> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    Ok(engine.local_db().list_tables()?)
}

#[tauri::command]
pub async fn list_remote_tables(
    state: State<'_, PgSyncState>,
    pg_schema: Option<String>,
) -> Result<Vec<String>> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;

    let remote_guard = engine.remote_db().read().await;
    let remote = remote_guard.as_ref().ok_or(Error::RemoteNotConnected)?;

    let schema = pg_schema.as_deref().unwrap_or("public");
    remote
        .list_tables_in_schema(schema)
        .await
        .map_err(|e| Error::Sync(e.to_string()))
}

#[tauri::command]
pub async fn push_table_schema(
    state: State<'_, PgSyncState>,
    table: String,
) -> Result<()> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;

    let local_schema = engine.local_db().get_table_schema(&table)?;

    if local_schema.is_empty() {
        return Err(Error::TableNotFound(table));
    }

    let remote_columns: Vec<crate::db::remote::ColumnDef> = local_schema
        .iter()
        .map(|c| crate::db::remote::ColumnDef {
            name: c.name.clone(),
            data_type: c.data_type.clone(),
            nullable: c.nullable,
            default: c.default.clone(),
        })
        .collect();

    let remote_guard = engine.remote_db().read().await;
    let remote = remote_guard.as_ref().ok_or(Error::RemoteNotConnected)?;

    remote
        .create_table(&table, &remote_columns)
        .await
        .map_err(|e| Error::Sync(e.to_string()))?;

    log::info!("[PgSync] Pushed table schema: {}", table);
    Ok(())
}

#[tauri::command]
pub async fn pull_table_schema(
    state: State<'_, PgSyncState>,
    table: String,
) -> Result<()> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;

    let remote_guard = engine.remote_db().read().await;
    let remote = remote_guard.as_ref().ok_or(Error::RemoteNotConnected)?;

    let remote_schema = remote
        .get_table_schema(&table)
        .await
        .map_err(|e| Error::Sync(e.to_string()))?;

    if remote_schema.is_empty() {
        return Err(Error::TableNotFound(table));
    }

    let local_columns: Vec<ColumnDef> = remote_schema
        .iter()
        .map(|c| ColumnDef {
            name: c.name.clone(),
            data_type: c.data_type.clone(),
            nullable: c.nullable,
            default: c.default.clone(),
        })
        .collect();

    engine.local_db().create_table_from_remote(&table, &local_columns)?;

    log::info!("[PgSync] Pulled table schema: {}", table);
    Ok(())
}

// ============================================================================
// 清理命令
// ============================================================================

#[tauri::command]
pub async fn purge_deleted(
    state: State<'_, PgSyncState>,
    table: String,
    days_old: Option<i64>,
) -> Result<usize> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    Ok(engine.local_db().purge_deleted(&table, days_old)?)
}

#[tauri::command]
pub async fn purge_all_deleted(
    state: State<'_, PgSyncState>,
    days_old: Option<i64>,
) -> Result<usize> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    Ok(engine.local_db().purge_all_deleted(days_old)?)
}

#[tauri::command]
pub async fn get_deleted_stats(
    state: State<'_, PgSyncState>,
    table: String,
) -> Result<serde_json::Value> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    Ok(engine.local_db().get_deleted_stats(&table)?)
}
