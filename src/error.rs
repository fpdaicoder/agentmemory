use thiserror::Error;

/// AgentMemory 统一错误类型
#[derive(Error, Debug)]
pub enum MemoryError {
    /// SQLite 数据库错误
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// JSON 序列化/反序列化错误
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// 记忆未找到
    #[error("Memory not found: {id}")]
    NotFound { id: i64 },

    /// 无效的记忆层级
    #[error("Invalid tier: {0}")]
    InvalidTier(String),

    /// 记忆已过期
    #[error("Memory expired: {id}")]
    Expired { id: i64 },

    /// 数据验证失败
    #[error("Validation error: {0}")]
    Validation(String),
}

/// Alias for `Result<T, MemoryError>`
pub type Result<T> = std::result::Result<T, MemoryError>;
