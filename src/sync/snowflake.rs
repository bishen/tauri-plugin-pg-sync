//! 雪花ID生成器
//! 
//! 64位结构：
//! - 1位符号位（固定0）
//! - 41位时间戳（毫秒，可用69年）
//! - 10位机器ID（从node_id哈希生成）
//! - 12位序列号（每毫秒4096个ID）

use std::sync::atomic::{AtomicI64, AtomicU16, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// 起始时间戳: 2024-01-01 00:00:00 UTC
const EPOCH: i64 = 1704067200000;

/// 机器ID位数
const MACHINE_ID_BITS: u8 = 10;
/// 序列号位数
const SEQUENCE_BITS: u8 = 12;

/// 机器ID最大值
const MAX_MACHINE_ID: u16 = (1 << MACHINE_ID_BITS) - 1;
/// 序列号最大值
const MAX_SEQUENCE: u16 = (1 << SEQUENCE_BITS) - 1;

/// 雪花ID生成器
pub struct SnowflakeGenerator {
    machine_id: u16,
    last_timestamp: AtomicI64,
    sequence: AtomicU16,
}

impl SnowflakeGenerator {
    /// 从 node_id 创建生成器
    pub fn from_node_id(node_id: &str) -> Self {
        // 使用简单哈希从 node_id 生成 machine_id
        let hash: u32 = node_id.bytes().fold(0u32, |acc, b| {
            acc.wrapping_mul(31).wrapping_add(b as u32)
        });
        let machine_id = (hash as u16) & MAX_MACHINE_ID;
        
        Self {
            machine_id,
            last_timestamp: AtomicI64::new(0),
            sequence: AtomicU16::new(0),
        }
    }

    /// 生成下一个雪花ID
    pub fn next_id(&self) -> i64 {
        let mut timestamp = current_timestamp();
        let last_ts = self.last_timestamp.load(Ordering::SeqCst);

        if timestamp == last_ts {
            // 同一毫秒内，序列号递增
            let seq = self.sequence.fetch_add(1, Ordering::SeqCst) & MAX_SEQUENCE;
            if seq == 0 {
                // 序列号溢出，等待下一毫秒
                timestamp = wait_next_millis(last_ts);
            }
        } else {
            // 新的毫秒，重置序列号
            self.sequence.store(0, Ordering::SeqCst);
        }

        self.last_timestamp.store(timestamp, Ordering::SeqCst);
        let sequence = self.sequence.load(Ordering::SeqCst);

        // 组装ID
        ((timestamp - EPOCH) << (MACHINE_ID_BITS + SEQUENCE_BITS) as i64)
            | ((self.machine_id as i64) << SEQUENCE_BITS as i64)
            | (sequence as i64)
    }

    /// 生成ID并返回字符串
    pub fn next_id_string(&self) -> String {
        self.next_id().to_string()
    }
}

/// 获取当前时间戳（毫秒）
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as i64
}

/// 等待下一毫秒
fn wait_next_millis(last_ts: i64) -> i64 {
    let mut ts = current_timestamp();
    while ts <= last_ts {
        std::hint::spin_loop();
        ts = current_timestamp();
    }
    ts
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_unique_ids() {
        let gen = SnowflakeGenerator::from_node_id("test-node-123");
        let mut ids = HashSet::new();
        
        for _ in 0..10000 {
            let id = gen.next_id();
            assert!(ids.insert(id), "Duplicate ID generated: {}", id);
        }
    }

    #[test]
    fn test_id_ordering() {
        let gen = SnowflakeGenerator::from_node_id("test-node");
        let id1 = gen.next_id();
        let id2 = gen.next_id();
        assert!(id2 > id1, "IDs should be monotonically increasing");
    }
}
