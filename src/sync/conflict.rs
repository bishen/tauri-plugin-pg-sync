use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::hlc::HybridLogicalClock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConflictStrategy {
    LastWriteWins,
    FirstWriteWins,
    Manual,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictRecord {
    pub id: String,
    pub table_name: String,
    pub row_id: String,
    pub local_value: Value,
    pub remote_value: Value,
    pub local_hlc: String,
    pub remote_hlc: String,
    pub resolved: bool,
    pub resolution: Option<Value>,
}

pub struct ConflictResolver {
    default_strategy: ConflictStrategy,
}

impl ConflictResolver {
    pub fn new(strategy: ConflictStrategy) -> Self {
        Self {
            default_strategy: strategy,
        }
    }

    pub fn resolve(
        &self,
        local: &Value,
        remote: &Value,
        local_hlc: &str,
        remote_hlc: &str,
    ) -> ResolveResult {
        match &self.default_strategy {
            ConflictStrategy::LastWriteWins => {
                self.last_write_wins(local, remote, local_hlc, remote_hlc)
            }
            ConflictStrategy::FirstWriteWins => {
                self.first_write_wins(local, remote, local_hlc, remote_hlc)
            }
            ConflictStrategy::Manual => ResolveResult::Conflict(ConflictRecord {
                id: uuid::Uuid::new_v4().to_string(),
                table_name: String::new(),
                row_id: local
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                local_value: local.clone(),
                remote_value: remote.clone(),
                local_hlc: local_hlc.to_string(),
                remote_hlc: remote_hlc.to_string(),
                resolved: false,
                resolution: None,
            }),
            ConflictStrategy::Custom(_) => {
                self.last_write_wins(local, remote, local_hlc, remote_hlc)
            }
        }
    }

    fn last_write_wins(
        &self,
        local: &Value,
        remote: &Value,
        local_hlc: &str,
        remote_hlc: &str,
    ) -> ResolveResult {
        match HybridLogicalClock::compare(local_hlc, remote_hlc) {
            std::cmp::Ordering::Greater => {
                ResolveResult::UseLocal(local.clone())
            }
            std::cmp::Ordering::Equal => {
                // HLC 相等时，检查 _node_id 是否不同
                // 如果远程来自 server，优先使用远程（服务端管理员修改）
                let local_node = local.get("_node_id").and_then(|v| v.as_str()).unwrap_or("");
                let remote_node = remote.get("_node_id").and_then(|v| v.as_str()).unwrap_or("");
                
                if remote_node == "server" && local_node != "server" {
                    // 服务端直接修改的数据，优先使用
                    ResolveResult::UseRemote(remote.clone())
                } else if local == remote {
                    // 数据完全相同，保留本地
                    ResolveResult::UseLocal(local.clone())
                } else {
                    // HLC 相同但数据不同，使用远程（可能是服务端修改）
                    ResolveResult::UseRemote(remote.clone())
                }
            }
            std::cmp::Ordering::Less => ResolveResult::UseRemote(remote.clone()),
        }
    }

    fn first_write_wins(
        &self,
        local: &Value,
        remote: &Value,
        local_hlc: &str,
        remote_hlc: &str,
    ) -> ResolveResult {
        match HybridLogicalClock::compare(local_hlc, remote_hlc) {
            std::cmp::Ordering::Less | std::cmp::Ordering::Equal => {
                ResolveResult::UseLocal(local.clone())
            }
            std::cmp::Ordering::Greater => ResolveResult::UseRemote(remote.clone()),
        }
    }

    pub fn merge_fields(
        &self,
        local: &Value,
        remote: &Value,
        prefer_local_fields: &[&str],
    ) -> Value {
        let mut result = remote.clone();

        if let (Some(local_obj), Some(result_obj)) = (local.as_object(), result.as_object_mut()) {
            for field in prefer_local_fields {
                if let Some(local_val) = local_obj.get(*field) {
                    result_obj.insert(field.to_string(), local_val.clone());
                }
            }
        }

        result
    }
}

#[derive(Debug)]
pub enum ResolveResult {
    UseLocal(Value),
    UseRemote(Value),
    Merged(Value),
    Conflict(ConflictRecord),
}
