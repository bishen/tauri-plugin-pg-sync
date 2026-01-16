# tauri-plugin-pg-sync

[![Crates.io](https://img.shields.io/crates/v/tauri-plugin-pg-sync.svg)](https://crates.io/crates/tauri-plugin-pg-sync)
[![npm](https://img.shields.io/npm/v/@bishen/tauri-plugin-pg-sync.svg)](https://www.npmjs.com/package/@bishen/tauri-plugin-pg-sync)

离线优先的 PostgreSQL 同步插件，支持全平台。

[English](./README_EN.md)

## 支持的平台

| 平台 | 架构 | 状态 |
|------|------|------|
| Windows | x86_64, aarch64 | ✅ |
| macOS | x86_64, aarch64 | ✅ |
| Linux | x86_64, aarch64 | ✅ |
| Android | arm64-v8a, armeabi-v7a, x86_64 | ✅ |
| iOS | arm64, x86_64 (模拟器) | ✅ |

## 功能特性

- **离线优先**: 本地 SQLite 数据库，支持完全离线使用
- **PostgreSQL 同步**: 自动与远程 PostgreSQL 数据库同步
- **冲突解决**: 基于 HLC (Hybrid Logical Clock) 的冲突解决
- **GIS 支持**: 可选的空间数据处理能力
- **跨平台**: 支持所有 Tauri 支持的平台

## 安装

### Rust 依赖

在 `src-tauri/Cargo.toml` 中添加:

```toml
[dependencies]
tauri-plugin-pg-sync = "0.1"
```

### JavaScript 依赖

```bash
npm install @bishen/tauri-plugin-pg-sync
# 或
pnpm add @bishen/tauri-plugin-pg-sync
# 或
yarn add @bishen/tauri-plugin-pg-sync
```

## 配置

### 1. 注册插件

在 `src-tauri/src/main.rs` 或 `src-tauri/src/lib.rs` 中:

```rust
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_pg_sync::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### 2. 配置权限

在 `src-tauri/capabilities/default.json` 中添加:

```json
{
  "permissions": [
    "pg-sync:default"
  ]
}
```

## 文档

- [API 完整参考](./API.md) - 前端 API 详细文档
- [服务端配置](./SERVER_SETUP.md) - PostgreSQL 服务端配置指南
- [类型映射](./TYPE_MAPPING.md) - SQLite ↔ PostgreSQL 类型映射

## 使用方法

### 智能初始化（推荐）

```typescript
import { smartInit, table, sync } from '@bishen/tauri-plugin-pg-sync';

// 智能初始化：自动检测本地/远程状态
const result = await smartInit({
  remoteUrl: 'postgres://user:pass@host:5432/db'
});

console.log('模式:', result.mode);
// 可能值: 'local_only' | 'offline' | 'pulled_from_remote' | 'pushed_to_remote' | 'synced'

// 定义表并操作
const users = table('users', {
  columns: [['name', 'TEXT'], ['age', 'INTEGER']]
});

const id = await users.insert({ name: '张三', age: 25 });
const all = await users.findAll();

// 手动同步
await sync.now();
```

### 基础用法

```typescript
import {
  initDatabase,
  ensureTable,
  insert,
  findAll,
  connectRemote,
  syncNow,
} from '@bishen/tauri-plugin-pg-sync';

// 1. 初始化本地数据库
const nodeId = await initDatabase();

// 2. 定义表结构
await ensureTable('users', {
  columns: [
    ['name', 'TEXT NOT NULL'],
    ['email', 'TEXT'],
    ['age', 'INTEGER'],
  ],
});

// 3. CRUD 操作（离线可用）
const id = await insert('users', {
  name: 'Alice',
  email: 'alice@example.com',
  age: 25,
});

const users = await findAll('users');

// 4. 连接远程数据库并同步
await connectRemote('postgres://user:pass@host:5432/db');
const result = await syncNow();
console.log(`Pushed: ${result.pushed}, Pulled: ${result.pulled}`);
```

### 高级查询

```typescript
import { query, findWhere, count } from '@bishen/tauri-plugin-pg-sync';

// 条件查询
const adults = await findWhere('users', { age: { $gte: 18 } });

// 高级查询
const results = await query('users', {
  where: { age: { $gte: 18 } },
  orderBy: 'name',
  orderDesc: false,
  limit: 10,
  offset: 0,
  select: ['id', 'name', 'email'],
});

// 计数
const total = await count('users');
const adultCount = await count('users', { age: { $gte: 18 } });
```

### 批量操作

```typescript
import { insertMany, updateMany, deleteMany } from '@bishen/tauri-plugin-pg-sync';

// 批量插入
const ids = await insertMany('users', [
  { name: 'Bob', email: 'bob@example.com' },
  { name: 'Charlie', email: 'charlie@example.com' },
]);

// 批量更新
await updateMany('users', [
  [ids[0], { age: 30 }],
  [ids[1], { age: 25 }],
]);

// 批量删除
await deleteMany('users', ids);
```

### 表结构同步

```typescript
import {
  pushTableSchema,
  pullTableSchema,
  listLocalTables,
  listRemoteTables,
} from '@bishen/tauri-plugin-pg-sync';

// 推送本地表结构到远程
await pushTableSchema('users');

// 从远程拉取表结构
await pullTableSchema('products');

// 列出表
const localTables = await listLocalTables();
const remoteTables = await listRemoteTables();
```

## API 参考

> 完整 API 请参考 [API.md](./API.md)

### 初始化

| 函数 | 描述 |
|------|------|
| `smartInit(options)` | 智能初始化（推荐） |
| `initDatabase()` | 初始化本地数据库（桌面端） |
| `initDatabaseMobile()` | 初始化本地数据库（移动端优化） |
| `getDbPath()` | 获取数据库文件路径 |

### 远程连接

| 函数 | 描述 |
|------|------|
| `connectRemote(url)` | 连接远程 PostgreSQL |
| `connectRemoteWithRetry(url)` | 带重试的连接（弱网优化） |
| `startAutoReconnect()` | 启动自动重连 |
| `disconnectRemote()` | 断开连接 |
| `syncNow()` | 立即同步 |
| `isOnline()` | 检查是否在线 |

### CRUD

| 函数 | 描述 |
|------|------|
| `insert(table, data)` | 插入记录 |
| `update(table, id, data)` | 更新记录 |
| `remove(table, id)` | 删除记录（软删除） |
| `findById(table, id)` | 根据 ID 查找 |
| `findAll(table, limit?, offset?)` | 查找所有 |
| `findWhere(table, conditions)` | 条件查询 |
| `query(table, options)` | 高级查询 |
| `count(table, conditions?)` | 计数 |

### 批量操作

| 函数 | 描述 |
|------|------|
| `insertMany(table, items)` | 批量插入 |
| `updateMany(table, updates)` | 批量更新 |
| `deleteMany(table, ids)` | 批量删除 |
| `clearTable(table)` | 清空表 |

## 许可证

MIT

Copyright (c) 2026 BiShen <bishen@live.com>
算金山™ (https://www.suanjinshan.com/)
