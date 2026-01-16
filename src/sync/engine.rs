use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use anyhow::Result;

use crate::db::{LocalDb, RemoteDb};
use crate::sync::hlc::HybridLogicalClock;
use crate::sync::conflict::{ConflictResolver, ConflictStrategy};
use crate::sync::queue::SyncQueue;
use crate::sync::network::{NetworkMonitor, NetworkState, NetworkConfig};

/// 数据变更通知
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DataChangeEvent {
    pub table: String,
    pub action: String,
    pub id: String,
}

pub struct SyncEngine {
    local_db: Arc<LocalDb>,
    remote_db: Arc<RwLock<Option<RemoteDb>>>,
    hlc: Arc<RwLock<HybridLogicalClock>>,
    resolver: ConflictResolver,
    #[allow(dead_code)]
    queue: SyncQueue,
    network: Arc<NetworkMonitor>,
    node_id: String,
    database_url: Arc<RwLock<Option<String>>>,
}

impl SyncEngine {
    pub fn new(local_db: Arc<LocalDb>, node_id: String) -> Self {
        Self::with_config(local_db, node_id, NetworkConfig::default())
    }

    /// 使用自定义网络配置创建
    pub fn with_config(local_db: Arc<LocalDb>, node_id: String, config: NetworkConfig) -> Self {
        Self {
            local_db,
            remote_db: Arc::new(RwLock::new(None)),
            hlc: Arc::new(RwLock::new(HybridLogicalClock::new(node_id.clone()))),
            resolver: ConflictResolver::new(ConflictStrategy::LastWriteWins),
            queue: SyncQueue::new(),
            network: Arc::new(NetworkMonitor::with_config(config)),
            node_id,
            database_url: Arc::new(RwLock::new(None)),
        }
    }

    /// 移动端优化配置
    pub fn for_mobile(local_db: Arc<LocalDb>, node_id: String) -> Self {
        Self::with_config(local_db, node_id, NetworkConfig::mobile())
    }

    /// 获取本地数据库引用
    pub fn local_db(&self) -> &LocalDb {
        &self.local_db
    }

    /// 获取节点 ID
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// 获取远程数据库引用
    pub fn remote_db(&self) -> &Arc<RwLock<Option<RemoteDb>>> {
        &self.remote_db
    }

    pub async fn connect_remote(&self, database_url: &str) -> Result<()> {
        let timeout = self.network.connect_timeout();
        
        self.network.set_state(NetworkState::Reconnecting);
        
        match RemoteDb::connect_with_timeout(database_url, timeout).await {
            Ok(remote) => {
                *self.remote_db.write().await = Some(remote);
                *self.database_url.write().await = Some(database_url.to_string());
                self.network.set_state(NetworkState::Online);
                self.network.reset_retries();
                log::info!("[SyncEngine] Connected to remote database");
                Ok(())
            }
            Err(e) => {
                self.network.set_state(NetworkState::Error);
                log::error!("[SyncEngine] Failed to connect: {}", e);
                Err(e)
            }
        }
    }

    /// 带重试的连接（适合弱网环境）
    pub async fn connect_remote_with_retry(&self, database_url: &str) -> Result<()> {
        *self.database_url.write().await = Some(database_url.to_string());
        
        while self.network.should_retry() {
            match self.connect_remote(database_url).await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    let backoff = self.network.next_backoff();
                    log::warn!(
                        "[SyncEngine] Connection failed, retrying in {:?}: {}",
                        backoff, e
                    );
                    tokio::time::sleep(backoff).await;
                }
            }
        }
        
        Err(anyhow::anyhow!("Max retries exceeded"))
    }

    /// 启动自动重连循环
    pub fn start_auto_reconnect(&self) -> tokio::task::JoinHandle<()> {
        let network = Arc::clone(&self.network);
        let remote_db = Arc::clone(&self.remote_db);
        let database_url = Arc::clone(&self.database_url);
        
        tokio::spawn(async move {
            loop {
                let interval = network.health_check_interval();
                tokio::time::sleep(interval).await;
                
                // 检查连接状态
                let should_reconnect = {
                    let remote_guard = remote_db.read().await;
                    if let Some(remote) = remote_guard.as_ref() {
                        match remote.health_check_with_timeout(Duration::from_secs(5)).await {
                            Ok(true) => false,
                            _ => {
                                log::warn!("[SyncEngine] Health check failed, will reconnect");
                                true
                            }
                        }
                    } else {
                        // 没有连接，检查是否有 URL 可以重连
                        database_url.read().await.is_some()
                    }
                };
                
                if should_reconnect {
                    network.set_state(NetworkState::Reconnecting);
                    
                    if let Some(url) = database_url.read().await.clone() {
                        // 清除旧连接
                        *remote_db.write().await = None;
                        
                        // 重连（带退避）
                        while network.should_retry() {
                            let timeout = network.connect_timeout();
                            match RemoteDb::connect_with_timeout(&url, timeout).await {
                                Ok(new_remote) => {
                                    *remote_db.write().await = Some(new_remote);
                                    network.set_state(NetworkState::Online);
                                    network.reset_retries();
                                    log::info!("[SyncEngine] Reconnected successfully");
                                    break;
                                }
                                Err(e) => {
                                    let backoff = network.next_backoff();
                                    log::warn!(
                                        "[SyncEngine] Reconnect failed, retrying in {:?}: {}",
                                        backoff, e
                                    );
                                    tokio::time::sleep(backoff).await;
                                }
                            }
                        }
                        
                        if !network.is_online() {
                            network.set_state(NetworkState::Offline);
                            log::error!("[SyncEngine] Reconnection failed after max retries");
                        }
                    }
                }
            }
        })
    }

    /// 启动实时监听（返回事件接收器）
    pub async fn start_realtime_listener(&self) -> Result<tokio::sync::mpsc::Receiver<DataChangeEvent>> {
        self.start_realtime_listener_with_reconnect(true).await
    }

    /// 启动实时监听（可选自动重连）
    pub async fn start_realtime_listener_with_reconnect(
        &self,
        auto_reconnect: bool,
    ) -> Result<tokio::sync::mpsc::Receiver<DataChangeEvent>> {
        let (tx, rx) = tokio::sync::mpsc::channel::<DataChangeEvent>(100);
        
        let remote_guard = self.remote_db.read().await;
        let remote = remote_guard.as_ref().ok_or_else(|| anyhow::anyhow!("Remote not connected"))?;
        
        let mut listener = remote.create_listener().await?;
        listener.listen("data_changes").await?;
        
        log::info!("[SyncEngine] Started realtime listener");
        
        let network = Arc::clone(&self.network);
        let database_url = Arc::clone(&self.database_url);
        
        tokio::spawn(async move {
            loop {
                match listener.recv().await {
                    Ok(notification) => {
                        let payload = notification.payload();
                        log::debug!("[SyncEngine] Received: {}", payload);
                        
                        // 解析通知
                        if let Ok(event) = serde_json::from_str::<DataChangeEvent>(payload) {
                            if tx.send(event).await.is_err() {
                                log::warn!("[SyncEngine] Receiver dropped, stopping listener");
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("[SyncEngine] Listener error: {}", e);
                        
                        if !auto_reconnect {
                            break;
                        }
                        
                        // 尝试重连监听器
                        log::info!("[SyncEngine] Attempting to reconnect listener...");
                        
                        let mut reconnected = false;
                        while network.should_retry() {
                            let backoff = network.next_backoff();
                            tokio::time::sleep(backoff).await;
                            
                            if let Some(url) = database_url.read().await.clone() {
                                match sqlx::postgres::PgListener::connect(&url).await {
                                    Ok(mut new_listener) => {
                                        if new_listener.listen("data_changes").await.is_ok() {
                                            listener = new_listener;
                                            network.reset_retries();
                                            log::info!("[SyncEngine] Listener reconnected");
                                            reconnected = true;
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        log::warn!("[SyncEngine] Listener reconnect failed: {}", e);
                                    }
                                }
                            }
                        }
                        
                        if !reconnected {
                            log::error!("[SyncEngine] Listener reconnection failed after max retries");
                            break;
                        }
                    }
                }
            }
        });
        
        Ok(rx)
    }

    pub async fn disconnect_remote(&self) {
        *self.remote_db.write().await = None;
        self.network.set_state(NetworkState::Offline);
    }

    pub fn is_online(&self) -> bool {
        self.network.is_online()
    }

    pub async fn generate_hlc(&self) -> String {
        self.hlc.write().await.tick()
    }

    pub async fn record_local_change(
        &self,
        table_name: &str,
        row_id: &str,
        operation: &str,
        payload: Option<&str>,
    ) -> Result<String> {
        let hlc = self.generate_hlc().await;
        self.local_db.record_change(table_name, row_id, operation, &hlc, payload)?;
        Ok(hlc)
    }

    pub async fn sync(&self) -> Result<SyncResult> {
        if !self.is_online() {
            return Ok(SyncResult {
                pushed: 0,
                pulled: 0,
                conflicts: 0,
                errors: vec!["Offline".to_string()],
            });
        }

        self.network.set_state(NetworkState::Syncing);

        let push_result = self.push().await;
        let pull_result = self.pull().await;

        self.network.set_state(NetworkState::Online);

        let mut result = SyncResult::default();
        
        match push_result {
            Ok(count) => result.pushed = count,
            Err(e) => result.errors.push(format!("Push error: {}", e)),
        }

        match pull_result {
            Ok((pulled, conflicts)) => {
                result.pulled = pulled;
                result.conflicts = conflicts;
            }
            Err(e) => result.errors.push(format!("Pull error: {}", e)),
        }

        Ok(result)
    }

    async fn push(&self) -> Result<usize> {
        let changes = self.local_db.get_unsynced_changes()?;
        
        if changes.is_empty() {
            log::debug!("[SyncEngine] No unsynced changes to push");
            return Ok(0);
        }
        
        log::info!("[SyncEngine] Pushing {} changes", changes.len());
        let mut pushed = 0;

        let remote_guard = self.remote_db.read().await;
        let remote = match remote_guard.as_ref() {
            Some(r) => r,
            None => return Ok(0),
        };

        let mut synced_ids = Vec::new();

        for change in &changes {
            // 解析 payload
            let payload: serde_json::Value = match &change.payload {
                Some(p) => match serde_json::from_str(p) {
                    Ok(v) => v,
                    Err(e) => {
                        log::warn!("[SyncEngine] Invalid payload for change {}: {}", change.id, e);
                        continue;
                    }
                },
                None => {
                    log::warn!("[SyncEngine] No payload for change {}", change.id);
                    continue;
                }
            };

            // 验证 payload 是对象
            if !payload.is_object() {
                log::warn!("[SyncEngine] Payload is not an object for change {}", change.id);
                continue;
            }

            log::debug!("[SyncEngine] Pushing change {} to table {}", change.id, change.table_name);

            match remote.push_change(&change.table_name, &payload).await {
                Ok(_) => {
                    synced_ids.push(change.id);
                    pushed += 1;
                    log::debug!("[SyncEngine] Successfully pushed change {}", change.id);
                }
                Err(e) => {
                    log::error!("[SyncEngine] Failed to push change {}: {}", change.id, e);
                }
            }
        }

        self.local_db.mark_synced(&synced_ids)?;
        log::info!("[SyncEngine] Push complete: {}/{} succeeded", pushed, changes.len());
        Ok(pushed)
    }

    async fn pull(&self) -> Result<(usize, usize)> {
        let remote_guard = self.remote_db.read().await;
        let remote = match remote_guard.as_ref() {
            Some(r) => r,
            None => return Ok((0, 0)),
        };

        let tables = self.local_db.list_tables()?;
        let mut total_pulled = 0;
        let mut total_conflicts = 0;

        for table in tables {
            // 获取上次同步的 HLC
            let since_hlc = self.local_db.get_last_sync_hlc(&table)?
                .unwrap_or_else(|| "0:0:".to_string());

            // 从远程获取变更
            let changes = match remote.fetch_changes_since(&table, &since_hlc).await {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("[SyncEngine] Failed to fetch changes for {}: {}", table, e);
                    continue;
                }
            };

            if changes.is_empty() {
                continue;
            }

            let mut last_hlc = since_hlc.clone();

            for remote_row in &changes {
                let row_id = remote_row.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let remote_hlc = remote_row.get("_hlc").and_then(|v| v.as_str()).unwrap_or("");
                let remote_node = remote_row.get("_node_id").and_then(|v| v.as_str()).unwrap_or("");

                // 跳过自己节点的变更（避免回环）
                if remote_node == self.node_id {
                    continue;
                }

                // 检查本地是否有此记录
                let local_row = self.local_db.find_by_id(&table, row_id)?;

                match local_row {
                    Some(local) => {
                        let local_hlc = local.get("_hlc").and_then(|v| v.as_str()).unwrap_or("");
                        
                        // 如果本地记录没有 HLC（预存数据），保留本地优先
                        // 这保护了本地已有数据不被远程覆盖
                        if local_hlc.is_empty() {
                            log::info!(
                                "[SyncEngine] Local record {} has no HLC, keeping local (protecting pre-existing data)",
                                row_id
                            );
                            // 为本地记录补充 HLC，标记为已同步
                            let new_hlc = self.generate_hlc().await;
                            self.local_db.execute(
                                &format!("UPDATE \"{}\" SET _hlc = ?1, _synced = 1 WHERE id = ?2", table),
                                &[&new_hlc as &dyn rusqlite::ToSql, &row_id as &dyn rusqlite::ToSql],
                            )?;
                            continue;
                        }
                        
                        // 使用冲突解决器
                        let result = self.resolver.resolve(&local, remote_row, local_hlc, remote_hlc);
                        
                        match result {
                            crate::sync::conflict::ResolveResult::UseRemote(data) => {
                                // 应用远程数据
                                self.apply_remote_change(&table, row_id, &data, remote_hlc).await?;
                                total_pulled += 1;
                            }
                            crate::sync::conflict::ResolveResult::UseLocal(_) => {
                                // 保留本地，不做变更
                            }
                            crate::sync::conflict::ResolveResult::Merged(data) => {
                                self.apply_remote_change(&table, row_id, &data, remote_hlc).await?;
                                total_pulled += 1;
                            }
                            crate::sync::conflict::ResolveResult::Conflict(record) => {
                                self.save_conflict(&record)?;
                                total_conflicts += 1;
                            }
                        }
                    }
                    None => {
                        // 本地没有，直接插入
                        self.apply_remote_change(&table, row_id, remote_row, remote_hlc).await?;
                        total_pulled += 1;
                    }
                }

                // 更新 last_hlc
                if crate::sync::hlc::HybridLogicalClock::compare(remote_hlc, &last_hlc) 
                    == std::cmp::Ordering::Greater 
                {
                    last_hlc = remote_hlc.to_string();
                }
            }

            // 保存同步进度
            if last_hlc != since_hlc {
                self.local_db.set_last_sync_hlc(&table, &last_hlc)?;
            }
        }

        Ok((total_pulled, total_conflicts))
    }

    /// 应用远程变更到本地
    async fn apply_remote_change(
        &self,
        table: &str,
        row_id: &str,
        data: &serde_json::Value,
        hlc: &str,
    ) -> Result<()> {
        let deleted = data.get("_deleted")
            .and_then(|v| v.as_bool())
            .or_else(|| data.get("_deleted").and_then(|v| v.as_i64()).map(|i| i != 0))
            .unwrap_or(false);

        if deleted {
            self.local_db.execute(
                &format!("UPDATE {} SET _deleted = 1, _hlc = ?1, _synced = 1 WHERE id = ?2", table),
                &[&hlc as &dyn rusqlite::ToSql, &row_id as &dyn rusqlite::ToSql],
            )?;
        } else {
            // 检查记录是否存在
            let exists = self.local_db.find_by_id(table, row_id)?.is_some();
            
            if exists {
                self.local_db.update(table, row_id, data, hlc)?;
            } else {
                self.local_db.insert(table, data, hlc, &self.node_id)?;
            }
            
            // 标记为已同步
            self.local_db.execute(
                &format!("UPDATE {} SET _synced = 1 WHERE id = ?1", table),
                &[&row_id as &dyn rusqlite::ToSql],
            )?;
        }

        Ok(())
    }

    /// 保存冲突记录
    fn save_conflict(&self, record: &crate::sync::conflict::ConflictRecord) -> Result<()> {
        self.local_db.execute(
            "INSERT OR REPLACE INTO _sync_conflicts (id, table_name, row_id, local_value, remote_value, local_hlc, remote_hlc, resolved) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
            &[
                &record.id as &dyn rusqlite::ToSql,
                &record.table_name as &dyn rusqlite::ToSql,
                &record.row_id as &dyn rusqlite::ToSql,
                &serde_json::to_string(&record.local_value).unwrap_or_default() as &dyn rusqlite::ToSql,
                &serde_json::to_string(&record.remote_value).unwrap_or_default() as &dyn rusqlite::ToSql,
                &record.local_hlc as &dyn rusqlite::ToSql,
                &record.remote_hlc as &dyn rusqlite::ToSql,
            ],
        )?;
        log::warn!("[SyncEngine] Conflict saved: {} in {}", record.row_id, record.table_name);
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct SyncResult {
    pub pushed: usize,
    pub pulled: usize,
    pub conflicts: usize,
    pub errors: Vec<String>,
}
