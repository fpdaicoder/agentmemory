use rusqlite::params;

use crate::error::{MemoryError, Result};
use crate::lifecycle::{self, CleanupResult, HealthReport};
use crate::models::*;
use crate::schema;
use crate::search;

// ---------------------------------------------------------------------------
// AgentMemory — 主入口
// ---------------------------------------------------------------------------

/// AgentMemory 系统，持有 SQLite 连接
pub struct AgentMemory {
    conn: rusqlite::Connection,
}

impl AgentMemory {
    // -----------------------------------------------------------------------
    // 初始化
    // -----------------------------------------------------------------------

    /// 打开（或创建）指定路径的数据库文件
    pub fn open(path: &str) -> Result<Self> {
        let conn = rusqlite::Connection::open(path)?;
        schema::init_schema(&conn)?;
        Ok(Self { conn })
    }

    /// 在内存中创建临时数据库（用于测试）
    pub fn open_in_memory() -> Result<Self> {
        let conn = rusqlite::Connection::open_in_memory()?;
        schema::init_schema(&conn)?;
        Ok(Self { conn })
    }

    // -----------------------------------------------------------------------
    // 写入
    // -----------------------------------------------------------------------

    /// 存储一条新记忆
    pub fn store(&self, req: StoreRequest) -> Result<Memory> {
        // 验证
        if req.content.is_empty() {
            return Err(MemoryError::Validation("content must not be empty".into()));
        }
        if req.source.is_empty() {
            return Err(MemoryError::Validation("source must not be empty".into()));
        }

        let metadata_json = serde_json::to_string(
            req.metadata
                .as_ref()
                .unwrap_or(&serde_json::Value::Object(Default::default())),
        )?;
        let tags_json = serde_json::to_string(
            req.tags
                .as_ref()
                .unwrap_or(&Vec::<String>::new()),
        )?;
        let confidence = req.confidence.unwrap_or(1.0);

        // 处理关联实体（确保实体存在）
        if let Some(ref entities) = req.entities {
            for name in entities {
                self.ensure_entity(name)?;
            }
        }

        self.conn.execute(
            "INSERT INTO memories (tier, source, content, metadata, tags, expires_at, distilled, confidence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)",
            params![
                req.tier.as_str(),
                req.source,
                req.content,
                metadata_json,
                tags_json,
                req.expires_at,
                confidence,
            ],
        )?;

        let id = self.conn.last_insert_rowid();
        self.get(id)
    }

    /// 批量存储记忆
    pub fn store_batch(&self, requests: Vec<StoreRequest>) -> Result<Vec<Memory>> {
        let mut results = Vec::with_capacity(requests.len());
        for req in requests {
            results.push(self.store(req)?);
        }
        Ok(results)
    }

    // -----------------------------------------------------------------------
    // 读取
    // -----------------------------------------------------------------------

    /// 根据 ID 获取单条记忆（自动增加 access_count 和刷新 last_accessed_at）
    pub fn get(&self, id: i64) -> Result<Memory> {
        // 先更新访问信息
        let now = chrono::Utc::now().timestamp();
        self.conn.execute(
            "UPDATE memories SET access_count = access_count + 1, last_accessed_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;

        let memory = self
            .conn
            .query_row(
                "SELECT * FROM memories WHERE id = ?1",
                params![id],
                Memory::from_row,
            )
            .map_err(|_| MemoryError::NotFound { id })?;

        Ok(memory)
    }

    /// 根据条件列出记忆
    pub fn list(&self, filter: &MemoryFilter) -> Result<Vec<Memory>> {
        let (sql, params) = Self::build_list_query(filter);
        let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_refs.as_slice(), Memory::from_row)?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// 统计记忆总数
    pub fn count(&self, filter: &MemoryFilter) -> Result<i64> {
        let (where_clause, params) = Self::build_where_clause(filter);
        let sql = format!("SELECT COUNT(*) FROM memories {}", where_clause);
        let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let count: i64 = self
            .conn
            .query_row(&sql, params_refs.as_slice(), |row| row.get(0))?;

        Ok(count)
    }

    // -----------------------------------------------------------------------
    // 更新
    // -----------------------------------------------------------------------

    /// 更新记忆内容和元数据
    pub fn update(
        &self,
        id: i64,
        content: &str,
        metadata: Option<serde_json::Value>,
    ) -> Result<Memory> {
        // 先检查记忆是否存在
        let _ = self.conn.query_row(
            "SELECT id FROM memories WHERE id = ?1",
            params![id],
            |row| row.get::<_, i64>(0),
        ).map_err(|_| MemoryError::NotFound { id })?;

        let now = chrono::Utc::now().timestamp();

        if let Some(meta) = metadata {
            let meta_json = serde_json::to_string(&meta)?;
            self.conn.execute(
                "UPDATE memories SET content = ?1, metadata = ?2, updated_at = ?3 WHERE id = ?4",
                params![content, meta_json, now, id],
            )?;
        } else {
            self.conn.execute(
                "UPDATE memories SET content = ?1, updated_at = ?2 WHERE id = ?3",
                params![content, now, id],
            )?;
        }

        self.get(id)
    }

    /// 增加标签
    pub fn add_tags(&self, id: i64, new_tags: &[String]) -> Result<()> {
        // 获取当前 tags
        let current_tags_json: String = self
            .conn
            .query_row(
                "SELECT tags FROM memories WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .map_err(|_| MemoryError::NotFound { id })?;

        let mut tags: Vec<String> = serde_json::from_str(&current_tags_json).unwrap_or_default();
        for tag in new_tags {
            if !tags.contains(tag) {
                tags.push(tag.clone());
            }
        }

        let tags_json = serde_json::to_string(&tags)?;
        let now = chrono::Utc::now().timestamp();
        self.conn.execute(
            "UPDATE memories SET tags = ?1, updated_at = ?2 WHERE id = ?3",
            params![tags_json, now, id],
        )?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // 删除
    // -----------------------------------------------------------------------

    /// 删除单条记忆
    pub fn delete(&self, id: i64) -> Result<()> {
        let affected = self
            .conn
            .execute("DELETE FROM memories WHERE id = ?1", params![id])?;
        if affected == 0 {
            return Err(MemoryError::NotFound { id });
        }
        Ok(())
    }

    /// 批量删除记忆
    pub fn delete_batch(&self, ids: &[i64]) -> Result<()> {
        for &id in ids {
            self.delete(id)?;
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // 检索
    // -----------------------------------------------------------------------

    /// 全文搜索记忆
    pub fn search(&self, query: &str, opts: SearchOptions) -> Result<Vec<ScoredMemory>> {
        search::search(&self.conn, query, opts)
    }

    /// 按实体名称检索关联记忆
    pub fn search_by_entity(
        &self,
        entity_name: &str,
        opts: SearchOptions,
    ) -> Result<Vec<ScoredMemory>> {
        search::search_by_entity(&self.conn, entity_name, opts)
    }

    // -----------------------------------------------------------------------
    // 生命周期
    // -----------------------------------------------------------------------

    /// 清理过期记忆
    pub fn cleanup_expired(&self) -> Result<CleanupResult> {
        lifecycle::cleanup_expired(&self.conn)
    }

    /// 获取可蒸馏的情景记忆
    pub fn get_distillable_episodic(&self, min_age_secs: i64) -> Result<Vec<Memory>> {
        lifecycle::get_distillable_episodic(&self.conn, min_age_secs)
    }

    /// 标记指定记忆为已蒸馏
    pub fn mark_distilled(&self, ids: &[i64]) -> Result<()> {
        lifecycle::mark_distilled(&self.conn, ids)
    }

    /// 获取系统健康状态
    pub fn health(&self) -> Result<HealthReport> {
        lifecycle::health(&self.conn)
    }

    // -----------------------------------------------------------------------
    // 实体操作
    // -----------------------------------------------------------------------

    /// 确保实体存在（不存在则创建）
    fn ensure_entity(&self, name: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO entities (name) VALUES (?1)",
            params![name],
        )?;
        Ok(())
    }

    /// 添加实体关系
    pub fn add_relationship(
        &self,
        source_name: &str,
        target_name: &str,
        relation_type: &str,
    ) -> Result<()> {
        self.ensure_entity(source_name)?;
        self.ensure_entity(target_name)?;

        let source_id: i64 = self
            .conn
            .query_row(
                "SELECT id FROM entities WHERE name = ?1",
                params![source_name],
                |row| row.get(0),
            )?;
        let target_id: i64 = self
            .conn
            .query_row(
                "SELECT id FROM entities WHERE name = ?1",
                params![target_name],
                |row| row.get(0),
            )?;

        self.conn.execute(
            "INSERT INTO relationships (source_id, target_id, relation_type) VALUES (?1, ?2, ?3)",
            params![source_id, target_id, relation_type],
        )?;

        Ok(())
    }

    /// 获取所有实体
    pub fn list_entities(&self) -> Result<Vec<Entity>> {
        let mut stmt = self.conn.prepare("SELECT * FROM entities ORDER BY name")?;
        let rows = stmt.query_map([], Entity::from_row)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    // -----------------------------------------------------------------------
    // 内部查询构建
    // -----------------------------------------------------------------------

    /// 构建 list 查询 SQL 和参数
    fn build_list_query(filter: &MemoryFilter) -> (String, Vec<Box<dyn rusqlite::types::ToSql>>) {
        let (where_clause, params) = Self::build_where_clause(filter);
        let sql = format!(
            "SELECT * FROM memories {} ORDER BY {} LIMIT ? OFFSET ?",
            where_clause,
            filter.order_by.to_sql(),
        );

        let mut all_params = params;
        all_params.push(Box::new(filter.limit));
        all_params.push(Box::new(filter.offset));

        (sql, all_params)
    }

    /// 构建 WHERE 子句
    fn build_where_clause(
        filter: &MemoryFilter,
    ) -> (String, Vec<Box<dyn rusqlite::types::ToSql>>) {
        let mut clauses: Vec<String> = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(ref tier) = filter.tier {
            clauses.push("tier = ?".to_string());
            params.push(Box::new(tier.as_str().to_string()));
        }
        if let Some(ref source) = filter.source {
            clauses.push("source = ?".to_string());
            params.push(Box::new(source.clone()));
        }
        if let Some(min_c) = filter.min_confidence {
            clauses.push("confidence >= ?".to_string());
            params.push(Box::new(min_c));
        }
        if let Some(d) = filter.distilled {
            clauses.push("distilled = ?".to_string());
            params.push(Box::new(d as i32));
        }
        if let Some(after) = filter.created_after {
            clauses.push("created_at >= ?".to_string());
            params.push(Box::new(after));
        }
        if let Some(before) = filter.created_before {
            clauses.push("created_at <= ?".to_string());
            params.push(Box::new(before));
        }
        // tags: LIKE 匹配 JSON 数组中的元素
        if let Some(ref tags) = filter.tags {
            let tag_conditions: Vec<String> = tags
                .iter()
                .map(|_| "tags LIKE ?".to_string())
                .collect();
            clauses.push(format!("({})", tag_conditions.join(" OR ")));
            for tag in tags {
                params.push(Box::new(format!("%\"{}\"%", tag)));
            }
        }

        let where_sql = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };

        (where_sql, params)
    }
}
