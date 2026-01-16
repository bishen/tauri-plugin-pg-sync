# sync-db.js API 文档

> 完整的前端 API 参考手册

## 目录

- [快速开始](#快速开始)
- [初始化函数](#初始化函数)
- [db - 数据库 API](#db---数据库-api)
- [sync - 同步 API](#sync---同步-api)
- [table - 数据表 API](#table---数据表-api)
- [schema - 表结构 API](#schema---表结构-api)
- [syncManager - 智能同步管理器](#syncmanager---智能同步管理器)
- [类型定义](#类型定义)

---

## 快速开始

```javascript
import { initApp, table, sync } from '@bishen/tauri-plugin-pg-sync';

// 初始化
const { nodeId, online } = await initApp({
  remoteUrl: 'postgres://user:pass@host:5432/db'
});

// 定义表
const users = table('users', {
  columns: [['name', 'TEXT'], ['age', 'INTEGER']]
});

// CRUD
const id = await users.insert({ name: '张三', age: 25 });
await users.update(id, { age: 26 });
const user = await users.findById(id);
await users.delete(id);

// 手动同步
await sync.now();
```

---

## 初始化函数

### `initApp(options)`

桌面端应用初始化。

```javascript
const { nodeId, online } = await initApp({
  remoteUrl: 'postgres://user:pass@host:5432/db',
  autoReconnect: true  // 可选，启动自动重连
});
```

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `options.remoteUrl` | `string` | 否 | PostgreSQL 连接字符串 |
| `options.autoReconnect` | `boolean` | 否 | 是否启动自动重连 |

**返回值**: `Promise<{ nodeId: string, online: boolean }>`

---

### `initAppMobile(options)`

移动端应用初始化（弱网优化）。

```javascript
const { nodeId, online } = await initAppMobile({
  remoteUrl: 'postgres://user:pass@host:5432/db'
});
```

**特点**:
- 使用更短的连接超时（5s vs 10s）
- 更多重试次数（8次 vs 5次）
- 自动启用弱网重试连接
- 自动启动后台重连任务

---

### `smartInit(options)`

智能初始化：自动检测本地/远程状态并处理。

```javascript
const result = await smartInit({
  remoteUrl: import.meta.env.VITE_PG_URL
});

// result.mode 可能值:
// - 'local_only'        - 纯本地模式
// - 'offline'           - 无法连接远程
// - 'pulled_from_remote' - 从远程拉取了表结构
// - 'pushed_to_remote'  - 推送了本地表结构
// - 'synced'            - 正常同步
```

| 场景 | 行为 |
|------|------|
| 本地空 + 远程有表 | 自动拉取远程表结构和数据 |
| 本地有表 + 远程空 | 自动推送本地表结构 |
| 都有表 | 正常数据同步 |
| 无法连接远程 | 离线模式，使用本地数据 |

---

### `initFromRemote(options)`

从远程初始化（首次启动场景）。

```javascript
const result = await initFromRemote({
  remoteUrl: 'postgres://user:pass@host:5432/db',
  pullData: true  // 同时拉取数据
});

console.log('已同步表:', result.tables);
console.log('拉取数据:', result.dataCount, '条');
```

---

### `autoSync(intervalMs)`

自动同步（定时轮询）。

```javascript
// 每 60 秒同步一次
const stopSync = autoSync(60000);

// 停止自动同步
stopSync();
```

---

## db - 数据库 API

### `db.init()`

初始化本地数据库。

```javascript
const nodeId = await db.init();
console.log('Node ID:', nodeId);
```

---

### `db.initMobile()`

初始化本地数据库（移动端优化配置）。

```javascript
const nodeId = await db.initMobile();
```

---

### `db.getPath()`

获取数据库文件路径。

```javascript
const path = await db.getPath();
// Windows: C:\Users\xxx\AppData\Roaming\app\data.db
// Android: /data/data/com.app/files/data.db
```

---

### `db.connectRemote(url)`

连接远程 PostgreSQL 数据库。

```javascript
await db.connectRemote('postgres://user:pass@host:5432/dbname');
```

---

### `db.connectRemoteWithRetry(url)`

带重试的连接（适合弱网环境）。

```javascript
// 使用指数退避重试策略
await db.connectRemoteWithRetry('postgres://user:pass@host:5432/dbname');
```

**重试策略**:
- 初始延迟: 0.5s（移动端）/ 1s（桌面端）
- 最大延迟: 30s（移动端）/ 60s（桌面端）
- 最大重试: 8次（移动端）/ 5次（桌面端）
- 带随机抖动，避免惊群效应

---

### `db.startAutoReconnect()`

启动自动重连（后台任务）。

```javascript
await db.startAutoReconnect();
// 后台定期健康检查，断线时自动重连
```

---

### `db.disconnectRemote()`

断开远程数据库连接。

```javascript
await db.disconnectRemote();
```

---

### `db.purgeAll(options?)`

清理所有表的已删除数据（释放存储空间）。

```javascript
// 清理 30 天前的所有已删除数据
const purged = await db.purgeAll({ daysOld: 30 });
console.log(`已清理 ${purged} 条记录`);

// 清理所有已删除数据（慎用）
await db.purgeAll();
```

**注意**: 此操作会同时清理已同步的 changelog 并执行 VACUUM 回收磁盘空间。

---

## sync - 同步 API

### `sync.isOnline()`

检查是否在线。

```javascript
const online = await sync.isOnline();
if (online) {
  console.log('已连接到远程数据库');
}
```

---

### `sync.now()`

执行同步（推送本地更改 + 拉取远程更改）。

```javascript
const result = await sync.now();
console.log(`推送: ${result.pushed}, 拉取: ${result.pulled}, 冲突: ${result.conflicts}`);

if (result.errors.length > 0) {
  console.error('同步错误:', result.errors);
}
```

**返回值**: `SyncResult`

```typescript
interface SyncResult {
  pushed: number;    // 推送的记录数
  pulled: number;    // 拉取的记录数
  conflicts: number; // 冲突数
  errors: string[];  // 错误信息列表
}
```

---

## table - 数据表 API

### `table(tableName, schema?)`

创建数据表操作对象。

```javascript
const users = table('users', {
  columns: [
    ['name', 'TEXT'],
    ['age', 'INTEGER'],
    ['email', 'TEXT'],
    ['score', 'REAL'],
    ['data', 'BLOB']
  ]
});
```

**支持的列类型**:

| 类型 | SQLite | PostgreSQL |
|------|--------|------------|
| `TEXT` | TEXT | TEXT / VARCHAR |
| `INTEGER` | INTEGER | INTEGER / BIGINT |
| `REAL` | REAL | REAL / DOUBLE |
| `BLOB` | BLOB | BYTEA |

---

### `table.insert(data)`

插入数据（自动生成 ID）。

```javascript
const id = await users.insert({
  name: '张三',
  age: 25,
  email: 'zhang@example.com'
});
console.log('新记录 ID:', id);
```

---

### `table.update(id, data)`

更新数据。

```javascript
const success = await users.update(id, {
  age: 26,
  email: 'new@example.com'
});
```

---

### `table.delete(id)`

删除数据（软删除）。

```javascript
const success = await users.delete(id);
// 数据不会物理删除，仅标记 _deleted = 1
```

---

### `table.findById(id)`

根据 ID 查询。

```javascript
const user = await users.findById(id);
if (user) {
  console.log(user.name, user.age);
}
```

---

### `table.findAll(options?)`

查询所有数据。

```javascript
// 全部数据
const all = await users.findAll();

// 分页
const page = await users.findAll({ limit: 10, offset: 0 });
```

---

### `table.findWhere(conditions)`

条件查询（精确匹配）。

```javascript
const adults = await users.findWhere({ age: 25 });
const active = await users.findWhere({ status: 'active', role: 'admin' });
```

---

### `table.query(options)`

高级查询（支持模糊、范围、排序、分页）。

```javascript
const results = await users.query({
  // 精确匹配
  whereEq: { status: 'active' },
  
  // 模糊查询 (LIKE '%keyword%')
  whereLike: { name: '张' },
  
  // 范围查询
  whereGte: { age: 18 },     // >= 18
  whereLte: { age: 60 },     // <= 60
  
  // 排序
  orderBy: 'created_at',
  orderDesc: true,           // 降序
  
  // 分页
  limit: 10,
  offset: 0
});
```

---

### `table.count(conditions?)`

统计数量。

```javascript
const total = await users.count();
const activeCount = await users.count({ status: 'active' });
```

---

### `table.insertMany(items)`

批量插入（高性能）。

```javascript
const ids = await users.insertMany([
  { name: '张三', age: 25 },
  { name: '李四', age: 30 },
  { name: '王五', age: 28 }
]);
console.log('插入了', ids.length, '条记录');
```

---

### `table.updateMany(updates)`

批量更新。

```javascript
const updated = await users.updateMany([
  ['id-1', { age: 26 }],
  ['id-2', { age: 31 }],
  ['id-3', { name: '王五五' }]
]);
console.log('更新了', updated, '条记录');
```

---

### `table.deleteMany(ids)`

批量删除。

```javascript
const deleted = await users.deleteMany(['id-1', 'id-2', 'id-3']);
console.log('删除了', deleted, '条记录');
```

---

### `table.clear()`

清空表（软删除所有数据）。

```javascript
const cleared = await users.clear();
console.log('清空了', cleared, '条记录');
```

---

### `table.purge(options?)`

物理删除已标记删除的数据（清理存储空间）。

```javascript
// 删除所有已标记删除的数据
const purged = await users.purge();

// 只删除 30 天前的已删除数据（保留近期可恢复）
const purged = await users.purge({ daysOld: 30 });
```

---

### `table.getDeletedStats()`

获取已删除数据的统计信息。

```javascript
const stats = await users.getDeletedStats();
// { table: 'users', deleted_count: 150, oldest_hlc: '1234567890:0:node-id' }

if (stats.deleted_count > 1000) {
  console.log('建议清理已删除数据');
  await users.purge({ daysOld: 30 });
}
```

---

## schema - 表结构 API

### `schema.setTablePrefix(prefix)`

设置表前缀。

```javascript
schema.setTablePrefix('app_');
// 之后 listLocalTables() 只返回以 'app_' 开头的表
```

---

### `schema.getLocalSchema(tableName)`

获取本地表结构。

```javascript
const columns = await schema.getLocalSchema('users');
// [{ name: 'id', data_type: 'TEXT', nullable: false, default: null }, ...]
```

---

### `schema.listLocalTables(prefix?)`

列出本地所有表。

```javascript
const tables = await schema.listLocalTables();
// ['users', 'orders', 'products']
```

---

### `schema.listRemoteTables(options?)`

列出远程所有表。

```javascript
const tables = await schema.listRemoteTables({
  prefix: 'app_',
  schema: 'public'
});
```

---

### `schema.pushTable(tableName)`

推表：将本地表结构推送到服务端创建。

```javascript
await schema.pushTable('users');
// PostgreSQL 中会创建对应的表
```

---

### `schema.pullTable(tableName)`

拉表：从服务端拉取表结构到本地创建。

```javascript
await schema.pullTable('orders');
// SQLite 中会创建对应的表
```

---

### `schema.pushAllTables()`

推送所有本地表到服务端。

```javascript
const result = await schema.pushAllTables();
console.log('成功:', result.success);
console.log('失败:', result.failed);
```

---

### `schema.pullAllTables()`

从服务端拉取所有表到本地。

```javascript
const result = await schema.pullAllTables();
console.log('成功:', result.success);
console.log('失败:', result.failed);
```

---

## syncManager - 智能同步管理器

结合轮询和实时通知，自动处理断线重连和弱网。

### `syncManager.start(options)`

启动智能同步。

```javascript
await syncManager.start({
  pollInterval: 30000,      // 轮询间隔（毫秒）
  enableRealtime: true,     // 是否启用实时通知
  
  // 回调函数
  onSync: (result) => {
    console.log('同步完成:', result);
  },
  onError: (error) => {
    console.error('同步错误:', error);
  },
  onStateChange: (state) => {
    // state.mode: 'realtime+polling' | 'polling' | 'offline' | 'weak_network'
    console.log('状态变化:', state);
  }
});
```

---

### `syncManager.stop()`

停止智能同步。

```javascript
syncManager.stop();
```

---

### `syncManager.getState()`

获取当前状态。

```javascript
const state = syncManager.getState();
// { polling: true, realtime: true, reconnectAttempts: 0 }
```

---

## 类型定义

### QueryOptions

```typescript
interface QueryOptions {
  whereEq?: Record<string, any>;    // 精确匹配
  whereLike?: Record<string, any>;  // 模糊匹配
  whereGte?: Record<string, any>;   // 大于等于
  whereLte?: Record<string, any>;   // 小于等于
  orderBy?: string;                 // 排序字段
  orderDesc?: boolean;              // 是否降序
  limit?: number;                   // 限制数量
  offset?: number;                  // 偏移量
}
```

### SyncResult

```typescript
interface SyncResult {
  pushed: number;     // 推送的记录数
  pulled: number;     // 拉取的记录数
  conflicts: number;  // 冲突数
  errors: string[];   // 错误信息列表
}
```

### ColumnDef

```typescript
interface ColumnDef {
  name: string;
  data_type: string;
  nullable: boolean;
  default: string | null;
}
```

---

## 同步元字段

每条记录自动包含以下元字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | TEXT/UUID | 主键，自动生成 |
| `_hlc` | TEXT | Hybrid Logical Clock 时间戳 |
| `_node_id` | TEXT | 节点 ID |
| `_version` | INTEGER | 版本号，每次更新自增 |
| `_deleted` | INTEGER | 软删除标记 (0/1) |
| `_synced` | INTEGER | 同步状态 (0/1)，仅本地 |

---

## 错误处理

```javascript
try {
  await db.connectRemote(url);
} catch (error) {
  if (error.includes('connection refused')) {
    console.error('无法连接到数据库服务器');
  } else if (error.includes('authentication failed')) {
    console.error('数据库认证失败');
  } else {
    console.error('未知错误:', error);
  }
}
```

---

## 智能同步管理器 (syncManager)

`syncManager` 提供自动化的同步管理，包括轮询、实时监听和状态跟踪。

### 基本用法

```javascript
import { syncManager, smartInit } from '@bishen/tauri-plugin-pg-sync';

// 初始化
await smartInit({ remoteUrl: 'postgres://...' });

// 启动同步管理器
await syncManager.start({
  pollInterval: 30000,      // 轮询间隔（毫秒）
  enableRealtime: true,     // 启用实时监听
  onSync: (result) => {
    console.log(`同步完成: 推送 ${result.pushed}, 拉取 ${result.pulled}`);
    refreshUI();
  },
  onStateChange: ({ mode, previous }) => {
    console.log(`状态变化: ${previous} -> ${mode}`);
  },
  onError: (error) => {
    console.error('同步错误:', error);
  }
});

// 停止同步管理器
syncManager.stop();
```

### SyncManagerOptions

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `pollInterval` | `number` | `30000` | 轮询间隔（毫秒） |
| `enableRealtime` | `boolean` | `true` | 是否监听服务端实时通知 |
| `onSync` | `(result: SyncResult) => void` | - | 同步完成回调 |
| `onStateChange` | `(state) => void` | - | 状态变化回调 |
| `onError` | `(error: Error) => void` | - | 错误回调 |

### SyncState 状态

| 状态 | 说明 |
|------|------|
| `offline` | 离线状态 |
| `online` | 在线，空闲 |
| `syncing` | 正在同步 |
| `error` | 同步出错 |

### 方法

| 方法 | 说明 |
|------|------|
| `start(options)` | 启动同步管理器 |
| `stop()` | 停止同步管理器 |
| `sync()` | 手动触发一次同步 |
| `getState()` | 获取当前状态 |
| `isRunning()` | 是否正在运行 |

### sync 命名空间

```javascript
import { sync } from '@bishen/tauri-plugin-pg-sync';

await sync.now();              // 立即同步
await sync.isOnline();         // 检查在线状态
await sync.manager.start();    // 启动管理器
sync.manager.stop();           // 停止管理器
```

---

## 实时通知与 UI 更新

当服务端数据变化时，客户端会收到通知并自动同步。以下是更新 UI 的方法：

### 方式一：使用 syncManager（推荐）

```javascript
import { syncManager, table, smartInit } from '@bishen/tauri-plugin-pg-sync';

const users = table('users');
let userList = [];

// 初始化
await smartInit({ remoteUrl: 'postgres://...' });
userList = await users.findAll();

// 启动实时同步
await syncManager.start({
  pollInterval: 30000,
  enableRealtime: true,
  
  // 同步完成回调 - 在这里更新 UI
  onSync: async (result) => {
    if (result.pulled > 0) {
      // 有新数据从服务端拉取，刷新 UI
      userList = await users.findAll();
      renderUI(userList);
    }
  },
  
  onStateChange: (state) => {
    showSyncStatus(state.mode);
  }
});
```

### 方式二：直接监听 Tauri 事件

```javascript
import { listen } from '@tauri-apps/api/event';
import { sync, table } from '@bishen/tauri-plugin-pg-sync';

const users = table('users');

// 监听服务端推送的数据变化事件
await listen('sync:data_changed', async (event) => {
  console.log('收到数据变化通知:', event.payload);
  
  const result = await sync.now();
  if (result.pulled > 0) {
    const data = await users.findAll();
    renderUI(data);
  }
});
```

### 可监听的事件列表

| 事件名 | 触发时机 | Payload 结构 |
|--------|----------|--------------|
| `sync:data_changed` | 服务端数据变化（PostgreSQL NOTIFY） | `{ table, action, id, timestamp }` |
| `sync:connected` | 成功连接远程数据库 | `{ url }` |
| `sync:disconnected` | 断开远程连接 | `{ reason }` |
| `sync:error` | 同步过程发生错误 | `{ message, code }` |
| `sync:state_changed` | 网络状态变化 | `{ state, previous }` |

### 事件 Payload 详解

#### `sync:data_changed`

当服务端数据库有 INSERT/UPDATE/DELETE 操作时触发：

```typescript
interface DataChangedPayload {
  table: string;      // 表名，如 "public.users"
  action: string;     // 操作类型: "INSERT" | "UPDATE" | "DELETE"
  id: string;         // 记录 ID
  timestamp: string;  // 服务端时间戳
}
```

#### 监听多个事件示例

```javascript
import { listen } from '@tauri-apps/api/event';

// 数据变化
const unlisten1 = await listen('sync:data_changed', (e) => {
  console.log(`表 ${e.payload.table} 有 ${e.payload.action} 操作`);
  refreshData();
});

// 连接状态
const unlisten2 = await listen('sync:connected', () => {
  showToast('已连接到服务器');
});

const unlisten3 = await listen('sync:disconnected', (e) => {
  showToast(`连接断开: ${e.payload.reason}`);
});

// 错误处理
const unlisten4 = await listen('sync:error', (e) => {
  console.error('同步错误:', e.payload.message);
});

// 组件卸载时取消监听
onDestroy(() => {
  unlisten1();
  unlisten2();
  unlisten3();
  unlisten4();
});
```

### Svelte 完整示例

```html
<script>
  import { onMount, onDestroy } from 'svelte';
  import { syncManager, table, smartInit } from '@bishen/tauri-plugin-pg-sync';
  
  const users = table('users');
  
  let userList = [];
  let syncStatus = 'offline';
  
  onMount(async () => {
    await smartInit({ remoteUrl: import.meta.env.VITE_PG_URL });
    userList = await users.findAll();
    
    await syncManager.start({
      pollInterval: 30000,
      enableRealtime: true,
      onSync: async (result) => {
        if (result.pulled > 0) {
          userList = await users.findAll();
        }
      },
      onStateChange: (state) => {
        syncStatus = state.mode;
      }
    });
  });
  
  onDestroy(() => syncManager.stop());
</script>

<div class="status">{syncStatus}</div>
<ul>
  {#each userList as user}
    <li>{user.name}</li>
  {/each}
</ul>
```

### 数据流

```
PostgreSQL NOTIFY → Rust 监听 → Tauri 事件 → onSync 回调 → 刷新 UI
```

---

## 最佳实践

### 1. 移动端使用

```javascript
// 使用移动端优化初始化
const { nodeId } = await initAppMobile({
  remoteUrl: import.meta.env.VITE_PG_URL
});

// 使用智能同步管理器
await syncManager.start({
  pollInterval: 60000,  // 移动端可以更长间隔
  onStateChange: (state) => {
    if (state.mode === 'weak_network') {
      showToast('网络不稳定，数据将在恢复后同步');
    }
  }
});
```

### 2. 离线场景

```javascript
// 检查在线状态
const online = await sync.isOnline();

if (!online) {
  // 离线时照常操作，数据会在恢复后同步
  await users.insert({ name: '离线数据' });
}

// 手动触发同步
if (await sync.isOnline()) {
  const result = await sync.now();
  console.log(`同步完成: ${result.pushed} 推送, ${result.pulled} 拉取`);
}
```

### 3. 批量操作

```javascript
// 推荐：使用批量 API
const ids = await users.insertMany(largeDataArray);

// 不推荐：循环单条插入
for (const item of largeDataArray) {
  await users.insert(item);  // 性能差
}
```
