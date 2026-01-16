use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct HybridLogicalClock {
    pub physical: u64,
    pub logical: u32,
    pub node_id: String,
}

impl HybridLogicalClock {
    pub fn new(node_id: String) -> Self {
        Self {
            physical: Self::now_millis(),
            logical: 0,
            node_id,
        }
    }

    fn now_millis() -> u64 {
        Utc::now().timestamp_millis() as u64
    }

    pub fn tick(&mut self) -> String {
        let now = Self::now_millis();
        
        if now > self.physical {
            self.physical = now;
            self.logical = 0;
        } else {
            self.logical += 1;
        }
        
        self.to_string()
    }

    pub fn update(&mut self, remote_hlc: &str) -> String {
        if let Some(remote) = Self::parse(remote_hlc) {
            let now = Self::now_millis();
            
            if now > self.physical && now > remote.physical {
                self.physical = now;
                self.logical = 0;
            } else if self.physical == remote.physical {
                self.logical = self.logical.max(remote.logical) + 1;
            } else if self.physical > remote.physical {
                self.logical += 1;
            } else {
                self.physical = remote.physical;
                self.logical = remote.logical + 1;
            }
        } else {
            self.tick();
        }
        
        self.to_string()
    }

    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 3 {
            return None;
        }
        
        Some(Self {
            physical: parts[0].parse().ok()?,
            logical: parts[1].parse().ok()?,
            node_id: parts[2].to_string(),
        })
    }

    pub fn compare(a: &str, b: &str) -> std::cmp::Ordering {
        let hlc_a = Self::parse(a);
        let hlc_b = Self::parse(b);
        
        match (hlc_a, hlc_b) {
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (None, None) => std::cmp::Ordering::Equal,
        }
    }
}

impl std::fmt::Display for HybridLogicalClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.physical, self.logical, self.node_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hlc_tick() {
        let mut hlc = HybridLogicalClock::new("node1".to_string());
        let t1 = hlc.tick();
        let t2 = hlc.tick();
        
        assert!(HybridLogicalClock::compare(&t2, &t1) == std::cmp::Ordering::Greater);
    }

    #[test]
    fn test_hlc_parse() {
        let hlc = HybridLogicalClock::new("node1".to_string());
        let s = hlc.to_string();
        let parsed = HybridLogicalClock::parse(&s).unwrap();
        
        assert_eq!(hlc.physical, parsed.physical);
        assert_eq!(hlc.logical, parsed.logical);
        assert_eq!(hlc.node_id, parsed.node_id);
    }
}
