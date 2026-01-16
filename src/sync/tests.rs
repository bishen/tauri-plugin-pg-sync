//! 同步引擎单元测试
//! 
//! 测试 HLC、冲突解决、同步逻辑

#[cfg(test)]
mod hlc_tests {
    use crate::sync::hlc::HybridLogicalClock;

    #[test]
    fn test_hlc_creation() {
        let hlc = HybridLogicalClock::new("node1".to_string());
        assert_eq!(hlc.node_id, "node1");
        assert!(hlc.physical > 0);
        assert_eq!(hlc.logical, 0);
    }

    #[test]
    fn test_hlc_tick_monotonic() {
        let mut hlc = HybridLogicalClock::new("node1".to_string());
        
        let t1 = hlc.tick();
        let t2 = hlc.tick();
        let t3 = hlc.tick();
        
        // 每次 tick 应该产生更大的值
        assert!(HybridLogicalClock::compare(&t2, &t1) == std::cmp::Ordering::Greater);
        assert!(HybridLogicalClock::compare(&t3, &t2) == std::cmp::Ordering::Greater);
    }

    #[test]
    fn test_hlc_parse_roundtrip() {
        let hlc = HybridLogicalClock::new("test-node".to_string());
        let s = hlc.to_string();
        let parsed = HybridLogicalClock::parse(&s).unwrap();
        
        assert_eq!(hlc.physical, parsed.physical);
        assert_eq!(hlc.logical, parsed.logical);
        assert_eq!(hlc.node_id, parsed.node_id);
    }

    #[test]
    fn test_hlc_parse_invalid() {
        assert!(HybridLogicalClock::parse("invalid").is_none());
        assert!(HybridLogicalClock::parse("1:2").is_none());
        assert!(HybridLogicalClock::parse("abc:0:node").is_none());
    }

    #[test]
    fn test_hlc_compare() {
        // 物理时间不同
        assert_eq!(
            HybridLogicalClock::compare("1000:0:a", "2000:0:b"),
            std::cmp::Ordering::Less
        );
        
        // 物理时间相同，逻辑时间不同
        assert_eq!(
            HybridLogicalClock::compare("1000:1:a", "1000:2:b"),
            std::cmp::Ordering::Less
        );
        
        // 完全相同（按 node_id 比较）
        assert_eq!(
            HybridLogicalClock::compare("1000:0:a", "1000:0:b"),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn test_hlc_update_from_remote() {
        let mut hlc = HybridLogicalClock::new("local".to_string());
        
        // 模拟收到一个更大的远程 HLC
        let remote_hlc = format!("{}:5:remote", hlc.physical + 1000);
        let new_ts = hlc.update(&remote_hlc);
        
        // 应该同步到远程时间
        let parsed = HybridLogicalClock::parse(&new_ts).unwrap();
        assert!(parsed.physical >= hlc.physical);
    }

    #[test]
    fn test_hlc_concurrent_ticks() {
        let mut hlc = HybridLogicalClock::new("node".to_string());
        
        // 快速连续 tick（同一毫秒内）
        let mut timestamps = Vec::new();
        for _ in 0..100 {
            timestamps.push(hlc.tick());
        }
        
        // 所有时间戳应该唯一且递增
        for i in 1..timestamps.len() {
            assert!(
                HybridLogicalClock::compare(&timestamps[i], &timestamps[i-1]) 
                == std::cmp::Ordering::Greater,
                "Timestamp {} should be greater than {}",
                timestamps[i], timestamps[i-1]
            );
        }
    }
}

#[cfg(test)]
mod conflict_tests {
    use crate::sync::conflict::{ConflictResolver, ConflictStrategy, ResolveResult};
    use serde_json::json;

    #[test]
    fn test_lww_local_wins() {
        let resolver = ConflictResolver::new(ConflictStrategy::LastWriteWins);
        
        let local = json!({"id": "1", "value": "local"});
        let remote = json!({"id": "1", "value": "remote"});
        
        // 本地 HLC 更大
        let result = resolver.resolve(&local, &remote, "2000:0:a", "1000:0:b");
        
        match result {
            ResolveResult::UseLocal(data) => {
                assert_eq!(data["value"], "local");
            }
            _ => panic!("Expected UseLocal"),
        }
    }

    #[test]
    fn test_lww_remote_wins() {
        let resolver = ConflictResolver::new(ConflictStrategy::LastWriteWins);
        
        let local = json!({"id": "1", "value": "local"});
        let remote = json!({"id": "1", "value": "remote"});
        
        // 远程 HLC 更大
        let result = resolver.resolve(&local, &remote, "1000:0:a", "2000:0:b");
        
        match result {
            ResolveResult::UseRemote(data) => {
                assert_eq!(data["value"], "remote");
            }
            _ => panic!("Expected UseRemote"),
        }
    }

    #[test]
    fn test_fww_first_wins() {
        let resolver = ConflictResolver::new(ConflictStrategy::FirstWriteWins);
        
        let local = json!({"id": "1", "value": "local"});
        let remote = json!({"id": "1", "value": "remote"});
        
        // 本地 HLC 更小（更早）
        let result = resolver.resolve(&local, &remote, "1000:0:a", "2000:0:b");
        
        match result {
            ResolveResult::UseLocal(data) => {
                assert_eq!(data["value"], "local");
            }
            _ => panic!("Expected UseLocal for FirstWriteWins"),
        }
    }

    #[test]
    fn test_manual_creates_conflict() {
        let resolver = ConflictResolver::new(ConflictStrategy::Manual);
        
        let local = json!({"id": "1", "value": "local"});
        let remote = json!({"id": "1", "value": "remote"});
        
        let result = resolver.resolve(&local, &remote, "1000:0:a", "2000:0:b");
        
        match result {
            ResolveResult::Conflict(record) => {
                assert_eq!(record.row_id, "1");
                assert!(!record.resolved);
            }
            _ => panic!("Expected Conflict"),
        }
    }

    #[test]
    fn test_merge_fields() {
        let resolver = ConflictResolver::new(ConflictStrategy::LastWriteWins);
        
        let local = json!({
            "id": "1",
            "name": "Local Name",
            "email": "local@test.com",
            "age": 25
        });
        let remote = json!({
            "id": "1",
            "name": "Remote Name",
            "email": "remote@test.com",
            "age": 30
        });
        
        // 合并，保留本地的 name 和 email
        let merged = resolver.merge_fields(&local, &remote, &["name", "email"]);
        
        assert_eq!(merged["name"], "Local Name");
        assert_eq!(merged["email"], "local@test.com");
        assert_eq!(merged["age"], 30); // 使用远程值
    }
}

#[cfg(test)]
mod queue_tests {
    use crate::sync::queue::{SyncQueue, SyncTask};
    use serde_json::json;

    #[test]
    fn test_queue_fifo() {
        let queue = SyncQueue::new();
        
        queue.enqueue(SyncTask::new("t1", "r1", "INSERT", json!({}), "1:0:n"));
        queue.enqueue(SyncTask::new("t2", "r2", "INSERT", json!({}), "2:0:n"));
        
        let first = queue.dequeue().unwrap();
        assert_eq!(first.table_name, "t1");
        
        let second = queue.dequeue().unwrap();
        assert_eq!(second.table_name, "t2");
        
        assert!(queue.dequeue().is_none());
    }

    #[test]
    fn test_retry_mechanism() {
        let queue = SyncQueue::new();
        
        let mut task = SyncTask::new("test", "row1", "INSERT", json!({}), "1:0:n");
        task.max_retries = 3;
        
        // 初始: retries=0, can_retry = 0 < 3 = true
        // mark_failed: retries=1, can_retry = 1 < 3 = true -> re-queued
        queue.mark_failed(task);
        assert_eq!(queue.len(), 1);
        
        let task = queue.dequeue().unwrap();
        assert_eq!(task.retries, 1);
        
        // mark_failed: retries=2, can_retry = 2 < 3 = true -> re-queued
        queue.mark_failed(task);
        let task = queue.dequeue().unwrap();
        assert_eq!(task.retries, 2);
        
        // mark_failed: retries=3, can_retry = 3 < 3 = false -> failed list
        queue.mark_failed(task);
        assert!(queue.is_empty());
        assert_eq!(queue.get_failed().len(), 1);
        
        // 验证 failed 任务的重试次数
        let failed = queue.get_failed();
        assert_eq!(failed[0].retries, 3);
    }

    #[test]
    fn test_retry_failed() {
        let queue = SyncQueue::new();
        
        let mut task = SyncTask::new("test", "row1", "INSERT", json!({}), "1:0:n");
        task.max_retries = 0; // 立即失败
        
        queue.mark_failed(task);
        assert_eq!(queue.get_failed().len(), 1);
        
        // 重试所有失败的任务
        queue.retry_failed();
        
        assert!(queue.get_failed().is_empty());
        assert_eq!(queue.len(), 1);
        
        let retried = queue.dequeue().unwrap();
        assert_eq!(retried.retries, 0); // 重置重试次数
    }
}

#[cfg(test)]
mod network_tests {
    use crate::sync::network::{NetworkMonitor, NetworkState, NetworkConfig};
    use std::time::Duration;

    #[test]
    fn test_network_state_transitions() {
        let monitor = NetworkMonitor::new();
        
        assert_eq!(monitor.state(), NetworkState::Offline);
        assert!(!monitor.is_online());
        
        monitor.set_state(NetworkState::Online);
        assert_eq!(monitor.state(), NetworkState::Online);
        assert!(monitor.is_online());
        
        monitor.set_state(NetworkState::Syncing);
        assert_eq!(monitor.state(), NetworkState::Syncing);
        assert!(monitor.is_online()); // Syncing 也算 online
        
        monitor.set_state(NetworkState::Error);
        assert_eq!(monitor.state(), NetworkState::Error);
        assert!(!monitor.is_online());
        
        monitor.set_state(NetworkState::Reconnecting);
        assert_eq!(monitor.state(), NetworkState::Reconnecting);
        assert!(!monitor.is_online());
    }

    #[test]
    fn test_network_subscription() {
        let monitor = NetworkMonitor::new();
        let mut rx = monitor.subscribe();
        
        // 初始状态
        assert_eq!(*rx.borrow(), NetworkState::Offline);
        
        monitor.set_state(NetworkState::Online);
        
        // 订阅者应该收到更新
        assert!(rx.has_changed().unwrap_or(false));
        assert_eq!(*rx.borrow_and_update(), NetworkState::Online);
    }

    #[test]
    fn test_mobile_config() {
        let config = NetworkConfig::mobile();
        
        // 移动端配置应该有更短的超时
        assert!(config.connect_timeout_ms < NetworkConfig::default().connect_timeout_ms);
        assert!(config.request_timeout_ms < NetworkConfig::default().request_timeout_ms);
        
        // 但更多的重试次数
        assert!(config.max_retries > NetworkConfig::default().max_retries);
    }

    #[test]
    fn test_weak_network_config() {
        let config = NetworkConfig::weak_network();
        
        // 弱网配置应该有更长的超时
        assert!(config.connect_timeout_ms > NetworkConfig::default().connect_timeout_ms);
        assert!(config.request_timeout_ms > NetworkConfig::default().request_timeout_ms);
        
        // 和更多的重试次数
        assert!(config.max_retries > NetworkConfig::default().max_retries);
    }

    #[test]
    fn test_exponential_backoff() {
        let monitor = NetworkMonitor::new();
        
        let backoff1 = monitor.next_backoff();
        let backoff2 = monitor.next_backoff();
        let backoff3 = monitor.next_backoff();
        
        // 退避时间应该指数增长
        assert!(backoff2 > backoff1);
        assert!(backoff3 > backoff2);
    }

    #[test]
    fn test_backoff_capped() {
        let config = NetworkConfig {
            max_backoff_ms: 1000,
            initial_backoff_ms: 100,
            ..Default::default()
        };
        let monitor = NetworkMonitor::with_config(config);
        
        // 多次调用，确保不会超过最大值
        for _ in 0..20 {
            let backoff = monitor.next_backoff();
            // 考虑 10% 抖动
            assert!(backoff.as_millis() <= 1100);
        }
    }

    #[test]
    fn test_should_retry() {
        let config = NetworkConfig {
            max_retries: 3,
            ..Default::default()
        };
        let monitor = NetworkMonitor::with_config(config);
        
        assert!(monitor.should_retry());
        monitor.next_backoff(); // retry 1
        assert!(monitor.should_retry());
        monitor.next_backoff(); // retry 2
        assert!(monitor.should_retry());
        monitor.next_backoff(); // retry 3
        assert!(!monitor.should_retry()); // 达到最大次数
    }

    #[test]
    fn test_retry_reset_on_success() {
        let monitor = NetworkMonitor::new();
        
        // 模拟几次失败
        monitor.next_backoff();
        monitor.next_backoff();
        
        // 成功后重置
        monitor.set_state(NetworkState::Online);
        
        // 应该可以再次重试
        assert!(monitor.should_retry());
    }

    #[test]
    fn test_timeout_durations() {
        let config = NetworkConfig {
            connect_timeout_ms: 5000,
            request_timeout_ms: 15000,
            health_check_interval_ms: 30000,
            ..Default::default()
        };
        let monitor = NetworkMonitor::with_config(config);
        
        assert_eq!(monitor.connect_timeout(), Duration::from_millis(5000));
        assert_eq!(monitor.request_timeout(), Duration::from_millis(15000));
        assert_eq!(monitor.health_check_interval(), Duration::from_millis(30000));
    }
}

#[cfg(test)]
mod sync_engine_tests {
    use std::sync::Arc;
    use crate::db::LocalDb;
    use crate::sync::SyncEngine;
    use serde_json::json;

    fn setup_engine() -> SyncEngine {
        let db = LocalDb::in_memory().expect("Failed to create db");
        let node_id = db.get_or_create_node_id().unwrap();
        SyncEngine::new(Arc::new(db), node_id)
    }

    #[tokio::test]
    async fn test_engine_initialization() {
        let engine = setup_engine();
        
        assert!(!engine.is_online());
        assert!(!engine.node_id().is_empty());
    }

    #[tokio::test]
    async fn test_hlc_generation() {
        let engine = setup_engine();
        
        let hlc1 = engine.generate_hlc().await;
        let hlc2 = engine.generate_hlc().await;
        
        // HLC 应该单调递增
        assert!(crate::sync::hlc::HybridLogicalClock::compare(&hlc2, &hlc1) 
            == std::cmp::Ordering::Greater);
    }

    #[tokio::test]
    async fn test_record_local_change() {
        let engine = setup_engine();
        
        let hlc = engine.record_local_change(
            "users",
            "user-1",
            "INSERT",
            Some(r#"{"name":"Test"}"#),
        ).await.unwrap();
        
        assert!(!hlc.is_empty());
        
        // 变更应该记录在 changelog 中
        let changes = engine.local_db().get_unsynced_changes().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].table_name, "users");
        assert_eq!(changes[0].row_id, "user-1");
    }

    #[tokio::test]
    async fn test_sync_offline() {
        let engine = setup_engine();
        
        // 离线状态下同步应该返回 offline 错误
        let result = engine.sync().await.unwrap();
        
        assert_eq!(result.pushed, 0);
        assert_eq!(result.pulled, 0);
        assert!(!result.errors.is_empty());
        assert!(result.errors[0].contains("Offline"));
    }

    #[tokio::test]
    async fn test_crud_with_sync_metadata() {
        let engine = setup_engine();
        let db = engine.local_db();
        
        // 创建表
        db.ensure_table("items", &[
            ("title".to_string(), "TEXT".to_string()),
        ]).unwrap();
        
        // 插入
        let hlc = engine.generate_hlc().await;
        let id = db.insert("items", &json!({"title": "Test"}), &hlc, engine.node_id()).unwrap();
        
        // 验证同步元数据
        let item = db.find_by_id("items", &id).unwrap().unwrap();
        assert_eq!(item["_node_id"].as_str().unwrap(), engine.node_id());
        assert_eq!(item["_synced"], 0);
        assert_eq!(item["_deleted"], 0);
        assert_eq!(item["_version"], 1);
    }
}
