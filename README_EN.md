# tauri-plugin-pg-sync

[![Crates.io](https://img.shields.io/crates/v/tauri-plugin-pg-sync.svg)](https://crates.io/crates/tauri-plugin-pg-sync)
[![npm](https://img.shields.io/npm/v/@bishen/tauri-plugin-pg-sync.svg)](https://www.npmjs.com/package/@bishen/tauri-plugin-pg-sync)

An offline-first PostgreSQL sync plugin for Tauri apps with cross-platform support.

[中文](./README.md)

## Supported Platforms

| Platform | Architecture | Status |
|----------|--------------|--------|
| Windows | x86_64, aarch64 | ✅ |
| macOS | x86_64, aarch64 | ✅ |
| Linux | x86_64, aarch64 | ✅ |
| Android | arm64-v8a, armeabi-v7a, x86_64 | ✅ |
| iOS | arm64, x86_64 (Simulator) | ✅ |

## Features

- **Offline-First**: Local SQLite database, fully functional offline
- **PostgreSQL Sync**: Automatic synchronization with remote PostgreSQL database
- **Conflict Resolution**: HLC (Hybrid Logical Clock) based conflict resolution
- **GIS Support**: Optional spatial data processing capabilities
- **Cross-Platform**: Supports all Tauri-supported platforms

## Installation

### Rust Dependency

Add to `src-tauri/Cargo.toml`:

```toml
[dependencies]
tauri-plugin-pg-sync = "0.1"
```

### JavaScript Dependency

```bash
npm install @bishen/tauri-plugin-pg-sync
# or
pnpm add @bishen/tauri-plugin-pg-sync
# or
yarn add @bishen/tauri-plugin-pg-sync
```

## Configuration

### 1. Register Plugin

In `src-tauri/src/main.rs` or `src-tauri/src/lib.rs`:

```rust
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_pg_sync::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### 2. Configure Permissions

Add to `src-tauri/capabilities/default.json`:

```json
{
  "permissions": [
    "pg-sync:default"
  ]
}
```

## Usage

### JavaScript/TypeScript

```typescript
import {
  initDatabase,
  ensureTable,
  insert,
  findAll,
  connectRemote,
  syncNow,
} from '@bishen/tauri-plugin-pg-sync';

// 1. Initialize local database
const nodeId = await initDatabase();

// 2. Define table schema
await ensureTable('users', {
  columns: [
    ['name', 'TEXT NOT NULL'],
    ['email', 'TEXT'],
    ['age', 'INTEGER'],
  ],
});

// 3. CRUD operations (works offline)
const id = await insert('users', {
  name: 'Alice',
  email: 'alice@example.com',
  age: 25,
});

const users = await findAll('users');

// 4. Connect to remote database and sync
await connectRemote('postgres://user:pass@host:5432/db');
const result = await syncNow();
console.log(`Pushed: ${result.pushed}, Pulled: ${result.pulled}`);
```

### Advanced Queries

```typescript
import { query, findWhere, count } from '@bishen/tauri-plugin-pg-sync';

// Conditional query
const adults = await findWhere('users', { age: { $gte: 18 } });

// Advanced query
const results = await query('users', {
  where: { age: { $gte: 18 } },
  orderBy: 'name',
  orderDesc: false,
  limit: 10,
  offset: 0,
  select: ['id', 'name', 'email'],
});

// Count
const total = await count('users');
const adultCount = await count('users', { age: { $gte: 18 } });
```

### Batch Operations

```typescript
import { insertMany, updateMany, deleteMany } from '@bishen/tauri-plugin-pg-sync';

// Batch insert
const ids = await insertMany('users', [
  { name: 'Bob', email: 'bob@example.com' },
  { name: 'Charlie', email: 'charlie@example.com' },
]);

// Batch update
await updateMany('users', [
  [ids[0], { age: 30 }],
  [ids[1], { age: 25 }],
]);

// Batch delete
await deleteMany('users', ids);
```

### Table Schema Sync

```typescript
import {
  pushTableSchema,
  pullTableSchema,
  listLocalTables,
  listRemoteTables,
} from '@bishen/tauri-plugin-pg-sync';

// Push local table schema to remote
await pushTableSchema('users');

// Pull table schema from remote
await pullTableSchema('products');

// List tables
const localTables = await listLocalTables();
const remoteTables = await listRemoteTables();
```

## API Reference

### Initialization

| Function | Description |
|----------|-------------|
| `initDatabase()` | Initialize local database (Desktop) |
| `initDatabaseMobile()` | Initialize local database (Mobile optimized) |
| `getDbPath()` | Get database file path |

### Remote Connection

| Function | Description |
|----------|-------------|
| `connectRemote(url)` | Connect to remote PostgreSQL |
| `connectRemoteWithRetry(url)` | Connect with retry (weak network optimized) |
| `startAutoReconnect()` | Start auto-reconnect |
| `disconnectRemote()` | Disconnect |
| `syncNow()` | Sync immediately |
| `isOnline()` | Check online status |

### CRUD

| Function | Description |
|----------|-------------|
| `insert(table, data)` | Insert record |
| `update(table, id, data)` | Update record |
| `remove(table, id)` | Delete record (soft delete) |
| `findById(table, id)` | Find by ID |
| `findAll(table, limit?, offset?)` | Find all |
| `findWhere(table, conditions)` | Conditional query |
| `query(table, options)` | Advanced query |
| `count(table, conditions?)` | Count |

### Batch Operations

| Function | Description |
|----------|-------------|
| `insertMany(table, items)` | Batch insert |
| `updateMany(table, updates)` | Batch update |
| `deleteMany(table, ids)` | Batch delete |
| `clearTable(table)` | Clear table |

## License

MIT

Copyright (c) 2026 BiShen <bishen@live.com>
算金山™ (https://www.suanjinshan.com/)
