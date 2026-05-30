use serde::{Deserialize, Serialize};

/// 实体类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityType {
    Person,
    Project,
    Concept,
    Tool,
    Generic,
}

impl EntityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Project => "project",
            Self::Concept => "concept",
            Self::Tool => "tool",
            Self::Generic => "generic",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "person" => Self::Person,
            "project" => Self::Project,
            "concept" => Self::Concept,
            "tool" => Self::Tool,
            _ => Self::Generic,
        }
    }
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 实体记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// 唯一标识
    pub id: i64,
    /// 实体名称（唯一）
    pub name: String,
    /// 实体类型
    pub entity_type: EntityType,
    /// 属性 (JSON)
    pub properties: serde_json::Value,
    /// 创建时间
    pub created_at: i64,
    /// 更新时间
    pub updated_at: i64,
}

impl Entity {
    /// 从数据库行解析
    pub fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        let entity_type_str: String = row.get("entity_type")?;
        let props_str: String = row.get("properties")?;

        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            entity_type: EntityType::from_str(&entity_type_str),
            properties: serde_json::from_str(&props_str)
                .unwrap_or(serde_json::Value::Object(Default::default())),
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}
