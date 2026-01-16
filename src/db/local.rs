use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;

use super::schema::SYNC_TABLES_SQL;

pub struct LocalDb {
    conn: Mutex<Connection>,
}

impl LocalDb {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_sync_tables()?;
        Ok(db)
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_sync_tables()?;
        Ok(db)
    }

    fn init_sync_tables(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(SYNC_TABLES_SQL)?;
        Ok(())
    }

    /// 获取或创建节点 ID（持久化存储）
    pub fn get_or_create_node_id(&self) -> Result<String> {
        let conn = self.conn.lock().unwrap();

        // 尝试获取已存储的 node_id
        let existing: Option<String> = conn
            .query_row(
                "SELECT value FROM _sync_metadata WHERE key = 'node_id'",
                [],
                |row| row.get(0),
            )
            .ok();

        if let Some(node_id) = existing {
            return Ok(node_id);
        }

        // 生成新的 node_id 并存储
        let node_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT OR REPLACE INTO _sync_metadata (key, value) VALUES ('node_id', ?1)",
            rusqlite::params![&node_id],
        )?;

        log::info!("[LocalDb] Generated new node_id: {}", node_id);
        Ok(node_id)
    }

    /// 获取上次同步的 HLC
    pub fn get_last_sync_hlc(&self, table_name: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let key = format!("last_sync_hlc:{}", table_name);
        let result: Option<String> = conn
            .query_row(
                "SELECT value FROM _sync_metadata WHERE key = ?1",
                rusqlite::params![&key],
                |row| row.get(0),
            )
            .ok();
        Ok(result)
    }

    /// 设置上次同步的 HLC
    pub fn set_last_sync_hlc(&self, table_name: &str, hlc: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let key = format!("last_sync_hlc:{}", table_name);
        conn.execute(
            "INSERT OR REPLACE INTO _sync_metadata (key, value) VALUES (?1, ?2)",
            rusqlite::params![&key, hlc],
        )?;
        Ok(())
    }

    /// 在事务中执行操作
    pub fn transaction<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T>,
    {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let result = f(&tx)?;
        tx.commit()?;
        Ok(result)
    }

    pub fn execute(&self, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute(sql, params)?;
        Ok(affected)
    }

    pub fn query_one<T, F>(
        &self,
        sql: &str,
        params: &[&dyn rusqlite::ToSql],
        f: F,
    ) -> Result<Option<T>>
    where
        F: FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(sql)?;
        let mut rows = stmt.query(params)?;

        if let Some(row) = rows.next()? {
            Ok(Some(f(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn query_all<T, F>(
        &self,
        sql: &str,
        params: &[&dyn rusqlite::ToSql],
        f: F,
    ) -> Result<Vec<T>>
    where
        F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params, f)?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn record_change(
        &self,
        table_name: &str,
        row_id: &str,
        operation: &str,
        hlc: &str,
        payload: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO _sync_changelog (table_name, row_id, operation, hlc, payload) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![table_name, row_id, operation, hlc, payload],
        )?;
        Ok(())
    }

    pub fn get_unsynced_changes(&self) -> Result<Vec<super::schema::ChangelogEntry>> {
        use super::schema::{ChangelogEntry, Operation};

        self.query_all(
            "SELECT id, table_name, row_id, operation, hlc, payload, synced FROM _sync_changelog WHERE synced = 0 ORDER BY hlc",
            &[],
            |row| {
                let op_str: String = row.get(3)?;
                let operation = match op_str.as_str() {
                    "INSERT" => Operation::Insert,
                    "UPDATE" => Operation::Update,
                    "DELETE" => Operation::Delete,
                    _ => Operation::Update,
                };
                Ok(ChangelogEntry {
                    id: row.get(0)?,
                    table_name: row.get(1)?,
                    row_id: row.get(2)?,
                    operation,
                    hlc: row.get(4)?,
                    payload: row.get(5)?,
                    synced: row.get::<_, i32>(6)? != 0,
                })
            },
        )
    }

    pub fn mark_synced(&self, changelog_ids: &[i64]) -> Result<()> {
        if changelog_ids.is_empty() {
            return Ok(());
        }

        let placeholders: Vec<String> = changelog_ids.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "UPDATE _sync_changelog SET synced = 1 WHERE id IN ({})",
            placeholders.join(",")
        );

        let conn = self.conn.lock().unwrap();
        let params: Vec<&dyn rusqlite::ToSql> = changelog_ids
            .iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();
        conn.execute(&sql, params.as_slice())?;
        Ok(())
    }

    /// 确保表存在，不存在则自动创建（包含同步元字段）
    /// 
    /// 功能：
    /// 1. 表不存在 → 创建表并注册到 _schema_registry
    /// 2. 表存在但结构变更 → 记录警告（需要迁移）
    /// 3. 表存在且结构一致 → 直接返回
    /// 
    /// columns 格式: [["name", "TEXT"], ["age", "INTEGER"], ...]
    pub fn ensure_table(&self, table_name: &str, columns: &[(String, String)]) -> Result<()> {
        use super::schema::compute_schema_hash;
        
        let conn = self.conn.lock().unwrap();
        let new_hash = compute_schema_hash(columns);

        // 检查 schema registry 中是否有记录
        let registered: Option<(String, i64)> = conn
            .query_row(
                "SELECT schema_hash, version FROM _schema_registry WHERE table_name = ?1",
                params![table_name],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        if let Some((existing_hash, version)) = registered {
            if existing_hash != new_hash {
                // 结构变更检测
                log::warn!(
                    "[LocalDb] Schema mismatch for table '{}': registered hash={}, new hash={}. Version={}",
                    table_name, existing_hash, new_hash, version
                );
                // TODO: 未来可以支持自动迁移
            }
            return Ok(());
        }

        // 检查表是否存在（可能是旧版本没有 registry 的情况）
        let table_exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name=?1",
            params![table_name],
            |row| row.get(0),
        )?;

        if !table_exists {
            // 构建列定义
            let mut col_defs = vec!["id TEXT PRIMARY KEY".to_string()];

            for (name, typ) in columns {
                if name != "id" {
                    // JSON 类型在 SQLite 中存储为 TEXT
                    let sqlite_type = normalize_column_type(typ);
                    col_defs.push(format!("{} {}", name, sqlite_type));
                }
            }

            // 添加同步元字段
            col_defs.extend([
                "_hlc TEXT NOT NULL".to_string(),
                "_node_id TEXT NOT NULL".to_string(),
                "_version INTEGER DEFAULT 1".to_string(),
                "_deleted INTEGER DEFAULT 0".to_string(),
                "_synced INTEGER DEFAULT 0".to_string(),
            ]);

            let sql = format!("CREATE TABLE \"{}\" ({})", table_name, col_defs.join(", "));
            conn.execute(&sql, [])?;

            // 创建索引
            conn.execute(
                &format!(
                    "CREATE INDEX IF NOT EXISTS idx_{}_hlc ON \"{}\"(_hlc)",
                    table_name, table_name
                ),
                [],
            )?;
            conn.execute(
                &format!(
                    "CREATE INDEX IF NOT EXISTS idx_{}_synced ON \"{}\"(_synced)",
                    table_name, table_name
                ),
                [],
            )?;

            log::info!("[LocalDb] Created table: {}", table_name);
        }

        // 注册到 schema registry
        let columns_json = serde_json::to_string(columns).unwrap_or_default();
        conn.execute(
            "INSERT OR REPLACE INTO _schema_registry (table_name, columns, schema_hash, version, updated_at) VALUES (?1, ?2, ?3, 1, datetime('now'))",
            params![table_name, columns_json, new_hash],
        )?;

        log::info!("[LocalDb] Registered schema for table: {} (hash={})", table_name, new_hash);
        Ok(())
    }
    
    /// 获取注册的表结构
    pub fn get_registered_schema(&self, table_name: &str) -> Result<Option<Vec<(String, String)>>> {
        let conn = self.conn.lock().unwrap();
        let columns_json: Option<String> = conn
            .query_row(
                "SELECT columns FROM _schema_registry WHERE table_name = ?1",
                params![table_name],
                |row| row.get(0),
            )
            .ok();
        
        if let Some(json) = columns_json {
            let columns: Vec<(String, String)> = serde_json::from_str(&json).unwrap_or_default();
            Ok(Some(columns))
        } else {
            Ok(None)
        }
    }
    
    /// 列出所有注册的表
    pub fn list_registered_tables(&self) -> Result<Vec<String>> {
        self.query_all(
            "SELECT table_name FROM _schema_registry ORDER BY table_name",
            &[],
            |row| row.get(0),
        )
    }

    /// 插入数据（自动生成 id 和同步元数据）
    pub fn insert(
        &self,
        table_name: &str,
        data: &serde_json::Value,
        hlc: &str,
        node_id: &str,
    ) -> Result<String> {
        let id = data
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let conn = self.conn.lock().unwrap();

        // 获取表的列信息
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table_name))?;
        let columns: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);

        let mut col_names = Vec::new();
        let mut placeholders = Vec::new();
        let mut values: Vec<String> = Vec::new();

        for (i, col) in columns.iter().enumerate() {
            col_names.push(col.as_str());
            placeholders.push(format!("?{}", i + 1));

            let value = match col.as_str() {
                "id" => id.clone(),
                "_hlc" => hlc.to_string(),
                "_node_id" => node_id.to_string(),
                "_version" => "1".to_string(),
                "_deleted" => "0".to_string(),
                "_synced" => "0".to_string(),
                other => data
                    .get(other)
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Null => String::new(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default(),
            };
            values.push(value);
        }

        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table_name,
            col_names.join(", "),
            placeholders.join(", ")
        );

        let params: Vec<&dyn rusqlite::ToSql> =
            values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

        conn.execute(&sql, params.as_slice())?;

        Ok(id)
    }

    /// 更新数据
    pub fn update(
        &self,
        table_name: &str,
        id: &str,
        data: &serde_json::Value,
        hlc: &str,
    ) -> Result<bool> {
        let conn = self.conn.lock().unwrap();

        let mut sets = vec![
            "_hlc = ?1".to_string(),
            "_version = _version + 1".to_string(),
            "_synced = 0".to_string(),
        ];
        let mut values: Vec<String> = vec![hlc.to_string()];
        let mut idx = 2;

        if let Some(obj) = data.as_object() {
            for (key, value) in obj {
                if !key.starts_with('_') && key != "id" {
                    sets.push(format!("{} = ?{}", key, idx));
                    values.push(match value {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Null => String::new(),
                        other => other.to_string(),
                    });
                    idx += 1;
                }
            }
        }

        values.push(id.to_string());

        let sql = format!(
            "UPDATE {} SET {} WHERE id = ?{} AND _deleted = 0",
            table_name,
            sets.join(", "),
            idx
        );

        let params: Vec<&dyn rusqlite::ToSql> =
            values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

        let affected = conn.execute(&sql, params.as_slice())?;
        Ok(affected > 0)
    }

    /// 软删除数据
    pub fn delete(&self, table_name: &str, id: &str, hlc: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute(
            &format!(
                "UPDATE {} SET _deleted = 1, _hlc = ?1, _synced = 0 WHERE id = ?2",
                table_name
            ),
            params![hlc, id],
        )?;
        Ok(affected > 0)
    }

    /// 查询单条数据
    pub fn find_by_id(&self, table_name: &str, id: &str) -> Result<Option<serde_json::Value>> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT * FROM {} WHERE id = ?1 AND _deleted = 0",
            table_name
        );

        let mut stmt = conn.prepare(&sql)?;
        let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

        let result = stmt.query_row(params![id], |row| {
            let mut obj = serde_json::Map::new();
            for (i, name) in column_names.iter().enumerate() {
                let value: rusqlite::types::Value = row.get(i)?;
                obj.insert(name.clone(), sqlite_value_to_json(value));
            }
            Ok(serde_json::Value::Object(obj))
        });

        match result {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// 查询所有数据
    pub fn find_all(
        &self,
        table_name: &str,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<serde_json::Value>> {
        let conn = self.conn.lock().unwrap();
        let mut sql = format!(
            "SELECT * FROM {} WHERE _deleted = 0 ORDER BY _hlc DESC",
            table_name
        );

        if let Some(l) = limit {
            sql.push_str(&format!(" LIMIT {}", l));
        }
        if let Some(o) = offset {
            sql.push_str(&format!(" OFFSET {}", o));
        }

        let mut stmt = conn.prepare(&sql)?;
        let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

        let rows = stmt.query_map([], |row| {
            let mut obj = serde_json::Map::new();
            for (i, name) in column_names.iter().enumerate() {
                let value: rusqlite::types::Value = row.get(i)?;
                obj.insert(name.clone(), sqlite_value_to_json(value));
            }
            Ok(serde_json::Value::Object(obj))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// 条件查询
    pub fn find_where(
        &self,
        table_name: &str,
        conditions: &serde_json::Value,
    ) -> Result<Vec<serde_json::Value>> {
        let conn = self.conn.lock().unwrap();

        let mut where_clauses = vec!["_deleted = 0".to_string()];
        let mut values: Vec<String> = Vec::new();
        let mut idx = 1;

        if let Some(obj) = conditions.as_object() {
            for (key, value) in obj {
                where_clauses.push(format!("{} = ?{}", key, idx));
                values.push(match value {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                });
                idx += 1;
            }
        }

        let sql = format!(
            "SELECT * FROM {} WHERE {} ORDER BY _hlc DESC",
            table_name,
            where_clauses.join(" AND ")
        );

        let mut stmt = conn.prepare(&sql)?;
        let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

        let params: Vec<&dyn rusqlite::ToSql> =
            values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

        let rows = stmt.query_map(params.as_slice(), |row| {
            let mut obj = serde_json::Map::new();
            for (i, name) in column_names.iter().enumerate() {
                let value: rusqlite::types::Value = row.get(i)?;
                obj.insert(name.clone(), sqlite_value_to_json(value));
            }
            Ok(serde_json::Value::Object(obj))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// 高级查询（支持模糊查询、排序、分页）
    pub fn query(
        &self,
        table_name: &str,
        options: &QueryOptions,
    ) -> Result<Vec<serde_json::Value>> {
        let conn = self.conn.lock().unwrap();

        let mut where_clauses = vec!["_deleted = 0".to_string()];
        let mut values: Vec<String> = Vec::new();
        let mut idx = 1;

        // 精确匹配条件
        if let Some(eq) = &options.where_eq {
            if let Some(obj) = eq.as_object() {
                for (key, value) in obj {
                    where_clauses.push(format!("{} = ?{}", key, idx));
                    values.push(json_value_to_string(value));
                    idx += 1;
                }
            }
        }

        // 模糊查询条件 (LIKE)
        if let Some(like) = &options.where_like {
            if let Some(obj) = like.as_object() {
                for (key, value) in obj {
                    where_clauses.push(format!("{} LIKE ?{}", key, idx));
                    let pattern = format!("%{}%", json_value_to_string(value));
                    values.push(pattern);
                    idx += 1;
                }
            }
        }

        // 范围查询 (>=)
        if let Some(gte) = &options.where_gte {
            if let Some(obj) = gte.as_object() {
                for (key, value) in obj {
                    where_clauses.push(format!("{} >= ?{}", key, idx));
                    values.push(json_value_to_string(value));
                    idx += 1;
                }
            }
        }

        // 范围查询 (<=)
        if let Some(lte) = &options.where_lte {
            if let Some(obj) = lte.as_object() {
                for (key, value) in obj {
                    where_clauses.push(format!("{} <= ?{}", key, idx));
                    values.push(json_value_to_string(value));
                    idx += 1;
                }
            }
        }

        // 构建 ORDER BY
        let order_by = if let Some(ref order) = options.order_by {
            let dir = if options.order_desc.unwrap_or(false) {
                "DESC"
            } else {
                "ASC"
            };
            format!("ORDER BY {} {}", order, dir)
        } else {
            "ORDER BY _hlc DESC".to_string()
        };

        // 构建 LIMIT/OFFSET
        let mut limit_clause = String::new();
        if let Some(limit) = options.limit {
            limit_clause.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = options.offset {
            limit_clause.push_str(&format!(" OFFSET {}", offset));
        }

        let sql = format!(
            "SELECT * FROM {} WHERE {} {} {}",
            table_name,
            where_clauses.join(" AND "),
            order_by,
            limit_clause
        );

        let mut stmt = conn.prepare(&sql)?;
        let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

        let params: Vec<&dyn rusqlite::ToSql> =
            values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

        let rows = stmt.query_map(params.as_slice(), |row| {
            let mut obj = serde_json::Map::new();
            for (i, name) in column_names.iter().enumerate() {
                let value: rusqlite::types::Value = row.get(i)?;
                obj.insert(name.clone(), sqlite_value_to_json(value));
            }
            Ok(serde_json::Value::Object(obj))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// 批量插入
    pub fn insert_many(
        &self,
        table_name: &str,
        items: &[serde_json::Value],
        hlc_prefix: &str,
        node_id: &str,
    ) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();

        // 获取表的列信息
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table_name))?;
        let columns: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);

        let mut ids = Vec::with_capacity(items.len());

        for (idx, data) in items.iter().enumerate() {
            let id = data
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

            let hlc = format!("{}_{:06}", hlc_prefix, idx);

            let mut col_names = Vec::new();
            let mut placeholders = Vec::new();
            let mut values: Vec<String> = Vec::new();

            for (i, col) in columns.iter().enumerate() {
                col_names.push(col.as_str());
                placeholders.push(format!("?{}", i + 1));

                let value = match col.as_str() {
                    "id" => id.clone(),
                    "_hlc" => hlc.clone(),
                    "_node_id" => node_id.to_string(),
                    "_version" => "1".to_string(),
                    "_deleted" => "0".to_string(),
                    "_synced" => "0".to_string(),
                    other => data
                        .get(other)
                        .map(json_value_to_string)
                        .unwrap_or_default(),
                };
                values.push(value);
            }

            let sql = format!(
                "INSERT INTO {} ({}) VALUES ({})",
                table_name,
                col_names.join(", "),
                placeholders.join(", ")
            );

            let params: Vec<&dyn rusqlite::ToSql> =
                values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

            conn.execute(&sql, params.as_slice())?;
            ids.push(id);
        }

        Ok(ids)
    }

    /// 批量更新
    pub fn update_many(
        &self,
        table_name: &str,
        updates: &[(String, serde_json::Value)],
        hlc_prefix: &str,
    ) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut updated = 0;

        for (idx, (id, data)) in updates.iter().enumerate() {
            let hlc = format!("{}_{:06}", hlc_prefix, idx);

            let mut sets = vec![
                "_hlc = ?1".to_string(),
                "_version = _version + 1".to_string(),
                "_synced = 0".to_string(),
            ];
            let mut values: Vec<String> = vec![hlc];
            let mut param_idx = 2;

            if let Some(obj) = data.as_object() {
                for (key, value) in obj {
                    if !key.starts_with('_') && key != "id" {
                        sets.push(format!("{} = ?{}", key, param_idx));
                        values.push(json_value_to_string(value));
                        param_idx += 1;
                    }
                }
            }

            values.push(id.clone());

            let sql = format!(
                "UPDATE {} SET {} WHERE id = ?{} AND _deleted = 0",
                table_name,
                sets.join(", "),
                param_idx
            );

            let params: Vec<&dyn rusqlite::ToSql> =
                values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

            let affected = conn.execute(&sql, params.as_slice())?;
            updated += affected;
        }

        Ok(updated)
    }

    /// 批量删除
    pub fn delete_many(&self, table_name: &str, ids: &[String], hlc_prefix: &str) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut deleted = 0;

        for (idx, id) in ids.iter().enumerate() {
            let hlc = format!("{}_{:06}", hlc_prefix, idx);

            let affected = conn.execute(
                &format!("UPDATE {} SET _deleted = 1, _hlc = ?1, _synced = 0 WHERE id = ?2 AND _deleted = 0", table_name),
                params![hlc, id],
            )?;
            deleted += affected;
        }

        Ok(deleted)
    }

    /// 清空表（软删除所有数据）
    pub fn clear_table(&self, table_name: &str, hlc: &str) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute(
            &format!(
                "UPDATE {} SET _deleted = 1, _hlc = ?1, _synced = 0 WHERE _deleted = 0",
                table_name
            ),
            params![hlc],
        )?;
        Ok(affected)
    }

    /// 物理删除已标记删除的数据（清理）
    ///
    /// - `days_old`: 删除多少天前的数据，None 表示删除所有已标记删除的数据
    /// - 返回删除的记录数
    pub fn purge_deleted(&self, table_name: &str, days_old: Option<i64>) -> Result<usize> {
        let conn = self.conn.lock().unwrap();

        let sql = if let Some(days) = days_old {
            format!(
                "DELETE FROM {} WHERE _deleted = 1 AND datetime(_hlc/1000, 'unixepoch') < datetime('now', '-{} days')",
                table_name, days
            )
        } else {
            format!("DELETE FROM {} WHERE _deleted = 1", table_name)
        };

        let affected = conn.execute(&sql, [])?;
        log::info!(
            "[LocalDb] Purged {} deleted records from {}",
            affected,
            table_name
        );
        Ok(affected)
    }

    /// 清理所有表的已删除数据
    pub fn purge_all_deleted(&self, days_old: Option<i64>) -> Result<usize> {
        let tables = self.list_tables()?;
        let mut total = 0;

        for table in tables {
            total += self.purge_deleted(&table, days_old)?;
        }

        // 同时清理已同步的 changelog（保留最近的）
        let conn = self.conn.lock().unwrap();
        let changelog_deleted = if let Some(days) = days_old {
            conn.execute(
                &format!("DELETE FROM _sync_changelog WHERE synced = 1 AND datetime(created_at) < datetime('now', '-{} days')", days),
                [],
            )?
        } else {
            conn.execute("DELETE FROM _sync_changelog WHERE synced = 1", [])?
        };

        log::info!("[LocalDb] Purged {} changelog entries", changelog_deleted);

        // 执行 VACUUM 回收空间
        conn.execute("VACUUM", [])?;

        Ok(total)
    }

    /// 获取已删除数据的统计信息
    pub fn get_deleted_stats(&self, table_name: &str) -> Result<serde_json::Value> {
        let conn = self.conn.lock().unwrap();

        let total: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {} WHERE _deleted = 1", table_name),
            [],
            |row| row.get(0),
        )?;

        let oldest: Option<String> = conn
            .query_row(
                &format!("SELECT MIN(_hlc) FROM {} WHERE _deleted = 1", table_name),
                [],
                |row| row.get(0),
            )
            .ok();

        Ok(serde_json::json!({
            "table": table_name,
            "deleted_count": total,
            "oldest_hlc": oldest
        }))
    }

    /// 统计数量
    pub fn count(&self, table_name: &str, conditions: Option<&serde_json::Value>) -> Result<i64> {
        let conn = self.conn.lock().unwrap();

        let mut where_clauses = vec!["_deleted = 0".to_string()];
        let mut values: Vec<String> = Vec::new();
        let mut idx = 1;

        if let Some(cond) = conditions {
            if let Some(obj) = cond.as_object() {
                for (key, value) in obj {
                    where_clauses.push(format!("{} = ?{}", key, idx));
                    values.push(json_value_to_string(value));
                    idx += 1;
                }
            }
        }

        let sql = format!(
            "SELECT COUNT(*) FROM {} WHERE {}",
            table_name,
            where_clauses.join(" AND ")
        );

        let params: Vec<&dyn rusqlite::ToSql> =
            values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

        let count: i64 = conn.query_row(&sql, params.as_slice(), |row| row.get(0))?;
        Ok(count)
    }
}

/// 高级查询选项
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct QueryOptions {
    /// 精确匹配 { field: value }
    pub where_eq: Option<serde_json::Value>,
    /// 模糊查询 { field: "keyword" } -> LIKE '%keyword%'
    pub where_like: Option<serde_json::Value>,
    /// 大于等于 { field: value }
    pub where_gte: Option<serde_json::Value>,
    /// 小于等于 { field: value }
    pub where_lte: Option<serde_json::Value>,
    /// 排序字段
    pub order_by: Option<String>,
    /// 是否降序
    pub order_desc: Option<bool>,
    /// 限制数量
    pub limit: Option<i64>,
    /// 偏移量
    pub offset: Option<i64>,
}

fn json_value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => {
            if *b {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        _ => value.to_string(),
    }
}

/// 列定义（与 remote 共用）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default: Option<String>,
}

impl LocalDb {
    /// 获取本地表结构
    pub fn get_table_schema(&self, table_name: &str) -> Result<Vec<ColumnDef>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table_name))?;
        let columns = stmt.query_map([], |row| {
            let name: String = row.get(1)?;
            let data_type: String = row.get(2)?;
            let notnull: i32 = row.get(3)?;
            let dflt_value: Option<String> = row.get(4)?;

            Ok(ColumnDef {
                name,
                data_type,
                nullable: notnull == 0,
                default: dflt_value,
            })
        })?;

        let mut result = Vec::new();
        for col in columns {
            result.push(col?);
        }
        Ok(result)
    }

    /// 检查本地表是否存在
    pub fn table_exists(&self, table_name: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name=?1",
            params![table_name],
            |row| row.get(0),
        )?;
        Ok(exists)
    }

    /// 获取所有本地表名
    pub fn list_tables(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE '\\_%' ESCAPE '\\' ORDER BY name"
        )?;

        let tables = stmt.query_map([], |row| row.get(0))?;
        let mut result = Vec::new();
        for table in tables {
            result.push(table?);
        }
        Ok(result)
    }

    /// 根据远程表结构创建本地表
    pub fn create_table_from_remote(&self, table_name: &str, columns: &[ColumnDef]) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        let mut col_defs = Vec::new();

        for col in columns {
            let sqlite_type = super::remote::pg_type_to_sqlite(&col.data_type);
            let nullable = if col.nullable { "" } else { " NOT NULL" };
            let default = col
                .default
                .as_ref()
                .map(|d| format!(" DEFAULT {}", convert_pg_default_to_sqlite(d)))
                .unwrap_or_default();

            if col.name == "id" {
                col_defs.push("id TEXT PRIMARY KEY".to_string());
            } else {
                col_defs.push(format!(
                    "{} {}{}{}",
                    col.name, sqlite_type, nullable, default
                ));
            }
        }

        // 确保同步元字段存在
        let meta_fields = ["_hlc", "_node_id", "_version", "_deleted", "_synced"];
        for meta in meta_fields {
            if !columns.iter().any(|c| c.name == meta) {
                match meta {
                    "_hlc" => col_defs.push("_hlc TEXT NOT NULL".to_string()),
                    "_node_id" => col_defs.push("_node_id TEXT NOT NULL".to_string()),
                    "_version" => col_defs.push("_version INTEGER DEFAULT 1".to_string()),
                    "_deleted" => col_defs.push("_deleted INTEGER DEFAULT 0".to_string()),
                    "_synced" => col_defs.push("_synced INTEGER DEFAULT 0".to_string()),
                    _ => {}
                }
            }
        }

        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {} ({})",
            table_name,
            col_defs.join(", ")
        );

        conn.execute(&sql, [])?;

        // 创建索引
        conn.execute(
            &format!(
                "CREATE INDEX IF NOT EXISTS idx_{}_hlc ON {}(_hlc)",
                table_name, table_name
            ),
            [],
        )?;
        conn.execute(
            &format!(
                "CREATE INDEX IF NOT EXISTS idx_{}_synced ON {}(_synced)",
                table_name, table_name
            ),
            [],
        )?;

        log::info!("[LocalDb] Created table from remote: {}", table_name);
        Ok(())
    }
}

/// 标准化列类型（将用户友好类型转换为 SQLite 类型）
/// 
/// 支持的类型映射：
/// - JSON, JSONB → TEXT（自动序列化/反序列化）
/// - BOOLEAN, BOOL → INTEGER
/// - UUID → TEXT
/// - TIMESTAMP, DATETIME → TEXT
/// - FLOAT, DOUBLE → REAL
/// - INT, BIGINT, SMALLINT → INTEGER
fn normalize_column_type(user_type: &str) -> &'static str {
    match user_type.to_uppercase().as_str() {
        // JSON 类型 → TEXT（在应用层处理序列化）
        "JSON" | "JSONB" => "TEXT",
        // 布尔类型 → INTEGER (0/1)
        "BOOLEAN" | "BOOL" => "INTEGER",
        // UUID → TEXT
        "UUID" => "TEXT",
        // 时间类型 → TEXT
        "TIMESTAMP" | "TIMESTAMPTZ" | "DATETIME" | "DATE" | "TIME" => "TEXT",
        // 浮点类型 → REAL
        "FLOAT" | "DOUBLE" | "DECIMAL" | "NUMERIC" | "FLOAT4" | "FLOAT8" => "REAL",
        // 整数类型 → INTEGER
        "INT" | "INT4" | "INT8" | "BIGINT" | "SMALLINT" | "SERIAL" | "BIGSERIAL" => "INTEGER",
        // 文本类型
        "VARCHAR" | "CHAR" | "CHARACTER VARYING" => "TEXT",
        // 已经是 SQLite 类型，直接返回
        "TEXT" => "TEXT",
        "INTEGER" => "INTEGER",
        "REAL" => "REAL",
        "BLOB" => "BLOB",
        // 未知类型默认为 TEXT
        _ => "TEXT",
    }
}

/// 检查列类型是否为 JSON 类型
fn is_json_type(user_type: &str) -> bool {
    matches!(user_type.to_uppercase().as_str(), "JSON" | "JSONB")
}

/// 转换 PostgreSQL 默认值为 SQLite 格式
fn convert_pg_default_to_sqlite(pg_default: &str) -> String {
    // 处理常见的 PostgreSQL 默认值
    if pg_default.contains("gen_random_uuid()") {
        return "''".to_string(); // SQLite 不支持，留空
    }
    if pg_default.to_lowercase().contains("now()")
        || pg_default.to_lowercase().contains("current_timestamp")
    {
        return "CURRENT_TIMESTAMP".to_string();
    }
    if pg_default == "true" {
        return "1".to_string();
    }
    if pg_default == "false" {
        return "0".to_string();
    }
    // 移除类型转换 ::type
    if let Some(pos) = pg_default.find("::") {
        return pg_default[..pos].to_string();
    }
    pg_default.to_string()
}

fn sqlite_value_to_json(value: rusqlite::types::Value) -> serde_json::Value {
    match value {
        rusqlite::types::Value::Null => serde_json::Value::Null,
        rusqlite::types::Value::Integer(i) => serde_json::Value::Number(i.into()),
        rusqlite::types::Value::Real(f) => serde_json::json!(f),
        rusqlite::types::Value::Text(s) => serde_json::Value::String(s),
        rusqlite::types::Value::Blob(b) => serde_json::Value::String(bytes_to_hex(&b)),
    }
}

/// 将二进制数据编码为 hex 字符串（跨平台兼容）
fn bytes_to_hex(data: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(data.len() * 2);
    for byte in data {
        write!(s, "{:02x}", byte).unwrap();
    }
    s
}

/// 将 hex 字符串解码为二进制数据
#[allow(dead_code)]
fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}
