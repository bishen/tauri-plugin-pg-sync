# SQLite ↔ PostgreSQL 类型映射

本文档定义了本地 SQLite 数据库与远程 PostgreSQL 数据库之间的字段类型映射关系，包括 GIS 空间类型。

## 基础类型映射

| SQLite 类型 | PostgreSQL 类型 | 说明 |
|-------------|-----------------|------|
| `TEXT` | `TEXT` / `VARCHAR` | 字符串 |
| `INTEGER` | `INTEGER` / `BIGINT` | 整数 |
| `REAL` | `REAL` / `DOUBLE PRECISION` | 浮点数 |
| `BLOB` | `BYTEA` | 二进制数据 |
| `NULL` | `NULL` | 空值 |

## 扩展类型映射

| SQLite 类型 | PostgreSQL 类型 | 说明 | 同步转换 |
|-------------|-----------------|------|----------|
| `TEXT` | `UUID` | UUID 主键 | 直接映射 |
| `TEXT` | `TIMESTAMP` / `TIMESTAMPTZ` | 时间戳 | ISO8601 字符串 |
| `TEXT` | `DATE` | 日期 | YYYY-MM-DD |
| `TEXT` | `TIME` | 时间 | HH:MM:SS |
| `TEXT` | `JSON` / `JSONB` | JSON 数据 | JSON 字符串 |
| `INTEGER` | `BOOLEAN` | 布尔值 | 0/1 ↔ false/true |
| `REAL` | `NUMERIC` / `DECIMAL` | 精确数值 | 可能有精度损失 |
| `TEXT` | `ENUM` | 枚举类型 | 字符串值 |
| `TEXT` | `ARRAY` | 数组类型 | JSON 字符串 |

## 同步元字段映射

每个同步表必须包含以下元字段：

| 字段名 | SQLite 类型 | PostgreSQL 类型 | 说明 |
|--------|-------------|-----------------|------|
| `id` | `TEXT PRIMARY KEY` | `UUID PRIMARY KEY` | 主键 |
| `_hlc` | `TEXT NOT NULL` | `TEXT NOT NULL` | 混合逻辑时钟 |
| `_node_id` | `TEXT NOT NULL` | `TEXT NOT NULL` | 节点标识 |
| `_version` | `INTEGER DEFAULT 1` | `INTEGER DEFAULT 1` | 版本号 |
| `_deleted` | `INTEGER DEFAULT 0` | `BOOLEAN DEFAULT FALSE` | 软删除标记 |
| `_synced` | `INTEGER DEFAULT 0` | `BOOLEAN DEFAULT TRUE` | 同步状态（仅本地） |

## GIS 空间类型映射

### 坐标存储方式

| 存储方式 | SQLite | PostgreSQL | 说明 |
|----------|--------|------------|------|
| **分离坐标** | `lng REAL`, `lat REAL` | `lng DOUBLE PRECISION`, `lat DOUBLE PRECISION` | 简单，易查询 |
| **WKB 二进制** | `geom BLOB` | `geom GEOMETRY` | Spatialite/PostGIS 原生 |
| **WKT 文本** | `geom TEXT` | `geom GEOMETRY` | 可读性好 |
| **GeoJSON** | `geom TEXT` | `geom GEOMETRY` / `JSONB` | Web 友好 |

### Spatialite ↔ PostGIS 类型映射

| Spatialite 类型 | PostGIS 类型 | 说明 |
|-----------------|--------------|------|
| `POINT` | `POINT` | 点 |
| `LINESTRING` | `LINESTRING` | 线 |
| `POLYGON` | `POLYGON` | 多边形 |
| `MULTIPOINT` | `MULTIPOINT` | 多点 |
| `MULTILINESTRING` | `MULTILINESTRING` | 多线 |
| `MULTIPOLYGON` | `MULTIPOLYGON` | 多多边形 |
| `GEOMETRYCOLLECTION` | `GEOMETRYCOLLECTION` | 几何集合 |

### 坐标系统 (SRID)

| SRID | 名称 | 说明 |
|------|------|------|
| `4326` | WGS84 | GPS 坐标，经纬度（推荐） |
| `3857` | Web Mercator | Web 地图投影 |
| `4490` | CGCS2000 | 中国国家大地坐标系 |

## 表定义示例

### 前端 JavaScript 定义

```javascript
// 基础表（用户）
const users = table('users', {
  columns: [
    ['name', 'TEXT'],           // → PostgreSQL TEXT
    ['age', 'INTEGER'],         // → PostgreSQL INTEGER
    ['email', 'TEXT'],          // → PostgreSQL TEXT
    ['is_active', 'INTEGER'],   // → PostgreSQL BOOLEAN
    ['created_at', 'TEXT'],     // → PostgreSQL TIMESTAMPTZ
    ['metadata', 'TEXT']        // → PostgreSQL JSONB
  ]
});

// GIS 表（位置）- 分离坐标方式
const locations = table('locations', {
  columns: [
    ['name', 'TEXT'],
    ['address', 'TEXT'],
    ['lng', 'REAL'],            // → PostgreSQL DOUBLE PRECISION
    ['lat', 'REAL'],            // → PostgreSQL DOUBLE PRECISION
    ['altitude', 'REAL']
  ]
});

// GIS 表（轨迹）- WKB 方式
const tracks = table('tracks', {
  columns: [
    ['name', 'TEXT'],
    ['geom', 'BLOB'],           // → PostgreSQL GEOMETRY(LINESTRING, 4326)
    ['distance', 'REAL'],
    ['duration', 'INTEGER']
  ]
});

// GIS 表（区域）- GeoJSON 方式
const areas = table('areas', {
  columns: [
    ['name', 'TEXT'],
    ['geojson', 'TEXT'],        // → PostgreSQL JSONB 或 GEOMETRY
    ['area_sqm', 'REAL']
  ]
});
```

### PostgreSQL 建表语句

```sql
-- 用户表
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT,
    age INTEGER,
    email TEXT,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    metadata JSONB,
    -- 同步元字段
    _hlc TEXT NOT NULL,
    _node_id TEXT NOT NULL,
    _version INTEGER DEFAULT 1,
    _deleted BOOLEAN DEFAULT FALSE
);

-- 位置表（分离坐标）
CREATE TABLE locations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT,
    address TEXT,
    lng DOUBLE PRECISION,
    lat DOUBLE PRECISION,
    altitude DOUBLE PRECISION,
    -- 同步元字段
    _hlc TEXT NOT NULL,
    _node_id TEXT NOT NULL,
    _version INTEGER DEFAULT 1,
    _deleted BOOLEAN DEFAULT FALSE
);

-- 可选：添加 PostGIS 空间索引
ALTER TABLE locations ADD COLUMN geom GEOMETRY(POINT, 4326);
CREATE INDEX idx_locations_geom ON locations USING GIST(geom);

-- 触发器：自动更新 geom 字段
CREATE OR REPLACE FUNCTION update_location_geom()
RETURNS TRIGGER AS $$
BEGIN
    NEW.geom = ST_SetSRID(ST_MakePoint(NEW.lng, NEW.lat), 4326);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_locations_geom
BEFORE INSERT OR UPDATE ON locations
FOR EACH ROW EXECUTE FUNCTION update_location_geom();

-- 轨迹表（PostGIS 几何）
CREATE TABLE tracks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT,
    geom GEOMETRY(LINESTRING, 4326),
    distance DOUBLE PRECISION,
    duration INTEGER,
    -- 同步元字段
    _hlc TEXT NOT NULL,
    _node_id TEXT NOT NULL,
    _version INTEGER DEFAULT 1,
    _deleted BOOLEAN DEFAULT FALSE
);

CREATE INDEX idx_tracks_geom ON tracks USING GIST(geom);

-- 区域表
CREATE TABLE areas (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT,
    geojson JSONB,
    geom GEOMETRY(POLYGON, 4326),
    area_sqm DOUBLE PRECISION,
    -- 同步元字段
    _hlc TEXT NOT NULL,
    _node_id TEXT NOT NULL,
    _version INTEGER DEFAULT 1,
    _deleted BOOLEAN DEFAULT FALSE
);

CREATE INDEX idx_areas_geom ON areas USING GIST(geom);
```

## 同步转换规则

### SQLite → PostgreSQL（推送）

| SQLite 值 | PostgreSQL 转换 |
|-----------|-----------------|
| `TEXT` (UUID) | 直接使用 |
| `TEXT` (ISO8601) | `CAST(value AS TIMESTAMPTZ)` |
| `INTEGER` (0/1) | `value = 1` → `TRUE/FALSE` |
| `TEXT` (JSON) | `value::JSONB` |
| `BLOB` (WKB) | `ST_GeomFromWKB(value, 4326)` |
| `TEXT` (WKT) | `ST_GeomFromText(value, 4326)` |
| `TEXT` (GeoJSON) | `ST_GeomFromGeoJSON(value)` |

### PostgreSQL → SQLite（拉取）

| PostgreSQL 值 | SQLite 转换 |
|---------------|-------------|
| `UUID` | 直接使用 (TEXT) |
| `TIMESTAMP` | `TO_CHAR(value, 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')` |
| `BOOLEAN` | `TRUE` → `1`, `FALSE` → `0` |
| `JSONB` | `value::TEXT` |
| `GEOMETRY` (WKB) | `ST_AsBinary(value)` |
| `GEOMETRY` (WKT) | `ST_AsText(value)` |
| `GEOMETRY` (GeoJSON) | `ST_AsGeoJSON(value)` |

## GIS 同步策略

### 推荐方式：分离坐标 + 服务端几何

```
┌─────────────────────────────────────────────────────────────┐
│                      SQLite (本地)                          │
│  id | name | lng | lat | ...                               │
│  (简单存储，无需 Spatialite)                                │
└─────────────────────────────────────────────────────────────┘
                              │
                              │ 同步 (JSON)
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    PostgreSQL (服务端)                       │
│  id | name | lng | lat | geom | ...                        │
│  (触发器自动生成 geom，支持 PostGIS 空间查询)               │
└─────────────────────────────────────────────────────────────┘
```

### 高级方式：WKB 同步

```
┌─────────────────────────────────────────────────────────────┐
│                  SQLite + Spatialite (本地)                  │
│  id | name | geom (BLOB/WKB) | ...                         │
│  (完整空间查询能力)                                         │
└─────────────────────────────────────────────────────────────┘
                              │
                              │ 同步 (WKB Hex)
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                PostgreSQL + PostGIS (服务端)                 │
│  id | name | geom (GEOMETRY) | ...                         │
│  ST_GeomFromWKB(hex_string)                                │
└─────────────────────────────────────────────────────────────┘
```

## 类型转换代码示例

### Rust 端类型转换

```rust
// SQLite INTEGER → PostgreSQL BOOLEAN
fn int_to_bool(value: i64) -> bool {
    value != 0
}

// PostgreSQL BOOLEAN → SQLite INTEGER
fn bool_to_int(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

// 经纬度 → WKB (需要 geo 和 wkb crate)
fn coords_to_wkb(lng: f64, lat: f64) -> Vec<u8> {
    use geo::Point;
    use wkb::geom_to_wkb;
    
    let point = Point::new(lng, lat);
    geom_to_wkb(&point.into()).unwrap()
}

// WKB → 经纬度
fn wkb_to_coords(wkb_data: &[u8]) -> Option<(f64, f64)> {
    use geo::Point;
    use wkb::wkb_to_geom;
    
    let geom = wkb_to_geom(wkb_data).ok()?;
    if let geo::Geometry::Point(p) = geom {
        Some((p.x(), p.y()))
    } else {
        None
    }
}
```

### 前端类型转换

```javascript
// ISO8601 时间戳
function toISOTimestamp(date) {
  return date.toISOString();
}

function fromISOTimestamp(str) {
  return new Date(str);
}

// 布尔值
function boolToInt(value) {
  return value ? 1 : 0;
}

function intToBool(value) {
  return value !== 0;
}

// GeoJSON 字符串
function coordsToGeoJSON(lng, lat) {
  return JSON.stringify({
    type: 'Point',
    coordinates: [lng, lat]
  });
}

function geoJSONToCoords(str) {
  const obj = JSON.parse(str);
  return {
    lng: obj.coordinates[0],
    lat: obj.coordinates[1]
  };
}
```

## 最佳实践

1. **主键**: 使用 `TEXT` (SQLite) / `UUID` (PostgreSQL)，由客户端生成
2. **时间戳**: 使用 ISO8601 字符串，时区使用 UTC
3. **布尔值**: SQLite 使用 `INTEGER` (0/1)，PostgreSQL 使用 `BOOLEAN`
4. **JSON**: 使用 `TEXT` 存储 JSON 字符串
5. **GIS 简单场景**: 使用分离坐标 (`lng`, `lat`)
6. **GIS 复杂场景**: 使用 WKB/WKT，服务端 PostGIS 处理
7. **枚举类型**: 使用 `TEXT` 存储，应用层验证
8. **数组类型**: 使用 JSON 字符串表示

## 注意事项

- SQLite 是动态类型，PostgreSQL 是静态类型，同步时需确保类型兼容
- 浮点数 (REAL) 可能有精度差异，金额等精确数值建议使用 `TEXT` 存储
- GIS 数据在 SQLite 侧使用 Spatialite 可获得完整空间查询能力
- 不使用 Spatialite 时，复杂空间查询应在服务端 PostGIS 执行
