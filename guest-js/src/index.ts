/**
 * Tauri Plugin: pg-sync
 *
 * 离线优先的 PostgreSQL 同步插件
 *
 * @example
 * ```typescript
 * import { initDatabase, insert, findAll, syncNow } from '@bishen/tauri-plugin-pg-sync';
 *
 * // 初始化
 * const nodeId = await initDatabase();
 *
 * // CRUD 操作
 * const id = await insert('users', { name: 'Alice' });
 * const users = await findAll('users');
 *
 * // 同步
 * await syncNow();
 * ```
 */

import { invoke } from '@tauri-apps/api/core';

// ============================================================================
// Types
// ============================================================================

export interface TableSchema {
  columns: [string, string][];
}

export interface QueryOptions {
  where?: Record<string, unknown>;
  orderBy?: string;
  orderDesc?: boolean;
  limit?: number;
  offset?: number;
  select?: string[];
}

export interface SyncResult {
  pushed: number;
  pulled: number;
  conflicts: number;
  errors: string[];
}

export interface ColumnDef {
  name: string;
  data_type: string;
  nullable: boolean;
  default: string | null;
}

export interface DeletedStats {
  total: number;
  by_age: Record<string, number>;
}

// ============================================================================
// 初始化
// ============================================================================

/**
 * 初始化数据库（桌面端）
 * @returns Node ID
 */
export async function initDatabase(): Promise<string> {
  return invoke<string>('plugin:pg-sync|init_database');
}

/**
 * 初始化数据库（移动端，使用移动端优化配置）
 * @returns Node ID
 */
export async function initDatabaseMobile(): Promise<string> {
  return invoke<string>('plugin:pg-sync|init_database_mobile');
}

/**
 * 获取数据库文件路径
 */
export async function getDbPath(): Promise<string> {
  return invoke<string>('plugin:pg-sync|get_db_path');
}

// ============================================================================
// 远程连接
// ============================================================================

/**
 * 连接远程 PostgreSQL 数据库
 * @param databaseUrl PostgreSQL 连接字符串
 */
export async function connectRemote(databaseUrl: string): Promise<void> {
  return invoke('plugin:pg-sync|connect_remote', { databaseUrl });
}

/**
 * 带重试的连接（适合弱网环境）
 * @param databaseUrl PostgreSQL 连接字符串
 */
export async function connectRemoteWithRetry(databaseUrl: string): Promise<void> {
  return invoke('plugin:pg-sync|connect_remote_with_retry', { databaseUrl });
}

/**
 * 启动自动重连（后台任务）
 */
export async function startAutoReconnect(): Promise<void> {
  return invoke('plugin:pg-sync|start_auto_reconnect');
}

/**
 * 断开远程连接
 */
export async function disconnectRemote(): Promise<void> {
  return invoke('plugin:pg-sync|disconnect_remote');
}

/**
 * 立即执行同步
 */
export async function syncNow(): Promise<SyncResult> {
  const result = await invoke<string>('plugin:pg-sync|sync_now');
  return JSON.parse(result);
}

/**
 * 检查是否在线
 */
export async function isOnline(): Promise<boolean> {
  return invoke<boolean>('plugin:pg-sync|is_online');
}

// ============================================================================
// 表操作
// ============================================================================

/**
 * 确保表存在（自动建表）
 * @param table 表名
 * @param schema 表结构定义
 */
export async function ensureTable(table: string, schema: TableSchema): Promise<void> {
  return invoke('plugin:pg-sync|ensure_table', { table, schema });
}

// ============================================================================
// CRUD 操作
// ============================================================================

/**
 * 插入记录
 * @param table 表名
 * @param data 数据对象
 * @returns 新记录的 ID
 */
export async function insert(table: string, data: Record<string, unknown>): Promise<string> {
  return invoke<string>('plugin:pg-sync|insert', { table, data });
}

/**
 * 更新记录
 * @param table 表名
 * @param id 记录 ID
 * @param data 更新数据
 * @returns 是否更新成功
 */
export async function update(table: string, id: string, data: Record<string, unknown>): Promise<boolean> {
  return invoke<boolean>('plugin:pg-sync|update', { table, id, data });
}

/**
 * 删除记录（软删除）
 * @param table 表名
 * @param id 记录 ID
 * @returns 是否删除成功
 */
export async function remove(table: string, id: string): Promise<boolean> {
  return invoke<boolean>('plugin:pg-sync|delete', { table, id });
}

/**
 * 根据 ID 查找记录
 * @param table 表名
 * @param id 记录 ID
 */
export async function findById<T = Record<string, unknown>>(table: string, id: string): Promise<T | null> {
  return invoke<T | null>('plugin:pg-sync|find_by_id', { table, id });
}

/**
 * 查找所有记录
 * @param table 表名
 * @param limit 限制数量
 * @param offset 偏移量
 */
export async function findAll<T = Record<string, unknown>>(
  table: string,
  limit?: number,
  offset?: number
): Promise<T[]> {
  return invoke<T[]>('plugin:pg-sync|find_all', { table, limit, offset });
}

/**
 * 条件查询
 * @param table 表名
 * @param conditions 查询条件
 */
export async function findWhere<T = Record<string, unknown>>(
  table: string,
  conditions: Record<string, unknown>
): Promise<T[]> {
  return invoke<T[]>('plugin:pg-sync|find_where', { table, conditions });
}

/**
 * 高级查询
 * @param table 表名
 * @param options 查询选项
 */
export async function query<T = Record<string, unknown>>(
  table: string,
  options: QueryOptions
): Promise<T[]> {
  return invoke<T[]>('plugin:pg-sync|query', { table, options });
}

/**
 * 计数
 * @param table 表名
 * @param conditions 可选的条件
 */
export async function count(table: string, conditions?: Record<string, unknown>): Promise<number> {
  return invoke<number>('plugin:pg-sync|count', { table, conditions });
}

// ============================================================================
// 批量操作
// ============================================================================

/**
 * 批量插入
 * @param table 表名
 * @param items 数据数组
 * @returns 新记录的 ID 数组
 */
export async function insertMany(table: string, items: Record<string, unknown>[]): Promise<string[]> {
  return invoke<string[]>('plugin:pg-sync|insert_many', { table, items });
}

/**
 * 批量更新
 * @param table 表名
 * @param updates [id, data] 元组数组
 * @returns 更新的记录数
 */
export async function updateMany(
  table: string,
  updates: [string, Record<string, unknown>][]
): Promise<number> {
  return invoke<number>('plugin:pg-sync|update_many', { table, updates });
}

/**
 * 批量删除
 * @param table 表名
 * @param ids ID 数组
 * @returns 删除的记录数
 */
export async function deleteMany(table: string, ids: string[]): Promise<number> {
  return invoke<number>('plugin:pg-sync|delete_many', { table, ids });
}

/**
 * 清空表
 * @param table 表名
 * @returns 删除的记录数
 */
export async function clearTable(table: string): Promise<number> {
  return invoke<number>('plugin:pg-sync|clear_table', { table });
}

// ============================================================================
// 表结构
// ============================================================================

/**
 * 获取本地表结构
 * @param table 表名
 */
export async function getLocalSchema(table: string): Promise<ColumnDef[]> {
  return invoke<ColumnDef[]>('plugin:pg-sync|get_local_schema', { table });
}

/**
 * 列出本地所有表
 */
export async function listLocalTables(): Promise<string[]> {
  return invoke<string[]>('plugin:pg-sync|list_local_tables');
}

/**
 * 列出远程所有表
 * @param pgSchema PostgreSQL schema 名称（默认 'public'）
 */
export async function listRemoteTables(pgSchema?: string): Promise<string[]> {
  return invoke<string[]>('plugin:pg-sync|list_remote_tables', { pgSchema });
}

/**
 * 推送表结构到远程
 * @param table 表名
 */
export async function pushTableSchema(table: string): Promise<void> {
  return invoke('plugin:pg-sync|push_table_schema', { table });
}

/**
 * 从远程拉取表结构
 * @param table 表名
 */
export async function pullTableSchema(table: string): Promise<void> {
  return invoke('plugin:pg-sync|pull_table_schema', { table });
}

// ============================================================================
// 清理
// ============================================================================

/**
 * 清理已删除的记录
 * @param table 表名
 * @param daysOld 删除多少天前的记录（默认 30 天）
 * @returns 清理的记录数
 */
export async function purgeDeleted(table: string, daysOld?: number): Promise<number> {
  return invoke<number>('plugin:pg-sync|purge_deleted', { table, daysOld });
}

/**
 * 清理所有表中已删除的记录
 * @param daysOld 删除多少天前的记录（默认 30 天）
 * @returns 清理的记录数
 */
export async function purgeAllDeleted(daysOld?: number): Promise<number> {
  return invoke<number>('plugin:pg-sync|purge_all_deleted', { daysOld });
}

/**
 * 获取已删除记录的统计信息
 * @param table 表名
 */
export async function getDeletedStats(table: string): Promise<DeletedStats> {
  return invoke<DeletedStats>('plugin:pg-sync|get_deleted_stats', { table });
}

// ============================================================================
// 智能同步管理器
// ============================================================================

export type SyncState = 'offline' | 'online' | 'syncing' | 'error';

export interface SyncManagerOptions {
  /** 轮询间隔（毫秒），默认 30000 */
  pollInterval?: number;
  /** 是否启用实时监听，默认 true */
  enableRealtime?: boolean;
  /** 同步完成回调 */
  onSync?: (result: SyncResult) => void | Promise<void>;
  /** 状态变化回调 */
  onStateChange?: (state: { mode: SyncState; previous: SyncState }) => void;
  /** 错误回调 */
  onError?: (error: Error) => void;
}

interface SyncManagerInstance {
  /** 启动同步管理器 */
  start: (options?: SyncManagerOptions) => Promise<void>;
  /** 停止同步管理器 */
  stop: () => void;
  /** 手动触发同步 */
  sync: () => Promise<SyncResult>;
  /** 获取当前状态 */
  getState: () => SyncState;
  /** 是否正在运行 */
  isRunning: () => boolean;
}

function createSyncManager(): SyncManagerInstance {
  let running = false;
  let state: SyncState = 'offline';
  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let unlistenFn: (() => void) | null = null;
  let options: SyncManagerOptions = {};

  const setState = (newState: SyncState) => {
    if (newState !== state) {
      const previous = state;
      state = newState;
      options.onStateChange?.({ mode: state, previous });
    }
  };

  const doSync = async (): Promise<SyncResult> => {
    const wasState = state;
    setState('syncing');
    try {
      const result = await syncNow();
      const online = await isOnline();
      setState(online ? 'online' : 'offline');
      await options.onSync?.(result);
      return result;
    } catch (err) {
      setState('error');
      options.onError?.(err instanceof Error ? err : new Error(String(err)));
      throw err;
    }
  };

  const start = async (opts: SyncManagerOptions = {}) => {
    if (running) return;
    
    options = {
      pollInterval: 30000,
      enableRealtime: true,
      ...opts,
    };
    running = true;

    // 检查初始状态
    try {
      const online = await isOnline();
      setState(online ? 'online' : 'offline');
    } catch {
      setState('offline');
    }

    // 启动轮询
    if (options.pollInterval && options.pollInterval > 0) {
      pollTimer = setInterval(async () => {
        if (running && state !== 'syncing') {
          try {
            await doSync();
          } catch {
            // 错误已在 doSync 中处理
          }
        }
      }, options.pollInterval);
    }

    // 启动实时监听
    if (options.enableRealtime) {
      try {
        const { listen } = await import('@tauri-apps/api/event');
        const unlisten = await listen('sync:data_changed', async () => {
          if (running && state !== 'syncing') {
            try {
              await doSync();
            } catch {
              // 错误已在 doSync 中处理
            }
          }
        });
        unlistenFn = unlisten;
      } catch {
        // 监听失败，继续使用轮询
      }
    }
  };

  const stop = () => {
    running = false;
    if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
    if (unlistenFn) {
      unlistenFn();
      unlistenFn = null;
    }
    setState('offline');
  };

  return {
    start,
    stop,
    sync: doSync,
    getState: () => state,
    isRunning: () => running,
  };
}

/** 智能同步管理器单例 */
export const syncManager = createSyncManager();

// ============================================================================
// 便捷 API（OOP 风格）
// ============================================================================

/** sync 命名空间 - 同步相关操作 */
export const sync = {
  /** 立即同步 */
  now: syncNow,
  /** 检查是否在线 */
  isOnline,
  /** 同步管理器 */
  manager: syncManager,
};

/** 创建表操作对象 */
export function table<T = Record<string, unknown>>(tableName: string, schema?: TableSchema) {
  return {
    /** 确保表存在 */
    ensure: () => schema ? ensureTable(tableName, schema) : Promise.resolve(),
    /** 插入记录 */
    insert: (data: Omit<T, 'id'>) => insert(tableName, data as Record<string, unknown>),
    /** 批量插入 */
    insertMany: (items: Omit<T, 'id'>[]) => insertMany(tableName, items as Record<string, unknown>[]),
    /** 更新记录 */
    update: (id: string, data: Partial<T>) => update(tableName, id, data as Record<string, unknown>),
    /** 删除记录 */
    delete: (id: string) => remove(tableName, id),
    /** 批量删除 */
    deleteMany: (ids: string[]) => deleteMany(tableName, ids),
    /** 根据 ID 查找 */
    findById: (id: string) => findById<T>(tableName, id),
    /** 查找所有 */
    findAll: (limit?: number, offset?: number) => findAll<T>(tableName, limit, offset),
    /** 条件查询 */
    findWhere: (conditions: Partial<T>) => findWhere<T>(tableName, conditions as Record<string, unknown>),
    /** 高级查询 */
    query: (options: QueryOptions) => query<T>(tableName, options),
    /** 计数 */
    count: (conditions?: Partial<T>) => count(tableName, conditions as Record<string, unknown>),
    /** 清空表 */
    clear: () => clearTable(tableName),
  };
}

// ============================================================================
// 智能初始化
// ============================================================================

export type SmartInitMode = 'local_only' | 'offline' | 'pulled_from_remote' | 'pushed_to_remote' | 'synced';

export interface SmartInitOptions {
  /** 远程数据库 URL，不提供则为纯本地模式 */
  remoteUrl?: string;
  /** 是否为移动端 */
  mobile?: boolean;
  /** 连接超时（毫秒） */
  timeout?: number;
}

export interface SmartInitResult {
  /** 节点 ID */
  nodeId: string;
  /** 初始化模式 */
  mode: SmartInitMode;
  /** 是否在线 */
  online: boolean;
  /** 同步结果（如果有同步发生） */
  syncResult?: SyncResult;
}

/**
 * 智能初始化：自动检测本地/远程状态并处理
 */
export async function smartInit(options: SmartInitOptions = {}): Promise<SmartInitResult> {
  const { remoteUrl, mobile = false, timeout = 10000 } = options;

  // 1. 初始化本地数据库
  const nodeId = mobile ? await initDatabaseMobile() : await initDatabase();

  // 纯本地模式
  if (!remoteUrl) {
    return { nodeId, mode: 'local_only', online: false };
  }

  // 2. 尝试连接远程
  try {
    await connectRemoteWithRetry(remoteUrl);
    const online = await isOnline();

    if (!online) {
      return { nodeId, mode: 'offline', online: false };
    }

    // 3. 执行同步
    const syncResult = await syncNow();
    
    let mode: SmartInitMode = 'synced';
    if (syncResult.pulled > 0 && syncResult.pushed === 0) {
      mode = 'pulled_from_remote';
    } else if (syncResult.pushed > 0 && syncResult.pulled === 0) {
      mode = 'pushed_to_remote';
    }

    return { nodeId, mode, online: true, syncResult };
  } catch {
    return { nodeId, mode: 'offline', online: false };
  }
}

/** 移动端智能初始化 */
export async function smartInitMobile(options: Omit<SmartInitOptions, 'mobile'> = {}): Promise<SmartInitResult> {
  return smartInit({ ...options, mobile: true });
}

// ============================================================================
// 兼容性别名（向后兼容）
// ============================================================================

/** @deprecated 使用 smartInit 代替 */
export const initApp = smartInit;

/** @deprecated 使用 smartInitMobile 代替 */
export const initAppMobile = smartInitMobile;

/** @deprecated 使用 smartInit 代替 */
export const initFromRemote = smartInit;

/** @deprecated 使用 syncManager.start() 代替 */
export async function autoSync(options?: {
  interval?: number;
  onSync?: (result: SyncResult) => void;
}): Promise<() => void> {
  await syncManager.start({
    pollInterval: options?.interval ?? 30000,
    enableRealtime: true,
    onSync: options?.onSync,
  });
  return () => syncManager.stop();
}

/** db 命名空间 - 数据库操作 */
export const db = {
  init: initDatabase,
  initMobile: initDatabaseMobile,
  getPath: getDbPath,
  connect: connectRemote,
  connectWithRetry: connectRemoteWithRetry,
  disconnect: disconnectRemote,
  isOnline,
  ensureTable,
  insert,
  insertMany,
  update,
  updateMany,
  delete: remove,
  deleteMany,
  findById,
  findAll,
  findWhere,
  query,
  count,
  clearTable,
  getLocalSchema,
  listLocalTables,
  listRemoteTables,
  pushTableSchema,
  pullTableSchema,
  purgeDeleted,
  purgeAllDeleted,
  getDeletedStats,
};

/** schema 命名空间 - 表结构操作 */
export const schema = {
  ensure: ensureTable,
  getLocal: getLocalSchema,
  listLocal: listLocalTables,
  listRemote: listRemoteTables,
  push: pushTableSchema,
  pull: pullTableSchema,
};
