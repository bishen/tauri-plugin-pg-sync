use anyhow::Result;
use sqlx::postgres::{PgPool, PgPoolOptions, PgListener};
use sqlx::Row;
use std::time::Duration;

pub struct RemoteDb {
    pool: PgPool,
    database_url: String,
}

impl RemoteDb {
    /// 使用默认超时连接
    pub async fn connect(database_url: &str) -> Result<Self> {
        Self::connect_with_timeout(database_url, Duration::from_secs(10)).await
    }

    /// 使用指定超时连接
    pub async fn connect_with_timeout(database_url: &str, timeout: Duration) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .min_connections(1)
            .acquire_timeout(timeout)
            .idle_timeout(Duration::from_secs(300))  // 5分钟空闲超时
            .max_lifetime(Duration::from_secs(1800)) // 30分钟最大生命周期
            .connect(database_url)
            .await?;
        
        log::info!("[RemoteDb] Connected to PostgreSQL");
        
        Ok(Self { 
            pool,
            database_url: database_url.to_string(),
        })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// 创建 PostgreSQL 监听器（用于实时通知）
    pub async fn create_listener(&self) -> Result<PgListener> {
        let listener = PgListener::connect(&self.database_url).await?;
        Ok(listener)
    }

    pub async fn execute(&self, sql: &str) -> Result<u64> {
        let result = sqlx::query(sql).execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    /// 检查远程表是否有同步元字段
    pub async fn has_sync_columns(&self, table_name: &str) -> Result<bool> {
        let schema = self.get_table_schema(table_name).await?;
        let column_names: Vec<&str> = schema.iter().map(|c| c.name.as_str()).collect();
        
        Ok(column_names.contains(&"_hlc") && 
           column_names.contains(&"_node_id") && 
           column_names.contains(&"_version") &&
           column_names.contains(&"_deleted"))
    }

    /// 为现有表添加同步元字段
    pub async fn add_sync_columns(&self, table_name: &str) -> Result<()> {
        let schema = self.get_table_schema(table_name).await?;
        let column_names: Vec<&str> = schema.iter().map(|c| c.name.as_str()).collect();
        
        let mut alterations = Vec::new();
        
        if !column_names.contains(&"_hlc") {
            alterations.push(format!(r#"ALTER TABLE "{}" ADD COLUMN IF NOT EXISTS "_hlc" TEXT"#, table_name));
        }
        if !column_names.contains(&"_node_id") {
            alterations.push(format!(r#"ALTER TABLE "{}" ADD COLUMN IF NOT EXISTS "_node_id" TEXT"#, table_name));
        }
        if !column_names.contains(&"_version") {
            alterations.push(format!(r#"ALTER TABLE "{}" ADD COLUMN IF NOT EXISTS "_version" BIGINT DEFAULT 1"#, table_name));
        }
        if !column_names.contains(&"_deleted") {
            alterations.push(format!(r#"ALTER TABLE "{}" ADD COLUMN IF NOT EXISTS "_deleted" BOOLEAN DEFAULT FALSE"#, table_name));
        }
        
        for sql in alterations {
            self.execute(&sql).await?;
        }
        
        log::info!("[RemoteDb] Added sync columns to table: {}", table_name);
        Ok(())
    }

    /// 获取自指定 HLC 以来的变更
    /// 
    /// 如果表没有 _hlc 列，会先尝试添加同步元字段
    pub async fn fetch_changes_since(&self, table_name: &str, since_hlc: &str) -> Result<Vec<serde_json::Value>> {
        // 检查是否有同步列
        if !self.has_sync_columns(table_name).await? {
            log::warn!("[RemoteDb] Table {} missing sync columns, adding them...", table_name);
            self.add_sync_columns(table_name).await?;
        }
        
        let sql = format!(
            r#"SELECT row_to_json(t) FROM "{}" t WHERE "_hlc" > $1 OR "_hlc" IS NULL ORDER BY "_hlc" NULLS FIRST"#,
            table_name
        );
        
        let rows = sqlx::query(&sql)
            .bind(since_hlc)
            .fetch_all(&self.pool)
            .await?;
        
        let mut results = Vec::new();
        for row in rows {
            let json: serde_json::Value = row.get(0);
            results.push(json);
        }
        
        Ok(results)
    }

    /// 获取表中所有数据（用于首次同步无 _hlc 的表）
    pub async fn fetch_all_rows(&self, table_name: &str) -> Result<Vec<serde_json::Value>> {
        let sql = format!(r#"SELECT row_to_json(t) FROM "{}" t"#, table_name);
        
        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await?;
        
        let mut results = Vec::new();
        for row in rows {
            let json: serde_json::Value = row.get(0);
            results.push(json);
        }
        
        Ok(results)
    }

    /// 推送变更到远程（包含所有数据列）
    /// 
    /// 注意：`_synced` 是本地字段，不会同步到远程
    pub async fn push_change(&self, table_name: &str, payload: &serde_json::Value) -> Result<()> {
        let obj = match payload.as_object() {
            Some(o) => o,
            None => return Err(anyhow::anyhow!("Payload must be a JSON object")),
        };

        // 本地专用字段，不同步到远程
        const LOCAL_ONLY_FIELDS: &[&str] = &["_synced"];

        // 提取所有列名和值（排除本地专用字段）
        let mut columns: Vec<String> = Vec::new();
        let mut column_names: Vec<String> = Vec::new();
        let mut placeholders: Vec<String> = Vec::new();
        let mut values: Vec<serde_json::Value> = Vec::new();
        let mut update_sets: Vec<String> = Vec::new();
        let mut idx = 0;

        for (key, value) in obj.iter() {
            // 跳过本地专用字段
            if LOCAL_ONLY_FIELDS.contains(&key.as_str()) {
                continue;
            }
            
            columns.push(format!(r#""{}""#, key));
            column_names.push(key.clone());
            placeholders.push(format!("${}", idx + 1));
            values.push(value.clone());
            idx += 1;
            
            // 更新子句（排除 id）
            if key != "id" {
                update_sets.push(format!(r#""{}" = EXCLUDED."{}""#, key, key));
            }
        }

        // 构建 UPSERT SQL
        let sql = format!(
            r#"
            INSERT INTO "{}" ({})
            VALUES ({})
            ON CONFLICT (id) DO UPDATE SET
                {}
            WHERE "{}"."_hlc" < EXCLUDED."_hlc" OR "{}"."_hlc" IS NULL
            "#,
            table_name,
            columns.join(", "),
            placeholders.join(", "),
            update_sets.join(", "),
            table_name,
            table_name
        );

        // 构建查询
        let mut query = sqlx::query(&sql);
        for (i, value) in values.iter().enumerate() {
            let col_name = column_names.get(i).map(|s| s.as_str()).unwrap_or("");
            query = bind_json_value(query, value, col_name);
        }

        query.execute(&self.pool).await?;
        Ok(())
    }

    /// 带超时的推送变更
    pub async fn push_change_with_timeout(
        &self,
        table_name: &str,
        payload: &serde_json::Value,
        timeout: Duration,
    ) -> Result<()> {
        tokio::time::timeout(timeout, self.push_change(table_name, payload))
            .await
            .map_err(|_| anyhow::anyhow!("Push change timed out"))?
    }

    pub async fn health_check(&self) -> Result<bool> {
        self.health_check_with_timeout(Duration::from_secs(5)).await
    }

    /// 带超时的健康检查
    pub async fn health_check_with_timeout(&self, timeout: Duration) -> Result<bool> {
        match tokio::time::timeout(timeout, sqlx::query("SELECT 1").fetch_one(&self.pool)).await {
            Ok(Ok(_)) => Ok(true),
            Ok(Err(e)) => {
                log::warn!("[RemoteDb] Health check failed: {}", e);
                Ok(false)
            }
            Err(_) => {
                log::warn!("[RemoteDb] Health check timed out");
                Ok(false)
            }
        }
    }

    /// 检查远程表是否存在
    pub async fn table_exists(&self, table_name: &str) -> Result<bool> {
        let row = sqlx::query(
            "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name = $1)"
        )
        .bind(table_name)
        .fetch_one(&self.pool)
        .await?;
        
        let exists: bool = row.get(0);
        Ok(exists)
    }

    /// 获取远程表结构
    pub async fn get_table_schema(&self, table_name: &str) -> Result<Vec<ColumnDef>> {
        let rows = sqlx::query(
            r#"
            SELECT column_name, data_type, is_nullable, column_default
            FROM information_schema.columns
            WHERE table_name = $1
            ORDER BY ordinal_position
            "#
        )
        .bind(table_name)
        .fetch_all(&self.pool)
        .await?;
        
        let mut columns = Vec::new();
        for row in rows {
            columns.push(ColumnDef {
                name: row.get("column_name"),
                data_type: row.get("data_type"),
                nullable: row.get::<String, _>("is_nullable") == "YES",
                default: row.get("column_default"),
            });
        }
        
        Ok(columns)
    }

    /// 在远程创建表（根据列定义）
    pub async fn create_table(&self, table_name: &str, columns: &[ColumnDef]) -> Result<()> {
        let mut col_defs = Vec::new();
        
        for col in columns {
            let nullable = if col.nullable { "" } else { " NOT NULL" };
            let default = col.default.as_ref()
                .map(|d| format!(" DEFAULT {}", d))
                .unwrap_or_default();
            
            let pg_type = sqlite_type_to_pg(&col.data_type);
            
            if col.name == "id" {
                col_defs.push(format!("id UUID PRIMARY KEY DEFAULT gen_random_uuid()"));
            } else {
                col_defs.push(format!("{} {}{}{}", col.name, pg_type, nullable, default));
            }
        }
        
        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {} ({})",
            table_name,
            col_defs.join(", ")
        );
        
        sqlx::query(&sql).execute(&self.pool).await?;
        
        log::info!("[RemoteDb] Created table: {}", table_name);
        Ok(())
    }

    /// 获取所有表名（默认 public schema）
    pub async fn list_tables(&self) -> Result<Vec<String>> {
        self.list_tables_in_schema("public").await
    }

    /// 获取指定 schema 的所有表名
    pub async fn list_tables_in_schema(&self, schema_name: &str) -> Result<Vec<String>> {
        let rows = sqlx::query(
            r#"
            SELECT table_name 
            FROM information_schema.tables 
            WHERE table_schema = $1 
              AND table_type = 'BASE TABLE'
              AND table_name NOT LIKE '\_%'
            ORDER BY table_name
            "#
        )
        .bind(schema_name)
        .fetch_all(&self.pool)
        .await?;
        
        let tables = rows.iter().map(|r| r.get("table_name")).collect();
        Ok(tables)
    }

    /// 获取指定 schema 的表结构
    pub async fn get_table_schema_in_schema(&self, schema_name: &str, table_name: &str) -> Result<Vec<ColumnDef>> {
        let rows = sqlx::query(
            r#"
            SELECT column_name, data_type, is_nullable, column_default
            FROM information_schema.columns
            WHERE table_schema = $1 AND table_name = $2
            ORDER BY ordinal_position
            "#
        )
        .bind(schema_name)
        .bind(table_name)
        .fetch_all(&self.pool)
        .await?;
        
        let mut columns = Vec::new();
        for row in rows {
            columns.push(ColumnDef {
                name: row.get("column_name"),
                data_type: row.get("data_type"),
                nullable: row.get::<String, _>("is_nullable") == "YES",
                default: row.get("column_default"),
            });
        }
        
        Ok(columns)
    }

    /// 在指定 schema 创建表
    pub async fn create_table_in_schema(&self, schema_name: &str, table_name: &str, columns: &[ColumnDef]) -> Result<()> {
        // 确保 schema 存在
        sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS {}", schema_name))
            .execute(&self.pool)
            .await?;

        let mut col_defs = Vec::new();
        
        for col in columns {
            let nullable = if col.nullable { "" } else { " NOT NULL" };
            let default = col.default.as_ref()
                .map(|d| format!(" DEFAULT {}", d))
                .unwrap_or_default();
            
            let pg_type = sqlite_type_to_pg(&col.data_type);
            
            if col.name == "id" {
                col_defs.push("id UUID PRIMARY KEY DEFAULT gen_random_uuid()".to_string());
            } else {
                col_defs.push(format!("{} {}{}{}", col.name, pg_type, nullable, default));
            }
        }
        
        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {}.{} ({})",
            schema_name,
            table_name,
            col_defs.join(", ")
        );
        
        sqlx::query(&sql).execute(&self.pool).await?;
        
        log::info!("[RemoteDb] Created table: {}.{}", schema_name, table_name);
        Ok(())
    }
}

/// 列定义
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default: Option<String>,
}

/// 将 JSON 值绑定到 sqlx 查询
/// 
/// 特殊处理：
/// - `id` 字段：字符串转换为 UUID
/// - `_deleted` 字段：0/1 转换为 boolean
fn bind_json_value<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    value: &'q serde_json::Value,
    column_name: &str,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    // 特殊处理 id 字段（SQLite 用 TEXT，PostgreSQL 用 UUID）
    if column_name == "id" {
        if let serde_json::Value::String(s) = value {
            if let Ok(uuid) = uuid::Uuid::parse_str(s) {
                return query.bind(uuid);
            }
        }
        // 如果不是有效 UUID，绑定为 NULL
        return query.bind(None::<uuid::Uuid>);
    }
    
    // 特殊处理 _deleted 字段（SQLite 用 0/1，PostgreSQL 用 boolean）
    if column_name == "_deleted" {
        let bool_val = match value {
            serde_json::Value::Bool(b) => *b,
            serde_json::Value::Number(n) => n.as_i64().unwrap_or(0) != 0,
            _ => false,
        };
        return query.bind(bool_val);
    }
    
    match value {
        serde_json::Value::Null => query.bind(None::<String>),
        serde_json::Value::Bool(b) => query.bind(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                query.bind(i)
            } else if let Some(f) = n.as_f64() {
                query.bind(f)
            } else {
                query.bind(n.to_string())
            }
        }
        serde_json::Value::String(s) => query.bind(s.as_str()),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            // JSON 数组和对象存储为 JSONB
            query.bind(value.clone())
        }
    }
}

/// SQLite 类型转 PostgreSQL 类型
fn sqlite_type_to_pg(sqlite_type: &str) -> &'static str {
    match sqlite_type.to_uppercase().as_str() {
        "TEXT" => "TEXT",
        "INTEGER" => "BIGINT",
        "REAL" => "DOUBLE PRECISION",
        "BLOB" => "BYTEA",
        "BOOLEAN" => "BOOLEAN",
        // PostgreSQL 类型直通
        "UUID" => "UUID",
        "TIMESTAMP" | "TIMESTAMPTZ" => "TIMESTAMPTZ",
        "JSONB" | "JSON" => "JSONB",
        "GEOMETRY" => "GEOMETRY",
        _ => "TEXT",
    }
}

/// PostgreSQL 类型转 SQLite 类型
pub fn pg_type_to_sqlite(pg_type: &str) -> &'static str {
    match pg_type.to_lowercase().as_str() {
        "text" | "character varying" | "varchar" | "char" | "uuid" => "TEXT",
        "integer" | "bigint" | "smallint" | "serial" | "bigserial" => "INTEGER",
        "real" | "double precision" | "numeric" | "decimal" => "REAL",
        "bytea" => "BLOB",
        "boolean" => "INTEGER",
        "timestamp without time zone" | "timestamp with time zone" | "date" | "time" => "TEXT",
        "json" | "jsonb" => "TEXT",
        "geometry" | "geography" => "BLOB",
        _ => "TEXT",
    }
}
