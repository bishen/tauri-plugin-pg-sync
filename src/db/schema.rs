use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMetadata {
    pub hlc: String,
    pub node_id: String,
    pub version: i64,
    pub deleted: bool,
    pub synced: bool,
    pub conflict: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    Insert,
    Update,
    Delete,
}

impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Operation::Insert => write!(f, "INSERT"),
            Operation::Update => write!(f, "UPDATE"),
            Operation::Delete => write!(f, "DELETE"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogEntry {
    pub id: i64,
    pub table_name: String,
    pub row_id: String,
    pub operation: Operation,
    pub hlc: String,
    pub payload: Option<String>,
    pub synced: bool,
}

pub const SYNC_TABLES_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS _sync_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS _sync_changelog (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    table_name TEXT NOT NULL,
    row_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    hlc TEXT NOT NULL,
    payload TEXT,
    synced INTEGER DEFAULT 0,
    created_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS _sync_conflicts (
    id TEXT PRIMARY KEY,
    table_name TEXT NOT NULL,
    row_id TEXT NOT NULL,
    local_value TEXT NOT NULL,
    remote_value TEXT NOT NULL,
    local_hlc TEXT NOT NULL,
    remote_hlc TEXT NOT NULL,
    resolved INTEGER DEFAULT 0,
    resolution TEXT,
    created_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_changelog_synced ON _sync_changelog(synced);
CREATE INDEX IF NOT EXISTS idx_changelog_hlc ON _sync_changelog(hlc);
CREATE INDEX IF NOT EXISTS idx_changelog_table ON _sync_changelog(table_name);
CREATE INDEX IF NOT EXISTS idx_conflicts_resolved ON _sync_conflicts(resolved);
"#;
