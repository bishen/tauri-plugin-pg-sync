# @bishen/tauri-plugin-pg-sync

[![npm](https://img.shields.io/npm/v/@bishen/tauri-plugin-pg-sync.svg)](https://www.npmjs.com/package/@bishen/tauri-plugin-pg-sync)
[![crates.io](https://img.shields.io/crates/v/tauri-plugin-pg-sync.svg)](https://crates.io/crates/tauri-plugin-pg-sync)

An offline-first PostgreSQL sync plugin for Tauri apps.

## Features

- **Offline-First**: Local SQLite database, fully functional offline
- **PostgreSQL Sync**: Automatic synchronization with remote PostgreSQL
- **Conflict Resolution**: HLC (Hybrid Logical Clock) based conflict resolution
- **Cross-Platform**: Windows, macOS, Linux, Android, iOS

## Installation

```bash
npm install @bishen/tauri-plugin-pg-sync
# or
yarn add @bishen/tauri-plugin-pg-sync
# or
pnpm add @bishen/tauri-plugin-pg-sync
```

Also add the Rust plugin to your `src-tauri/Cargo.toml`:

```toml
[dependencies]
tauri-plugin-pg-sync = "0.1"
```

## Setup

Register the plugin in your Tauri app:

```rust
// src-tauri/src/main.rs
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_pg_sync::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

## Documentation

- [API Reference](https://github.com/bishen/tauri-plugin-pg-sync/blob/master/API.md) - Complete API documentation
- [Server Setup](https://github.com/bishen/tauri-plugin-pg-sync/blob/master/SERVER_SETUP.md) - PostgreSQL configuration
- [Type Mapping](https://github.com/bishen/tauri-plugin-pg-sync/blob/master/TYPE_MAPPING.md) - SQLite ↔ PostgreSQL types

## Usage

### Smart Initialization (Recommended)

```typescript
import { smartInit, table, sync } from '@bishen/tauri-plugin-pg-sync';

// Smart init: auto-detect local/remote state
const result = await smartInit({
  remoteUrl: 'postgres://user:pass@host:5432/db'
});

console.log('Mode:', result.mode);
// 'local_only' | 'offline' | 'pulled_from_remote' | 'pushed_to_remote' | 'synced'

const users = table('users', {
  columns: [['name', 'TEXT'], ['age', 'INTEGER']]
});

await users.insert({ name: 'Alice', age: 25 });
await sync.now();
```

### Smart Sync Manager

```typescript
import { syncManager, smartInit } from '@bishen/tauri-plugin-pg-sync';

await smartInit({ remoteUrl: 'postgres://...' });

await syncManager.start({
  pollInterval: 30000,
  enableRealtime: true,
  onSync: (result) => {
    if (result.pulled > 0) refreshUI();
  },
  onStateChange: ({ mode }) => {
    console.log('State:', mode); // 'offline' | 'online' | 'syncing' | 'error'
  }
});

syncManager.stop();
```

### Basic Usage

```typescript
import {
  initDatabase,
  connectRemote,
  ensureTable,
  insert,
  findAll,
  findWhere,
  update,
  delete as deleteRecord,
  syncNow,
  isOnline,
} from '@bishen/tauri-plugin-pg-sync';

// Initialize local database
const nodeId = await initDatabase();

// Connect to remote PostgreSQL (optional)
await connectRemote('postgres://user:pass@host:5432/db');

// Create a table
await ensureTable('users', [
  { name: 'name', type: 'TEXT' },
  { name: 'email', type: 'TEXT' },
  { name: 'age', type: 'INTEGER' },
]);

// Insert data
const id = await insert('users', {
  name: 'Alice',
  email: 'alice@example.com',
  age: 30,
});

// Query data
const allUsers = await findAll('users');
const adults = await findWhere('users', 'age >= 18');

// Update data
await update('users', id, { age: 31 });

// Delete data (soft delete)
await deleteRecord('users', id);

// Sync with remote
const result = await syncNow();
console.log(`Pushed: ${result.pushed}, Pulled: ${result.pulled}`);

// Check online status
const online = await isOnline();
```

## API Reference

### Database Initialization

| Function | Description |
|----------|-------------|
| `smartInit(options)` | Smart initialization (Recommended) |
| `initDatabase()` | Initialize local SQLite database |
| `initDatabaseMobile(path)` | Initialize for mobile with custom path |
| `getDbPath()` | Get current database file path |

### Remote Connection

| Function | Description |
|----------|-------------|
| `connectRemote(url)` | Connect to PostgreSQL |
| `connectRemoteWithRetry(url, retries, delay)` | Connect with retry logic |
| `disconnectRemote()` | Disconnect from remote |
| `isOnline()` | Check connection status |

### CRUD Operations

| Function | Description |
|----------|-------------|
| `insert(table, data)` | Insert a record |
| `insertMany(table, records)` | Batch insert |
| `update(table, id, data)` | Update a record |
| `updateMany(table, updates)` | Batch update |
| `delete(table, id)` | Soft delete a record |
| `deleteMany(table, ids)` | Batch soft delete |
| `findById(table, id)` | Find by ID |
| `findAll(table, options?)` | Find all records |
| `findWhere(table, condition, options?)` | Find with condition |
| `query(sql, params?)` | Execute raw SQL |
| `count(table, condition?)` | Count records |

### Sync Operations

| Function | Description |
|----------|-------------|
| `syncNow()` | Trigger immediate sync |
| `startAutoReconnect(interval)` | Start auto-reconnect |

### Table Management

| Function | Description |
|----------|-------------|
| `ensureTable(name, columns)` | Create table if not exists |
| `listLocalTables()` | List local tables |
| `listRemoteTables()` | List remote tables |
| `getLocalSchema(table)` | Get table schema |
| `pushTableSchema(table)` | Push schema to remote |
| `pullTableSchema(table)` | Pull schema from remote |

## License

MIT © [BiShen](mailto:bishen@live.com)

算金山™ (https://www.suanjinshan.com/)
