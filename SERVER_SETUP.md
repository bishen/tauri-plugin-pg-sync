# 服务端 PostgreSQL 配置指南

本文档说明如何配置 PostgreSQL 服务端以支持实时数据同步。

## 概述

使用 PostgreSQL 内置的 `LISTEN/NOTIFY` 机制实现实时变更通知，无需额外的 WebSocket 服务。

```
┌─────────────┐     INSERT/UPDATE/DELETE     ┌─────────────┐
│  客户端 A   │ ─────────────────────────────▶│             │
└─────────────┘                               │             │
                                              │ PostgreSQL  │
┌─────────────┐     LISTEN 'data_changes'    │             │
│  客户端 B   │ ◀─────────────────────────────│             │
└─────────────┘     pg_notify()              └─────────────┘
```

## 第一步：创建通知函数

在 PostgreSQL 中执行以下 SQL：

```sql
-- 创建通知函数
CREATE OR REPLACE FUNCTION notify_table_change()
RETURNS trigger AS $$
DECLARE
    payload JSON;
    record_id TEXT;
BEGIN
    -- 获取记录 ID
    IF TG_OP = 'DELETE' THEN
        record_id := OLD.id::TEXT;
    ELSE
        record_id := NEW.id::TEXT;
    END IF;
    
    -- 构建通知载荷
    payload := json_build_object(
        'table', TG_TABLE_SCHEMA || '.' || TG_TABLE_NAME,
        'action', TG_OP,
        'id', record_id,
        'timestamp', CURRENT_TIMESTAMP
    );
    
    -- 发送通知
    PERFORM pg_notify('data_changes', payload::TEXT);
    
    -- 返回结果
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    ELSE
        RETURN NEW;
    END IF;
END;
$$ LANGUAGE plpgsql;
```

## 第二步：为同步表添加触发器

为每个需要同步的表添加触发器：

```sql
-- 为 users 表添加触发器
CREATE TRIGGER users_sync_trigger
AFTER INSERT OR UPDATE OR DELETE ON users
FOR EACH ROW EXECUTE FUNCTION notify_table_change();

-- 为 orders 表添加触发器
CREATE TRIGGER orders_sync_trigger
AFTER INSERT OR UPDATE OR DELETE ON orders
FOR EACH ROW EXECUTE FUNCTION notify_table_change();

-- 通用模板（替换 TABLE_NAME）
CREATE TRIGGER TABLE_NAME_sync_trigger
AFTER INSERT OR UPDATE OR DELETE ON TABLE_NAME
FOR EACH ROW EXECUTE FUNCTION notify_table_change();
```

## 第三步：批量添加触发器脚本

使用此脚本为所有带前缀的表添加触发器：

```sql
-- 为所有 app_ 前缀的表添加触发器
DO $$
DECLARE
    tbl RECORD;
BEGIN
    FOR tbl IN 
        SELECT table_name 
        FROM information_schema.tables 
        WHERE table_schema = 'public' 
          AND table_type = 'BASE TABLE'
          AND table_name LIKE 'app_%'  -- 修改为你的表前缀
    LOOP
        EXECUTE format(
            'DROP TRIGGER IF EXISTS %I_sync_trigger ON %I',
            tbl.table_name, tbl.table_name
        );
        EXECUTE format(
            'CREATE TRIGGER %I_sync_trigger 
             AFTER INSERT OR UPDATE OR DELETE ON %I
             FOR EACH ROW EXECUTE FUNCTION notify_table_change()',
            tbl.table_name, tbl.table_name
        );
        RAISE NOTICE 'Created trigger for table: %', tbl.table_name;
    END LOOP;
END $$;
```

## 第四步：验证配置

### 测试通知是否工作

打开两个 PostgreSQL 终端：

**终端 1（监听）：**
```sql
LISTEN data_changes;
```

**终端 2（触发）：**
```sql
INSERT INTO users (id, name, _hlc, _node_id) 
VALUES (gen_random_uuid(), 'Test', '2024-01-01T00:00:00Z', 'test-node');
```

**终端 1 应收到通知：**
```
Asynchronous notification "data_changes" with payload 
"{"table":"public.users","action":"INSERT","id":"xxx-xxx","timestamp":"2024-01-01T12:00:00"}" 
received from server process with PID 12345.
```

## 第五步：确保同步元字段存在

每个同步表必须包含以下字段：

```sql
-- 检查并添加同步元字段
ALTER TABLE users ADD COLUMN IF NOT EXISTS _hlc TEXT NOT NULL DEFAULT '';
ALTER TABLE users ADD COLUMN IF NOT EXISTS _node_id TEXT NOT NULL DEFAULT '';
ALTER TABLE users ADD COLUMN IF NOT EXISTS _version INTEGER DEFAULT 1;
ALTER TABLE users ADD COLUMN IF NOT EXISTS _deleted BOOLEAN DEFAULT FALSE;

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_users_hlc ON users(_hlc);
CREATE INDEX IF NOT EXISTS idx_users_deleted ON users(_deleted);
```

## 完整建表示例

```sql
-- 创建用户表（完整示例）
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    email TEXT,
    age INTEGER,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    
    -- 同步元字段（必需）
    _hlc TEXT NOT NULL,
    _node_id TEXT NOT NULL,
    _version INTEGER DEFAULT 1,
    _deleted BOOLEAN DEFAULT FALSE
);

-- 创建索引
CREATE INDEX idx_users_hlc ON users(_hlc);
CREATE INDEX idx_users_deleted ON users(_deleted);

-- 添加同步触发器
CREATE TRIGGER users_sync_trigger
AFTER INSERT OR UPDATE OR DELETE ON users
FOR EACH ROW EXECUTE FUNCTION notify_table_change();
```

## PostgreSQL 配置优化（可选）

### 增加通知队列大小

编辑 `postgresql.conf`：

```ini
# 增加异步通知队列大小（默认 8MB）
notify_queue_pages = 1000
```

### 连接池配置

如果使用连接池（如 PgBouncer），需要配置为 session 模式以支持 LISTEN：

```ini
# pgbouncer.ini
pool_mode = session
```

## 使用 Schema 隔离（可选）

如果多个应用共用同一个数据库，建议使用 PostgreSQL schema 隔离：

```sql
-- 创建应用专用 schema
CREATE SCHEMA IF NOT EXISTS mobile_app;

-- 在指定 schema 创建表
CREATE TABLE mobile_app.users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    -- ...其他字段
    _hlc TEXT NOT NULL,
    _node_id TEXT NOT NULL,
    _version INTEGER DEFAULT 1,
    _deleted BOOLEAN DEFAULT FALSE
);

-- 为指定 schema 的表添加触发器
CREATE TRIGGER users_sync_trigger
AFTER INSERT OR UPDATE OR DELETE ON mobile_app.users
FOR EACH ROW EXECUTE FUNCTION notify_table_change();
```

## 权限配置

确保应用数据库用户有以下权限：

```sql
-- 创建应用用户
CREATE USER app_user WITH PASSWORD 'your_password';

-- 授权
GRANT CONNECT ON DATABASE your_db TO app_user;
GRANT USAGE ON SCHEMA public TO app_user;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO app_user;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO app_user;

-- 如果使用自定义 schema
GRANT USAGE ON SCHEMA mobile_app TO app_user;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA mobile_app TO app_user;
```

## 云数据库支持

### Supabase

Supabase 基于 PostgreSQL，完全支持 LISTEN/NOTIFY：

```javascript
// 连接字符串
const url = 'postgres://postgres:[PASSWORD]@db.[PROJECT_ID].supabase.co:5432/postgres';
```

### AWS RDS

AWS RDS PostgreSQL 支持 LISTEN/NOTIFY，无需额外配置。

### Azure Database for PostgreSQL

Azure PostgreSQL 支持 LISTEN/NOTIFY，但需要使用 SSL 连接。

### 阿里云 RDS PostgreSQL

支持 LISTEN/NOTIFY，确保安全组开放 5432 端口。

## 监控和调试

### 查看当前监听

```sql
SELECT * FROM pg_stat_activity WHERE state = 'idle' AND query LIKE '%LISTEN%';
```

### 查看通知统计

```sql
SELECT * FROM pg_stat_database WHERE datname = 'your_db';
```

### 手动发送测试通知

```sql
SELECT pg_notify('data_changes', '{"table":"test","action":"TEST","id":"123"}');
```

## 故障排查

### 客户端收不到通知

1. 检查触发器是否创建成功：
   ```sql
   SELECT * FROM information_schema.triggers WHERE trigger_name LIKE '%sync%';
   ```

2. 检查连接是否保持（LISTEN 需要持久连接）

3. 检查防火墙/安全组是否允许长连接

### 通知延迟

1. 检查数据库负载
2. 考虑增加 `notify_queue_pages` 配置
3. 检查网络延迟

## 下一步

服务端配置完成后，客户端会自动监听变更通知并触发同步。

详见 [API.md](./API.md) 中的实时同步章节。
