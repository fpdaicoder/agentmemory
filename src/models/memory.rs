use serde::{Deserialize, Serialize};

use crate::error::{MemoryError, Result};

// ---------------------------------------------------------------------------
// MemoryTier
// ---------------------------------------------------------------------------

/// 记忆层级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryTier {
    /// 情景记忆 — 原始对话、事件日志
    Episodic,
    /// 语义记忆 — 提炼后的事实、知识
    Semantic,
    /// 过程记忆 — 规则、偏好、工作流
    Procedural,
}

impl MemoryTier {
    /// 数据库中存储的字符串表示
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Episodic => "episodic",
            Self::Semantic => "semantic",
            Self::Procedural => "procedural",
        }
    }

    /// 从字符串解析，失败返回 `MemoryError::InvalidTier`
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "episodic" => Ok(Self::Episodic),
            "semantic" => Ok(Self::Semantic),
            "procedural" => Ok(Self::Procedural),
            other => Err(MemoryError::InvalidTier(other.to_string())),
        }
    }

    /// 迭代所有层级
    pub fn all() -> &'static [MemoryTier] {
        &[Self::Episodic, Self::Semantic, Self::Procedural]
    }
}

impl std::fmt::Display for MemoryTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// OrderBy
// ---------------------------------------------------------------------------

/// 排序方式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderBy {
    CreatedAtAsc,
    CreatedAtDesc,
    UpdatedAtAsc,
    UpdatedAtDesc,
    ConfidenceDesc,
    AccessCountDesc,
}

impl Default for OrderBy {
    fn default() -> Self {
        Self::CreatedAtDesc
    }
}

impl OrderBy {
    /// 转换为 SQL ORDER BY 子句
    pub fn to_sql(&self) -> &'static str {
        match self {
            Self::CreatedAtAsc => "created_at ASC",
            Self::CreatedAtDesc => "created_at DESC",
            Self::UpdatedAtAsc => "updated_at ASC",
            Self::UpdatedAtDesc => "updated_at DESC",
            Self::ConfidenceDesc => "confidence DESC",
            Self::AccessCountDesc => "access_count DESC",
        }
    }
}

// ---------------------------------------------------------------------------
// StoreRequest
// ---------------------------------------------------------------------------

/// 存储记忆请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreRequest {
    /// 记忆层级
    pub tier: MemoryTier,
    /// 来源标识（如 "user:alice", "session:123"）
    pub source: String,
    /// 记忆内容
    pub content: String,
    /// 结构化元数据
    pub metadata: Option<serde_json::Value>,
    /// 标签列表
    pub tags: Option<Vec<String>>,
    /// 过期时间 (Unix timestamp)，None 表示永不过期
    pub expires_at: Option<i64>,
    /// 置信度 [0.0, 1.0]，默认 1.0
    pub confidence: Option<f64>,
    /// 关联实体名列表
    pub entities: Option<Vec<String>>,
}

impl StoreRequest {
    /// 创建一个简单的 StoreRequest
    pub fn new(tier: MemoryTier, source: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tier,
            source: source.into(),
            content: content.into(),
            metadata: None,
            tags: None,
            expires_at: None,
            confidence: None,
            entities: None,
        }
    }

    /// 设置标签
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = Some(tags);
        self
    }

    /// 设置元数据
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// 设置过期时间
    pub fn with_expires_at(mut self, expires_at: i64) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// 设置置信度
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = Some(confidence);
        self
    }

    /// 设置关联实体
    pub fn with_entities(mut self, entities: Vec<String>) -> Self {
        self.entities = Some(entities);
        self
    }
}

// ---------------------------------------------------------------------------
// MemoryFilter
// ---------------------------------------------------------------------------

/// 记忆过滤条件
#[derive(Debug, Clone, Default)]
pub struct MemoryFilter {
    /// 仅筛选指定层级
    pub tier: Option<MemoryTier>,
    /// 仅筛选指定来源
    pub source: Option<String>,
    /// 包含任一标签
    pub tags: Option<Vec<String>>,
    /// 最小置信度
    pub min_confidence: Option<f64>,
    /// 是否已蒸馏
    pub distilled: Option<bool>,
    /// 创建时间起始 (Unix timestamp)
    pub created_after: Option<i64>,
    /// 创建时间截止 (Unix timestamp)
    pub created_before: Option<i64>,
    /// 返回数量限制
    pub limit: i64,
    /// 偏移量
    pub offset: i64,
    /// 排序方式
    pub order_by: OrderBy,
}

impl MemoryFilter {
    /// 创建一个新的 filter，使用默认值
    pub fn new() -> Self {
        Self {
            limit: 50,
            offset: 0,
            order_by: OrderBy::default(),
            ..Default::default()
        }
    }

    /// 设置层级过滤
    pub fn with_tier(mut self, tier: MemoryTier) -> Self {
        self.tier = Some(tier);
        self
    }

    /// 设置来源过滤
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// 设置标签过滤
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = Some(tags);
        self
    }

    /// 设置 limit
    pub fn with_limit(mut self, limit: i64) -> Self {
        self.limit = limit;
        self
    }

    /// 设置 offset
    pub fn with_offset(mut self, offset: i64) -> Self {
        self.offset = offset;
        self
    }
}

// ---------------------------------------------------------------------------
// SearchOptions
// ---------------------------------------------------------------------------

/// 搜索选项
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// 返回数量限制（默认 10）
    pub limit: i64,
    /// 偏移量
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

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            limit: 10,
            offset: 0,
            tier: None,
            source: None,
            min_confidence: None,
            time_decay: true,
        }
    }
}

impl SearchOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_limit(mut self, limit: i64) -> Self {
        self.limit = limit;
        self
    }

    pub fn with_tier(mut self, tier: MemoryTier) -> Self {
        self.tier = Some(tier);
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_min_confidence(mut self, min_confidence: f64) -> Self {
        self.min_confidence = Some(min_confidence);
        self
    }
}

// ---------------------------------------------------------------------------
// Memory (完整记录)
// ---------------------------------------------------------------------------

/// 记忆记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    /// 唯一标识
    pub id: i64,
    /// 记忆层级
    pub tier: MemoryTier,
    /// 来源标识
    pub source: String,
    /// 记忆内容
    pub content: String,
    /// 结构化元数据 (JSON)
    pub metadata: serde_json::Value,
    /// 关键词标签
    pub tags: Vec<String>,
    /// 创建时间 (Unix timestamp)
    pub created_at: i64,
    /// 最后访问时间
    pub last_accessed_at: i64,
    /// 更新时间
    pub updated_at: i64,
    /// 过期时间
    pub expires_at: Option<i64>,
    /// 是否已被蒸馏
    pub distilled: bool,
    /// 置信度
    pub confidence: f64,
    /// 访问次数
    pub access_count: i64,
}

impl Memory {
    /// 从数据库行解析（返回 rusqlite::Result 以适配 query_row/query_map 回调）
    pub fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        let tier_str: String = row.get("tier")?;
        let metadata_str: String = row.get("metadata")?;
        let tags_str: String = row.get("tags")?;

        Ok(Self {
            id: row.get("id")?,
            tier: MemoryTier::from_str(&tier_str)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
            source: row.get("source")?,
            content: row.get("content")?,
            metadata: serde_json::from_str(&metadata_str)
                .unwrap_or(serde_json::Value::Object(Default::default())),
            tags: serde_json::from_str(&tags_str).unwrap_or_default(),
            created_at: row.get("created_at")?,
            last_accessed_at: row.get("last_accessed_at")?,
            updated_at: row.get("updated_at")?,
            expires_at: row.get("expires_at")?,
            distilled: row.get::<_, i32>("distilled")? != 0,
            confidence: row.get("confidence")?,
            access_count: row.get("access_count")?,
        })
    }
}

// ---------------------------------------------------------------------------
// ScoredMemory
// ---------------------------------------------------------------------------

/// 带分数的检索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredMemory {
    /// 记忆记录
    pub memory: Memory,
    /// 综合得分
    pub score: f64,
}
