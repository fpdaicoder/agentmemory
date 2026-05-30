# AgentMemory 系统 — 详细设计文档

> **版本**: v1.0  
> **日期**: 2026-05-30  
> **语言**: Rust  
> **数据库**: SQLite (通过 rusqlite + bundled)  
> **状态**: 设计阶段

---

## 目录

1. [项目概述](#1-项目概述)
2. [系统架构](#2-系统架构)
3. [记忆模型设计](#3-记忆模型设计)
4. [数据库设计](#4-数据库设计)
5. [核心模块设计](#5-核心模块设计)
6. [API 设计](#6-api-设计)
7. [记忆生命周期](#7-记忆生命周期)
8. [检索与排序](#8-检索与排序)
9. [项目结构](#9-项目结构)
10. [依赖选型](#10-依赖选型)
11. [未来扩展](#11-未来扩展)

---

## 1. 项目概述

### 1.1 背景

AI Agent 在多轮对话和跨会话场景中面临"遗忘"问题——每次会话都是无状态的。AgentMemory 是一个轻量级的、嵌入式的 Agent 记忆系统，为 Agent 提供持久化的记忆存储和智能检索能力，使其具备跨会话的上下文保持和知识积累能力。

### 1.2 目标

| 目标 | 说明 |
|------|------|
| **轻量嵌入式** | 作为库 (library) 集成，无需独立服务，单文件 SQLite 数据库 |
| **多类型记忆** | 支持情景记忆、语义记忆、过程记忆三种核心类型 |
| **智能检索** | 基于关键词 (FTS5) + 时间衰减 + 访问频率的混合排序 |
| **自动管理** | 记忆蒸馏、过期清理、去重、访问计数 |
| **Rust 原生** | 类型安全、零成本抽象、高性能 |

### 1.3 非目标

- 不做向量数据库 / embedding 相似度搜索（v1 阶段）
- 不做分布式存储
- 不做 LLM 调用封装
- 不做 REST/gRPC 服务

---

## 2. 系统架构

```
┌─────────────────────────────────────────────────────┐
│                    Agent Application                 │
│                                                     │
│   ┌─────────────┐  ┌─────────────┐  ┌───────────┐ │
│   │  Store      │  │  Recall     │  │  Manage   │ │
│   │  (写入记忆) │  │  (检索记忆) │  │  (管理)   │ │
│   └──────┬──────┘  └──────┬──────┘  └─────┬─────┘ │
│          │                │               │        │
├──────────┼────────────────┼───────────────┼────────┤
│          ▼                ▼               ▼        │
│   ┌──────────────────────────────────────────────┐ │
│   │           AgentMemory Core API               │ │
│   │                                              │ │
│   │  ┌────────────┐ ┌──────────┐ ┌────────────┐ │ │
│   │  │ MemoryStore│ │ Search   │ │ Lifecycle  │ │ │
│   │  │ (CRUD)     │ │ Engine   │ │ Manager    │ │ │
│   │  └─────┬──────┘ └────┬─────┘ └──────┬─────┘ │ │
│   │        │             │              │        │ │
│   │        ▼             ▼              ▼        │ │
│   │  ┌────────────┐ ┌──────────┐ ┌────────────┐ │ │
│   │  │ Schema     │ │ FTS5     │ │ Decay &    │ │ │
│   │  │ Layer      │ │ Index    │ │ Distill    │ │ │
│   │  └─────┬──────┘ └────┬─────┘ └──────┬─────┘ │ │
│   └────────┼──────────────┼──────────────┼───────┘ │
│           └──────────────┬┴──────────────┘         │
│                          ▼                          │
│                ┌──────────────────┐                 │
│                │   SQLite (rusqlite)│                │
│                │   agent_memory.db │                │
│                └──────────────────┘                 │
└─────────────────────────────────────────────────────┘
```

### 2.1 分层说明

| 层 | 职责 |
|----|------|
| **Core API** | 对外暴露的公共接口，包含 `AgentMemory` 主结构体 |
| **MemoryStore** | 记忆的 CRUD 操作，负责数据验证和持久化 |
| **SearchEngine** | 基于 FTS5 的全文检索 + 自定义排序函数 |
| **LifecycleManager** | 记忆生命周期管理：衰减、蒸馏、过期、去重 |
| **Schema Layer** | 数据库迁移、表结构管理 |
| **SQLite** | 底层存储引擎，通过 rusqlite 访问 |

---

## 3. 记忆模型设计

### 3.1 记忆类型 (MemoryTier)

```rust
/// 记忆层级
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MemoryTier {
    /// 情景记忆 — 原始对话、事件日志
    /// 特点：量大、时效性强、可被蒸馏压缩
    Episodic,
    /// 语义记忆 — 提炼后的事实、知识
    /// 特点：精炼、长期有效、由情景记忆蒸馏而来
    Semantic,
    /// 过程记忆 — 规则、偏好、工作流
    /// 特点：持久、结构化、显式写入
    Procedural,
}
```

### 3.2 核心数据结构

```rust
/// 记忆记录
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Memory {
    /// 唯一标识
    pub id: i64,
    /// 记忆层级
    pub tier: MemoryTier,
    /// 来源标识（如 "user:alice", "session:123"）
    pub source: String,
    /// 记忆内容（纯文本）
    pub content: String,
    /// 结构化元数据（JSON）
    pub metadata: serde_json::Value,
    /// 关键词标签（用于分类和检索）
    pub tags: Vec<String>,
    /// 创建时间 (Unix timestamp)
    pub created_at: i64,
    /// 最后访问时间
    pub last_accessed_at: i64,
    /// 更新时间
    pub updated_at: i64,
    /// 过期时间（None 表示永不过期）
    pub expires_at: Option<i64>,
    /// 是否已被蒸馏（从 Episodic 提炼为 Semantic）
    pub distilled: bool,
    /// 置信度 [0.0, 1.0]
    pub confidence: f64,
    /// 访问次数
    pub access_count: i64,
    /// 关联的实体名称列表
    pub entities: Vec<String>,
}
```

### 3.3 元数据 (Metadata) 规范

```rust
/// metadata 字段的约定结构
{
    "session_id": "abc123",           // 所属会话
    "topic": "database_design",       // 主题分类
    "importance": 0.8,                // 重要度 [0, 1]
    "entities": ["user", "database"], // 涉及的实体
    "origin": "llm_summary",         // 来源类型: user_input | llm_summary | system
    "raw_refs": [42, 43]             // 蒸馏来源的原始记忆 ID
}
```

### 3.4 各类型记忆的特征对比

| 特征 | Episodic | Semantic | Procedural |
|------|----------|----------|------------|
| 写入方式 | 自动捕获 | 蒸馏生成 | 显式写入 |
| 保留期限 | 短（默认 7 天） | 长（默认 90 天） | 永久 |
| 典型大小 | 50-500 tokens | 10-50 tokens | 20-100 tokens |
| 蒸馏比例 | 原始 | ~50:1 压缩 | N/A |
| 过期策略 | 可过期 | 可过期 | 不过期 |
| 典型来源 | 对话记录 | LLM 总结 | 用户/系统规则 |

---

## 4. 数据库设计

### 4.1 ER 图

```
┌──────────────────┐       ┌──────────────────┐
│    memories      │       │    entities      │
├──────────────────┤       ├──────────────────┤
│ id (PK)          │       │ id (PK)          │
│ tier             │       │ name (UNIQUE)    │
│ source           │       │ entity_type      │
│ content          │       │ properties (JSON)│
│ metadata (JSON)  │       │ created_at       │
│ tags (JSON)      │       │ updated_at       │
│ created_at       │       └────────┬─────────┘
│ last_accessed_at │                │
│ updated_at       │       ┌────────┴─────────┐
│ expires_at       │       │  relationships   │
│ distilled        │       ├──────────────────┤
│ confidence       │       │ id (PK)          │
│ access_count     │       │ source_id (FK)   │
└──────────────────┘       │ target_id (FK)   │
                           │ relation_type    │
                           │ properties (JSON)│
                           │ created_at       │
                           └──────────────────┘

┌──────────────────────────┐
│  memories_fts (虚拟表)    │
├──────────────────────────┤
│ rowid (= memories.id)    │
│ content                  │
│ source                   │
│ tags                     │
└──────────────────────────┘
```

### 4.2 表结构定义

```sql
-- ============================================================
-- 记忆主表
-- ============================================================
CREATE TABLE IF NOT EXISTS memories (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    tier            TEXT    NOT NULL CHECK(tier IN ('episodic', 'semantic', 'procedural')),
    source          TEXT    NOT NULL,
    content         TEXT    NOT NULL,
    metadata        TEXT    NOT NULL DEFAULT '{}',    -- JSON
    tags            TEXT    NOT NULL DEFAULT '[]',    -- JSON array of strings
    created_at      INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    last_accessed_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at      INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    expires_at      INTEGER,                          -- NULL = never expires
    distilled       INTEGER NOT NULL DEFAULT 0,       -- 0 = false, 1 = true
    confidence      REAL    NOT NULL DEFAULT 1.0,
    access_count    INTEGER NOT NULL DEFAULT 0
);

-- 索引
CREATE INDEX IF NOT EXISTS idx_memories_tier       ON memories(tier);
CREATE INDEX IF NOT EXISTS idx_memories_source     ON memories(source);
CREATE INDEX IF NOT EXISTS idx_memories_distilled  ON memories(distilled);
CREATE INDEX IF NOT EXISTS idx_memories_expires    ON memories(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_memories_created    ON memories(created_at);
CREATE INDEX IF NOT EXISTS idx_memories_confidence ON memories(confidence);

-- ============================================================
-- FTS5 全文检索虚拟表
-- ============================================================
CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    content,
    source,
    tags,
    content='memories',
    content_rowid='id',
    tokenize='unicode61'               -- 支持 Unicode（中文等）
);

-- FTS5 同步触发器
CREATE TRIGGER IF NOT EXISTS memories_fts_insert AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts(rowid, content, source, tags)
    VALUES (new.id, new.content, new.source, new.tags);
END;

CREATE TRIGGER IF NOT EXISTS memories_fts_update AFTER UPDATE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, content, source, tags)
    VALUES ('delete', old.id, old.content, old.source, old.tags);
    INSERT INTO memories_fts(rowid, content, source, tags)
    VALUES (new.id, new.content, new.source, new.tags);
END;

CREATE TRIGGER IF NOT EXISTS memories_fts_delete AFTER DELETE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, content, source, tags)
    VALUES ('delete', old.id, old.content, old.source, old.tags);
END;

-- ============================================================
-- 实体表
-- ============================================================
CREATE TABLE IF NOT EXISTS entities (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    NOT NULL UNIQUE,
    entity_type TEXT    NOT NULL DEFAULT 'generic',    -- person | project | concept | tool | generic
    properties  TEXT    NOT NULL DEFAULT '{}',         -- JSON
    created_at  INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at  INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);

CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(entity_type);

-- ============================================================
-- 关系表
-- ============================================================
CREATE TABLE IF NOT EXISTS relationships (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id     INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    target_id     INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    relation_type TEXT    NOT NULL,                       -- e.g. "works_on", "prefers", "related_to"
    properties    TEXT    NOT NULL DEFAULT '{}',          -- JSON
    created_at    INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);

CREATE INDEX IF NOT EXISTS idx_rel_source     ON relationships(source_id);
CREATE INDEX IF NOT EXISTS idx_rel_target     ON relationships(target_id);
CREATE INDEX IF NOT EXISTS idx_rel_type       ON relationships(relation_type);

-- ============================================================
-- Schema 版本管理
-- ============================================================
CREATE TABLE IF NOT EXISTS schema_version (
    version     INTEGER PRIMARY KEY,
    applied_at  INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    description TEXT
);
```

### 4.3 设计要点

| 决策 | 原因 |
|------|------|
| `metadata` / `tags` 用 TEXT 存储 JSON | SQLite 无原生 JSON 列类型，rusqlite 可轻松 serde 序列化 |
| FTS5 使用 `content=` 外部内容模式 | 避免数据冗余，通过触发器保持同步 |
| `tokenize='unicode61'` | 支持中文、日文等 Unicode 字符 |
| `expires_at` 用 Unix timestamp | 便于整数比较和跨平台一致性 |
| `distilled` 用 INTEGER 而非 BOOLEAN | SQLite 无原生布尔类型 |
| 独立 `entities` / `relationships` 表 | 为知识图谱查询预留扩展能力 |
| `schema_version` 表 | 支持数据库迁移和版本管理 |

---

## 5. 核心模块设计

### 5.1 模块职责

```
src/
├── lib.rs                  # 库入口，re-export 公共 API
├── error.rs                # 统一错误类型
├── models/
│   ├── mod.rs              # 模型定义
│   ├── memory.rs           # Memory 结构体
│   ├── entity.rs           # Entity 结构体
│   └── relationship.rs     # Relationship 结构体
├── store.rs                # AgentMemory 主入口，持有 DB 连接
├── search.rs               # 检索引擎
├── lifecycle.rs            # 生命周期管理（衰减、蒸馏、清理）
└── schema.rs               # 数据库 schema 初始化和迁移
```

### 5.2 各模块详细设计

#### 5.2.1 `error.rs` — 错误处理

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Memory not found: {id}")]
    NotFound { id: i64 },

    #[error("Invalid tier: {0}")]
    InvalidTier(String),

    #[error("Memory expired: {id}")]
    Expired { id: i64 },

    #[error("Validation error: {0}")]
    Validation(String),
}

pub type Result<T> = std::result::Result<T, MemoryError>;
```

#### 5.2.2 `store.rs` — 核心存储（AgentMemory 主结构体）

```rust
pub struct AgentMemory {
    conn: rusqlite::Connection,
}

impl AgentMemory {
    // --- 初始化 ---

    /// 打开或创建指定路径的数据库
    pub fn open(path: &str) -> Result<Self>;

    /// 在内存中创建临时数据库（用于测试）
    pub fn open_in_memory() -> Result<Self>;

    // --- 写入 ---

    /// 存储一条新记忆
    pub fn store(&self, request: StoreRequest) -> Result<Memory>;

    /// 批量存储记忆
    pub fn store_batch(&self, requests: Vec<StoreRequest>) -> Result<Vec<Memory>>;

    // --- 读取 ---

    /// 根据 ID 获取单条记忆（自动增加 access_count）
    pub fn get(&self, id: i64) -> Result<Memory>;

    /// 获取所有记忆（分页）
    pub fn list(&self, filter: MemoryFilter) -> Result<Vec<Memory>>;

    /// 统计记忆总数
    pub fn count(&self, filter: MemoryFilter) -> Result<i64>;

    // --- 更新 ---

    /// 更新记忆内容
    pub fn update(&self, id: i64, content: &str, metadata: Option<Value>) -> Result<Memory>;

    /// 增加标签
    pub fn add_tags(&self, id: i64, tags: &[String]) -> Result<()>;

    // --- 删除 ---

    /// 删除单条记忆
    pub fn delete(&self, id: i64) -> Result<()>;

    /// 批量删除记忆
    pub fn delete_batch(&self, ids: &[i64]) -> Result<()>;

    // --- 检索 ---

    /// 全文搜索记忆
    pub fn search(&self, query: &str, opts: SearchOptions) -> Result<Vec<ScoredMemory>>;

    /// 按实体检索关联记忆
    pub fn search_by_entity(&self, entity: &str, opts: SearchOptions) -> Result<Vec<ScoredMemory>>;

    // --- 生命周期 ---

    /// 清理过期记忆
    pub fn cleanup_expired(&self) -> Result<CleanupResult>;

    /// 蒸馏情景记忆为语义记忆
    pub fn distill_episodic(&self) -> Result<DistillResult>;

    /// 获取系统健康状态
    pub fn health(&self) -> Result<HealthReport>;
}
```

#### 5.2.3 `search.rs` — 检索引擎

```rust
/// 搜索选项
pub struct SearchOptions {
    /// 限制返回数量（默认 10）
    pub limit: i64,
    /// 偏移量（用于分页）
    pub offset: i64,
    /// 仅搜索指定层级
    pub tier: Option<MemoryTier>,
    /// 仅搜索指定来源
    pub source: Option<String>,
    /// 最小置信度阈值
    pub min_confidence: Option<f64>,
    /// 是否启用时间衰减排序
    pub time_decay: bool,
}

/// 带分数的检索结果
pub struct ScoredMemory {
    pub memory: Memory,
    pub score: f64,
}
```

**排序算法**:

```
final_score = fts_rank × 0.5
            + time_decay_score × 0.25
            + frequency_score × 0.25
            × confidence

其中:
  fts_rank       = BM25 标准分 (FTS5 内置 rank)
  time_decay_score = exp(-λ × hours_since_last_access), λ = 0.01
  frequency_score = min(1.0, log(1 + access_count) / log(1 + 100))
  confidence     = memory.confidence
```

#### 5.2.4 `lifecycle.rs` — 生命周期管理

```rust
/// 清理结果
pub struct CleanupResult {
    pub deleted_count: i64,
    pub freed_memories: Vec<i64>,
}

/// 蒸馏结果
pub struct DistillResult {
    pub source_count: i64,       // 被蒸馏的原始记忆数
    pub produced_count: i64,     // 生成的语义记忆数
    pub compression_ratio: f64,  // 压缩比
}

/// 健康报告
pub struct HealthReport {
    pub total_memories: i64,
    pub by_tier: HashMap<String, i64>,
    pub expired_count: i64,
    pub distilled_ratio: f64,
    pub avg_confidence: f64,
    pub stale_count: i64,        // 30 天未访问的记忆数
    pub db_size_bytes: i64,
}
```

**衰减策略**:

```
记忆的 time_decay_score 随时间指数衰减：

  score(t) = importance × exp(-λ × Δt)

  Δt = 当前时间 - last_accessed_at（小时）
  λ  = 默认 0.01（约 3 天半衰期）
  importance = metadata.importance（默认 0.5）

每次被检索到时，last_accessed_at 刷新为当前时间，
使经常被使用的记忆保持"新鲜"。
```

---

## 6. API 设计

### 6.1 写入请求

```rust
/// 存储记忆请求
pub struct StoreRequest {
    pub tier: MemoryTier,
    pub source: String,
    pub content: String,
    pub metadata: Option<serde_json::Value>,
    pub tags: Option<Vec<String>>,
    pub expires_at: Option<i64>,        // Unix timestamp
    pub confidence: Option<f64>,        // 默认 1.0
    pub entities: Option<Vec<String>>,  // 关联实体名
}
```

### 6.2 查询过滤

```rust
/// 记忆过滤条件
pub struct MemoryFilter {
    pub tier: Option<MemoryTier>,
    pub source: Option<String>,
    pub tags: Option<Vec<String>>,           // 包含任一标签
    pub min_confidence: Option<f64>,
    pub distilled: Option<bool>,
    pub created_after: Option<i64>,
    pub created_before: Option<i64>,
    pub limit: i64,                          // 默认 50
    pub offset: i64,                         // 默认 0
    pub order_by: OrderBy,                   // 默认 created_at DESC
}

pub enum OrderBy {
    CreatedAtAsc,
    CreatedAtDesc,
    UpdatedAtAsc,
    UpdatedAtDesc,
    ConfidenceDesc,
    AccessCountDesc,
}
```

### 6.3 典型使用流程

```rust
use agentmemory::{AgentMemory, MemoryTier, StoreRequest, SearchOptions};

fn main() -> anyhow::Result<()> {
    // 1. 打开数据库
    let mem = AgentMemory::open("agent_memory.db")?;

    // 2. 存储情景记忆（对话中自动捕获）
    mem.store(StoreRequest {
        tier: MemoryTier::Episodic,
        source: "user:alice".into(),
        content: "用户 Alice 偏好使用 PostgreSQL 而非 MySQL".into(),
        tags: Some(vec!["preference".into(), "database".into()]),
        confidence: Some(0.9),
        entities: Some(vec!["alice".into(), "postgresql".into()]),
        ..Default::default()
    })?;

    // 3. 存储过程记忆（显式规则）
    mem.store(StoreRequest {
        tier: MemoryTier::Procedural,
        source: "system".into(),
        content: "始终使用 UTF-8 编码处理用户输入".into(),
        tags: Some(vec!["rule".into(), "encoding".into()]),
        ..Default::default()
    })?;

    // 4. 检索记忆
    let results = mem.search(
        "数据库偏好",
        SearchOptions {
            limit: 5,
            ..Default::default()
        },
    )?;

    for scored in &results {
        println!("[{:.2}] {}", scored.score, scored.memory.content);
    }

    // 5. 生命周期管理
    let cleanup = mem.cleanup_expired()?;
    println!("清理了 {} 条过期记忆", cleanup.deleted_count);

    let health = mem.health()?;
    println!("系统状态: {:#?}", health);

    Ok(())
}
```

---

## 7. 记忆生命周期

### 7.1 生命周期状态机

```
                    ┌──────────────┐
                    │   Created    │
                    │  (写入记忆)   │
                    └──────┬───────┘
                           │
              ┌────────────┼─────────────┐
              │            │             │
              ▼            ▼             ▼
     ┌────────────┐ ┌────────────┐ ┌────────────┐
     │  Episodic  │ │  Semantic  │ │ Procedural │
     │  情景记忆  │ │  语义记忆  │ │  过程记忆  │
     └─────┬──────┘ └────────────┘ └────────────┘
           │
           │ 蒸馏（distill）
           │ 50:1 压缩
           ▼
     ┌────────────┐
     │  Semantic  │
     │  (由蒸馏   │
     │   生成)    │
     └─────┬──────┘
           │
           │ 时间衰减 + 不再被访问
           │
           ▼
     ┌────────────┐
     │  Expired   │──── cleanup_expired() 删除
     │  (已过期)  │
     └────────────┘

被访问时 (get/search 命中):
  ┌────────────┐
  │  access +1 │─── last_accessed_at 刷新
  │  保持活跃  │
  └────────────┘
```

### 7.2 蒸馏流程

```
Episodic 记忆 (多条)
    │
    │ 1. 按时间窗口分组（如同一 session_id）
    │ 2. 按 topic / entities 聚类
    │ 3. 提取关键事实（由调用方通过 LLM 完成）
    │
    ▼
Semantic 记忆 (少量)
    - distilled = true
    - confidence 继承或平均
    - metadata.raw_refs 记录来源 ID
    - 原始 Episodic 记忆标记为 distilled = true
```

> **注意**: v1 阶段，蒸馏的"提取关键事实"步骤由调用方通过 LLM 完成。`distill_episodic()` 仅负责：
> 1. 查找可蒸馏的 Episodic 记忆（`distilled = false`）
> 2. 按条件分组返回给调用方
> 3. 调用方写入新的 Semantic 记忆后，标记原始记忆为 `distilled = true`

---

## 8. 检索与排序

### 8.1 检索流程

```
用户查询 "数据库偏好"
       │
       ▼
  ┌─────────────┐
  │ FTS5 全文检索 │──── 候选集 (ID + BM25 rank)
  └──────┬──────┘
         │
         ▼
  ┌──────────────┐
  │ 条件过滤      │──── tier / source / confidence
  └──────┬───────┘
         │
         ▼
  ┌──────────────┐
  │ 综合排序      │──── fts_rank + time_decay + frequency
  │ (Rust 侧)    │     × confidence
  └──────┬───────┘
         │
         ▼
  ┌──────────────┐
  │ 返回 Top-K    │
  └──────────────┘
```

### 8.2 排序公式详解

```rust
/// 计算记忆的综合得分
fn compute_score(memory: &Memory, fts_rank: f64, now: i64) -> f64 {
    // FTS5 BM25 得分 (归一化到 [0, 1])
    let fts_score = 1.0 - (-fts_rank).exp();  // sigmoid 归一化

    // 时间衰减得分
    let hours_elapsed = (now - memory.last_accessed_at) as f64 / 3600.0;
    let lambda = 0.01; // 衰减系数
    let time_score = (-lambda * hours_elapsed).exp();

    // 访问频率得分 (对数归一化)
    let freq_score = (1.0 + memory.access_count as f64).ln()
                   / (1.0 + 100.0_f64).ln();

    // 加权综合
    let base = fts_score * 0.5 + time_score * 0.25 + freq_score * 0.25;

    // 乘以置信度
    base * memory.confidence
}
```

---

## 9. 项目结构

```
agentmemory/
├── Cargo.toml
├── README.md
├── docs/
│   └── design.md              # 本文档
├── src/
│   ├── lib.rs                 # 库入口，re-export
│   ├── error.rs               # 统一错误类型
│   ├── models/
│   │   ├── mod.rs
│   │   ├── memory.rs          # Memory, MemoryTier, StoreRequest, MemoryFilter, ...
│   │   ├── entity.rs          # Entity, EntityType
│   │   └── relationship.rs    # Relationship
│   ├── store.rs               # AgentMemory 主结构体 (CRUD)
│   ├── search.rs              # 检索引擎 (FTS5 + 排序)
│   ├── lifecycle.rs           # 生命周期管理
│   └── schema.rs              # Schema 初始化和迁移
└── tests/
    ├── integration_test.rs    # 集成测试
    └── fixtures/              # 测试数据
```

---

## 10. 依赖选型

```toml
[package]
name = "agentmemory"
version = "0.1.0"
edition = "2021"
description = "A lightweight embedded memory system for AI agents"

[dependencies]
rusqlite = { version = "0.34", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
tempfile = "3"
```

| 依赖 | 用途 | 选型理由 |
|------|------|----------|
| `rusqlite` (bundled) | SQLite 访问 | Rust 生态 SQLite 标准库，bundled 免系统依赖 |
| `serde` + `serde_json` | 序列化 | metadata/tags JSON 处理 |
| `thiserror` | 错误定义 | Rust 错误处理最佳实践 |
| `chrono` | 时间处理 | 时间戳格式化和计算 |
| `tempfile` (dev) | 测试 | 测试时创建临时数据库文件 |

---

## 11. 未来扩展

| 优先级 | 扩展方向 | 说明 |
|--------|----------|------|
| P1 | 向量检索 | 集成 `sqlite-vec`，支持 embedding 相似度搜索 |
| P1 | 自动蒸馏 | 集成 LLM API，自动将 Episodic 蒸馏为 Semantic |
| P2 | 记忆去重 | 基于 trigram 相似度的自动去重（阈值 0.85） |
| P2 | 导入/导出 | JSONL 格式的记忆导入导出 |
| P2 | 多 Agent 隔离 | 基于 agent_id 的记忆命名空间隔离 |
| P3 | CLI 工具 | 命令行工具用于查看和管理记忆 |
| P3 | HTTP API | 基于 Axum 的 REST API 服务 |
| P3 | 观测性 | Prometheus metrics + 日志结构化 |

---

## 附录 A: 数据库操作关键 SQL

### A.1 全文检索

```sql
SELECT m.*, fts.rank as fts_rank
FROM memories_fts fts
JOIN memories m ON m.id = fts.rowid
WHERE memories_fts MATCH ?
  AND (? IS NULL OR m.tier = ?)
  AND m.confidence >= ?
ORDER BY fts.rank DESC
LIMIT ? OFFSET ?;
```

### A.2 清理过期记忆

```sql
DELETE FROM memories
WHERE expires_at IS NOT NULL
  AND expires_at < strftime('%s', 'now');
```

### A.3 查找可蒸馏的情景记忆

```sql
SELECT *
FROM memories
WHERE tier = 'episodic'
  AND distilled = 0
  AND created_at < strftime('%s', 'now') - ?  -- 超过指定年龄
ORDER BY created_at ASC;
```

### A.4 健康统计

```sql
SELECT
    tier,
    COUNT(*) as total,
    AVG(confidence) as avg_confidence,
    SUM(CASE WHEN distilled = 1 THEN 1 ELSE 0 END) as distilled_count,
    SUM(CASE WHEN last_accessed_at < strftime('%s','now') - 2592000 THEN 1 ELSE 0 END) as stale_count
FROM memories
GROUP BY tier;
```

---

*文档结束*
