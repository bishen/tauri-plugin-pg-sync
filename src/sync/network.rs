use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkState {
    Online,
    Offline,
    Syncing,
    Error,
    Reconnecting,
}

/// 网络配置
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// 连接超时（毫秒）
    pub connect_timeout_ms: u64,
    /// 请求超时（毫秒）
    pub request_timeout_ms: u64,
    /// 最大重试次数
    pub max_retries: u32,
    /// 初始退避时间（毫秒）
    pub initial_backoff_ms: u64,
    /// 最大退避时间（毫秒）
    pub max_backoff_ms: u64,
    /// 健康检查间隔（毫秒）
    pub health_check_interval_ms: u64,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            connect_timeout_ms: 10_000,       // 10秒连接超时
            request_timeout_ms: 30_000,       // 30秒请求超时
            max_retries: 5,                   // 最多重试5次
            initial_backoff_ms: 1_000,        // 初始1秒退避
            max_backoff_ms: 60_000,           // 最大60秒退避
            health_check_interval_ms: 30_000, // 30秒健康检查
        }
    }
}

impl NetworkConfig {
    /// 移动端优化配置（更短超时，更激进重试）
    pub fn mobile() -> Self {
        Self {
            connect_timeout_ms: 5_000,        // 5秒连接超时
            request_timeout_ms: 15_000,       // 15秒请求超时
            max_retries: 8,                   // 更多重试次数
            initial_backoff_ms: 500,          // 更短初始退避
            max_backoff_ms: 30_000,           // 更短最大退避
            health_check_interval_ms: 15_000, // 更频繁健康检查
        }
    }

    /// 弱网环境配置
    pub fn weak_network() -> Self {
        Self {
            connect_timeout_ms: 30_000,       // 30秒连接超时
            request_timeout_ms: 60_000,       // 60秒请求超时
            max_retries: 10,                  // 更多重试
            initial_backoff_ms: 2_000,        // 更长初始退避
            max_backoff_ms: 120_000,          // 2分钟最大退避
            health_check_interval_ms: 60_000, // 1分钟健康检查
        }
    }
}

pub struct NetworkMonitor {
    state: Arc<std::sync::RwLock<NetworkState>>,
    sender: watch::Sender<NetworkState>,
    receiver: watch::Receiver<NetworkState>,
    config: NetworkConfig,
    retry_count: AtomicU32,
    last_success_time: AtomicU64,
}

impl NetworkMonitor {
    pub fn new() -> Self {
        Self::with_config(NetworkConfig::default())
    }

    pub fn with_config(config: NetworkConfig) -> Self {
        let (sender, receiver) = watch::channel(NetworkState::Offline);
        Self {
            state: Arc::new(std::sync::RwLock::new(NetworkState::Offline)),
            sender,
            receiver,
            config,
            retry_count: AtomicU32::new(0),
            last_success_time: AtomicU64::new(0),
        }
    }

    pub fn state(&self) -> NetworkState {
        *self.state.read().unwrap()
    }

    pub fn set_state(&self, state: NetworkState) {
        let prev_state = *self.state.read().unwrap();
        *self.state.write().unwrap() = state;
        let _ = self.sender.send(state);

        // 状态变更日志
        if prev_state != state {
            log::info!(
                "[NetworkMonitor] State changed: {:?} -> {:?}",
                prev_state,
                state
            );
        }

        // 成功时重置重试计数
        if state == NetworkState::Online {
            self.retry_count.store(0, Ordering::SeqCst);
            self.last_success_time.store(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                Ordering::SeqCst,
            );
        }
    }

    pub fn is_online(&self) -> bool {
        matches!(self.state(), NetworkState::Online | NetworkState::Syncing)
    }

    pub fn subscribe(&self) -> watch::Receiver<NetworkState> {
        self.receiver.clone()
    }

    pub fn config(&self) -> &NetworkConfig {
        &self.config
    }

    /// 计算下次重试的退避时间
    pub fn next_backoff(&self) -> Duration {
        let retry = self.retry_count.fetch_add(1, Ordering::SeqCst);
        let backoff = self.config.initial_backoff_ms * 2u64.pow(retry.min(10));
        let capped = backoff.min(self.config.max_backoff_ms);

        // 添加 10% 抖动，避免惊群效应
        let jitter = (capped as f64 * 0.1 * rand_jitter()) as u64;
        Duration::from_millis(capped + jitter)
    }

    /// 是否应该重试
    pub fn should_retry(&self) -> bool {
        self.retry_count.load(Ordering::SeqCst) < self.config.max_retries
    }

    /// 重置重试计数
    pub fn reset_retries(&self) {
        self.retry_count.store(0, Ordering::SeqCst);
    }

    /// 获取连接超时
    pub fn connect_timeout(&self) -> Duration {
        Duration::from_millis(self.config.connect_timeout_ms)
    }

    /// 获取请求超时
    pub fn request_timeout(&self) -> Duration {
        Duration::from_millis(self.config.request_timeout_ms)
    }

    /// 获取健康检查间隔
    pub fn health_check_interval(&self) -> Duration {
        Duration::from_millis(self.config.health_check_interval_ms)
    }

    /// 获取自上次成功以来的秒数
    pub fn seconds_since_last_success(&self) -> u64 {
        let last = self.last_success_time.load(Ordering::SeqCst);
        if last == 0 {
            return u64::MAX;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(last)
    }
}

impl Default for NetworkMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// 简单的随机抖动（0.0 - 1.0）
fn rand_jitter() -> f64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64,
    );
    (hasher.finish() % 1000) as f64 / 1000.0
}
