use serde::{Deserialize, Serialize};

/// 实体关系记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    /// 唯一标识
    pub id: i64,
    /// 源实体 ID
    pub source_id: i64,
    /// 目标实体 ID
    pub target_id: i64,
    /// 关系类型（如 "works_on", "prefers", "related_to"）
    pub relation_type: String,
    /// 属性 (JSON)
    pub properties: serde_json::Value,
    /// 创建时间
    pub created_at: i64,
}

impl Relationship {
    /// 从数据库行解析
    pub fn from_row(row: &rusqlite::Row<'_>) -> crate::error::Result<Self> {
        let props_str: String = row.get("properties")?;

        Ok(Self {
            id: row.get("id")?,
            source_id: row.get("source_id")?,
            target_id: row.get("target_id")?,
            relation_type: row.get("relation_type")?,
            properties: serde_json::from_str(&props_str)
                .unwrap_or(serde_json::Value::Object(Default::default())),
            created_at: row.get("created_at")?,
        })
    }
}
