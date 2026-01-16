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
