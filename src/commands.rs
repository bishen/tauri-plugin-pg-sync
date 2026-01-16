//! Tauri 命令实现

use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Runtime, State};

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

/// 连接远程数据库并自动拉取远程表结构
#[tauri::command]
pub async fn connect_remote(state: State<'_, PgSyncState>, database_url: String) -> Result<()> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    engine
        .connect_remote(&database_url)
        .await
        .map_err(|e| Error::Sync(e.to_string()))?;
    
    // 连接成功后，自动拉取远程表结构到本地
    if let Err(e) = auto_pull_remote_schemas(engine).await {
        log::warn!("[PgSync] Failed to auto-pull remote schemas: {}", e);
    }
    
    Ok(())
}

/// 自动拉取远程表结构到本地
async fn auto_pull_remote_schemas(engine: &SyncEngine) -> anyhow::Result<()> {
    let remote_guard = engine.remote_db().read().await;
    let remote = remote_guard.as_ref().ok_or_else(|| anyhow::anyhow!("Remote not connected"))?;
    
    // 获取远程所有表（排除以 _ 开头的系统表）
    let tables = remote.list_tables().await?;
    
    for table in tables {
        // 检查本地是否已有此表
        let local_schema = engine.local_db().get_table_schema(&table)?;
        if !local_schema.is_empty() {
            log::debug!("[PgSync] Local table {} already exists, skipping", table);
            continue;
        }
        
        // 拉取远程表结构
        let remote_schema = remote.get_table_schema(&table).await?;
        if remote_schema.is_empty() {
            continue;
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
        log::info!("[PgSync] Auto-pulled table schema: {}", table);
    }
    
    Ok(())
}

/// 带重试的连接远程数据库，成功后自动拉取远程表结构
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
        .map_err(|e| Error::Sync(e.to_string()))?;
    
    // 连接成功后，自动拉取远程表结构到本地
    if let Err(e) = auto_pull_remote_schemas(engine).await {
        log::warn!("[PgSync] Failed to auto-pull remote schemas: {}", e);
    }
    
    Ok(())
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
    println!("[PgSync] sync_now command called");
    
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or_else(|| {
        println!("[PgSync] sync_now: engine not initialized");
        Error::NotInitialized
    })?;
    
    println!("[PgSync] Starting sync...");
    let result = engine
        .sync()
        .await
        .map_err(|e| {
            println!("[PgSync] sync error: {}", e);
            Error::Sync(e.to_string())
        })?;

    println!("[PgSync] Sync complete: pushed={}, pulled={}, conflicts={}", 
        result.pushed, result.pulled, result.conflicts);
    
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

/// 启动实时监听器，收到 PostgreSQL 通知时发射 Tauri 事件
#[tauri::command]
pub async fn start_realtime_listener<R: Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, PgSyncState>,
) -> Result<()> {
    log::info!("[PgSync] Starting realtime listener...");
    
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    
    let mut rx = engine.start_realtime_listener().await
        .map_err(|e| {
            log::error!("[PgSync] Failed to start realtime listener: {}", e);
            Error::Sync(e.to_string())
        })?;
    
    log::info!("[PgSync] Realtime listener started successfully, waiting for notifications on 'data_changes' channel");
    
    let app_clone = app.clone();
    let puller = engine.clone_for_pull();
    
    tokio::spawn(async move {
        log::info!("[PgSync] Listener task started, waiting for events...");
        while let Some(event) = rx.recv().await {
            log::info!("[PgSync] Received realtime event: {:?}", event);
            
            // 发射 Tauri 事件通知前端
            let _ = app_clone.emit("sync:data_changed", &event);
            
            // 自动拉取远程变更
            match puller.pull_remote().await {
                Ok(count) => {
                    if count > 0 {
                        log::info!("[PgSync] Pulled {} changes after realtime notification", count);
                        // 通知前端有数据被拉取，触发 UI 刷新
                        let _ = app_clone.emit("sync:pulled", serde_json::json!({
                            "pulled": count,
                            "source": "realtime"
                        }));
                    }
                }
                Err(e) => {
                    log::warn!("[PgSync] Failed to pull after notification: {}", e);
                }
            }
        }
        log::info!("[PgSync] Realtime listener stopped (channel closed)");
    });
    
    Ok(())
}

// ============================================================================
// 同步过滤命令
// ============================================================================

/// 设置表的同步过滤条件
/// 
/// filter: SQL WHERE 条件（不含 WHERE 关键字）
/// 例如: "company_id = 'abc'" 或 "uid = '123'"
/// 
/// 设置后，只同步满足条件的数据，避免拉取无关数据
#[tauri::command]
pub async fn set_sync_filter(
    state: State<'_, PgSyncState>,
    table: String,
    filter: String,
) -> Result<()> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    engine.set_sync_filter(&table, &filter).await;
    Ok(())
}

/// 移除表的同步过滤条件
#[tauri::command]
pub async fn remove_sync_filter(state: State<'_, PgSyncState>, table: String) -> Result<()> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    engine.remove_sync_filter(&table).await;
    Ok(())
}

/// 获取表的同步过滤条件
#[tauri::command]
pub async fn get_sync_filter(
    state: State<'_, PgSyncState>,
    table: String,
) -> Result<Option<String>> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    Ok(engine.get_sync_filter(&table).await)
}

/// 获取所有同步过滤条件
#[tauri::command]
pub async fn get_all_sync_filters(
    state: State<'_, PgSyncState>,
) -> Result<std::collections::HashMap<String, String>> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    Ok(engine.get_all_sync_filters().await)
}

// ============================================================================
// 表操作命令
// ============================================================================

#[derive(serde::Deserialize)]
pub struct TableSchema {
    pub columns: Vec<(String, String)>,
}

/// 确保表存在（本地 + 远程自动同步）
/// 
/// 1. 本地不存在 → 创建本地表
/// 2. 如果在线且远程不存在 → 自动推送表结构到远程
#[tauri::command]
pub async fn ensure_table(
    state: State<'_, PgSyncState>,
    table: String,
    schema: TableSchema,
) -> Result<()> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    
    // 1. 确保本地表存在
    engine.local_db().ensure_table(&table, &schema.columns)?;
    
    // 2. 如果在线，自动推送表结构到远程（如果远程不存在）
    if engine.is_online() {
        let remote_guard = engine.remote_db().read().await;
        if let Some(remote) = remote_guard.as_ref() {
            // 检查远程表是否存在
            match remote.table_exists(&table).await {
                Ok(false) => {
                    // 远程表不存在，推送本地表结构
                    let local_schema = engine.local_db().get_table_schema(&table)?;
                    let remote_columns: Vec<crate::db::remote::ColumnDef> = local_schema
                        .iter()
                        .map(|c| crate::db::remote::ColumnDef {
                            name: c.name.clone(),
                            data_type: c.data_type.clone(),
                            nullable: c.nullable,
                            default: c.default.clone(),
                        })
                        .collect();
                    
                    if let Err(e) = remote.create_table(&table, &remote_columns).await {
                        log::warn!("[PgSync] Failed to auto-push table {}: {}", table, e);
                    } else {
                        log::info!("[PgSync] Auto-pushed table schema: {}", table);
                    }
                }
                Ok(true) => {
                    log::debug!("[PgSync] Remote table {} already exists", table);
                }
                Err(e) => {
                    log::warn!("[PgSync] Failed to check remote table {}: {}", table, e);
                }
            }
        }
    }
    
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
    let node_id = engine.node_id().to_string();

    let id = engine.local_db().insert(&table, &data, &hlc, &node_id)?;

    // 构建完整的 payload（包含同步元数据）
    let mut full_data = data.clone();
    if let Some(obj) = full_data.as_object_mut() {
        obj.insert("id".to_string(), serde_json::Value::String(id.clone()));
        obj.insert("_hlc".to_string(), serde_json::Value::String(hlc.clone()));
        obj.insert("_node_id".to_string(), serde_json::Value::String(node_id.clone()));
        obj.insert("_version".to_string(), serde_json::Value::Number(1.into()));
        obj.insert("_deleted".to_string(), serde_json::Value::Number(0.into()));
    }
    let payload = serde_json::to_string(&full_data).ok();
    engine
        .local_db()
        .record_change(&table, &id, "INSERT", &hlc, payload.as_deref())?;

    // 在线时立即推送
    if engine.is_online() {
        let engine_clone = engine.clone_for_push();
        tokio::spawn(async move {
            if let Err(e) = engine_clone.push_pending().await {
                log::warn!("[PgSync] Background push failed: {}", e);
            }
        });
    }

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
        // 获取更新后的完整数据
        let full_row = engine.local_db().find_by_id(&table, &id)?;
        let payload = full_row.and_then(|r| serde_json::to_string(&r).ok());
        engine
            .local_db()
            .record_change(&table, &id, "UPDATE", &hlc, payload.as_deref())?;

        // 在线时立即推送
        if engine.is_online() {
            let engine_clone = engine.clone_for_push();
            tokio::spawn(async move {
                if let Err(e) = engine_clone.push_pending().await {
                    log::warn!("[PgSync] Background push failed: {}", e);
                }
            });
        }
    }

    Ok(updated)
}

#[tauri::command]
pub async fn delete(state: State<'_, PgSyncState>, table: String, id: String) -> Result<bool> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;

    let hlc = engine.generate_hlc().await;
    let node_id = engine.node_id().to_string();
    
    // 先获取完整数据再删除
    let full_row = engine.local_db().find_by_id(&table, &id)?;
    let deleted = engine.local_db().delete(&table, &id, &hlc)?;

    if deleted {
        // 构建删除的 payload
        let payload = if let Some(mut row) = full_row {
            if let Some(obj) = row.as_object_mut() {
                obj.insert("_hlc".to_string(), serde_json::Value::String(hlc.clone()));
                obj.insert("_node_id".to_string(), serde_json::Value::String(node_id.clone()));
                obj.insert("_deleted".to_string(), serde_json::Value::Number(1.into()));
            }
            serde_json::to_string(&row).ok()
        } else {
            // 如果没有完整数据，构建最小 payload
            let min_payload = serde_json::json!({
                "id": id,
                "_hlc": hlc,
                "_node_id": node_id,
                "_deleted": 1
            });
            serde_json::to_string(&min_payload).ok()
        };
        engine
            .local_db()
            .record_change(&table, &id, "DELETE", &hlc, payload.as_deref())?;

        // 在线时立即推送
        if engine.is_online() {
            let engine_clone = engine.clone_for_push();
            tokio::spawn(async move {
                if let Err(e) = engine_clone.push_pending().await {
                    log::warn!("[PgSync] Background push failed: {}", e);
                }
            });
        }
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
    let node_id = engine.node_id().to_string();

    let ids = engine
        .local_db()
        .insert_many(&table, &items, &hlc, &node_id)?;

    for (idx, id) in ids.iter().enumerate() {
        let item_hlc = format!("{}_{:06}", hlc, idx);
        // 构建完整 payload
        let payload = if let Some(item) = items.get(idx) {
            let mut full_data = item.clone();
            if let Some(obj) = full_data.as_object_mut() {
                obj.insert("id".to_string(), serde_json::Value::String(id.clone()));
                obj.insert("_hlc".to_string(), serde_json::Value::String(item_hlc.clone()));
                obj.insert("_node_id".to_string(), serde_json::Value::String(node_id.clone()));
                obj.insert("_version".to_string(), serde_json::Value::Number(1.into()));
                obj.insert("_deleted".to_string(), serde_json::Value::Number(0.into()));
            }
            serde_json::to_string(&full_data).ok()
        } else {
            None
        };
        engine
            .local_db()
            .record_change(&table, id, "INSERT", &item_hlc, payload.as_deref())?;
    }

    // 在线时立即推送
    if engine.is_online() {
        let engine_clone = engine.clone_for_push();
        tokio::spawn(async move {
            if let Err(e) = engine_clone.push_pending().await {
                log::warn!("[PgSync] Background push failed: {}", e);
            }
        });
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

    for (idx, (id, _)) in updates.iter().enumerate() {
        let item_hlc = format!("{}_{:06}", hlc, idx);
        // 获取更新后的完整数据
        let full_row = engine.local_db().find_by_id(&table, id)?;
        let payload = full_row.and_then(|r| serde_json::to_string(&r).ok());
        engine
            .local_db()
            .record_change(&table, id, "UPDATE", &item_hlc, payload.as_deref())?;
    }

    // 在线时立即推送
    if engine.is_online() {
        let engine_clone = engine.clone_for_push();
        tokio::spawn(async move {
            if let Err(e) = engine_clone.push_pending().await {
                log::warn!("[PgSync] Background push failed: {}", e);
            }
        });
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
    let node_id = engine.node_id().to_string();
    
    // 先获取所有要删除的数据
    let mut full_rows: Vec<Option<serde_json::Value>> = Vec::new();
    for id in &ids {
        full_rows.push(engine.local_db().find_by_id(&table, id)?);
    }
    
    let deleted = engine.local_db().delete_many(&table, &ids, &hlc)?;

    for (idx, id) in ids.iter().enumerate() {
        let item_hlc = format!("{}_{:06}", hlc, idx);
        // 构建删除的 payload
        let payload = if let Some(Some(mut row)) = full_rows.get(idx).cloned() {
            if let Some(obj) = row.as_object_mut() {
                obj.insert("_hlc".to_string(), serde_json::Value::String(item_hlc.clone()));
                obj.insert("_node_id".to_string(), serde_json::Value::String(node_id.clone()));
                obj.insert("_deleted".to_string(), serde_json::Value::Number(1.into()));
            }
            serde_json::to_string(&row).ok()
        } else {
            let min_payload = serde_json::json!({
                "id": id,
                "_hlc": item_hlc,
                "_node_id": node_id,
                "_deleted": 1
            });
            serde_json::to_string(&min_payload).ok()
        };
        engine
            .local_db()
            .record_change(&table, id, "DELETE", &item_hlc, payload.as_deref())?;
    }

    // 在线时立即推送
    if engine.is_online() {
        let engine_clone = engine.clone_for_push();
        tokio::spawn(async move {
            if let Err(e) = engine_clone.push_pending().await {
                log::warn!("[PgSync] Background push failed: {}", e);
            }
        });
    }

    Ok(deleted)
}

#[tauri::command]
pub async fn clear_table(state: State<'_, PgSyncState>, table: String) -> Result<usize> {
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
pub async fn push_table_schema(state: State<'_, PgSyncState>, table: String) -> Result<()> {
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
pub async fn pull_table_schema(state: State<'_, PgSyncState>, table: String) -> Result<()> {
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

    engine
        .local_db()
        .create_table_from_remote(&table, &local_columns)?;

    log::info!("[PgSync] Pulled table schema: {}", table);
    Ok(())
}

// ============================================================================
// Schema Registry 命令
// ============================================================================

/// 获取已注册的表结构
#[tauri::command]
pub async fn get_registered_schema(
    state: State<'_, PgSyncState>,
    table: String,
) -> Result<Option<Vec<(String, String)>>> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    Ok(engine.local_db().get_registered_schema(&table)?)
}

/// 列出所有注册的表
#[tauri::command]
pub async fn list_registered_tables(state: State<'_, PgSyncState>) -> Result<Vec<String>> {
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or(Error::NotInitialized)?;
    Ok(engine.local_db().list_registered_tables()?)
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
