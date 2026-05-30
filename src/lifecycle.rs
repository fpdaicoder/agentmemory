use std::collections::HashMap;

use rusqlite::params;

use crate::error::Result;
use crate::models::*;

// ---------------------------------------------------------------------------
// 生命周期管理
// ---------------------------------------------------------------------------

/// 清理结果
#[derive(Debug, Clone)]
pub struct CleanupResult {
    /// 被删除的记忆数
    pub deleted_count: i64,
    /// 被删除的记忆 ID 列表
    pub freed_ids: Vec<i64>,
}

/// 蒸馏结果
#[derive(Debug, Clone)]
pub struct DistillResult {
    /// 被蒸馏的原始记忆数
    pub source_count: i64,
    /// 生成的语义记忆数
    pub produced_count: i64,
    /// 压缩比
    pub compression_ratio: f64,
}

/// 健康报告
#[derive(Debug, Clone)]
pub struct HealthReport {
    /// 记忆总数
    pub total_memories: i64,
    /// 按层级统计
    pub by_tier: HashMap<String, i64>,
    /// 过期记忆数
    pub expired_count: i64,
    /// 已蒸馏比例
    pub distilled_ratio: f64,
    /// 平均置信度
    pub avg_confidence: f64,
    /// 陈旧记忆数（30天未访问）
    pub stale_count: i64,
    /// 数据库文件大小（字节），内存数据库返回 0
    pub db_size_bytes: i64,
}

/// 清理过期记忆
pub fn cleanup_expired(conn: &rusqlite::Connection) -> Result<CleanupResult> {
    // 先查询过期记忆 ID
    let now = chrono::Utc::now().timestamp();
    let mut stmt = conn.prepare(
        "SELECT id FROM memories WHERE expires_at IS NOT NULL AND expires_at < ?1",
    )?;
    let rows = stmt.query_map(params![now], |row| row.get::<_, i64>(0))?;

    let mut freed_ids = Vec::new();
    for row in rows {
        freed_ids.push(row?);
    }

    let deleted_count = freed_ids.len() as i64;

    // 批量删除
    for id in &freed_ids {
        conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
    }

    Ok(CleanupResult {
        deleted_count,
        freed_ids,
    })
}

/// 获取可蒸馏的情景记忆（distilled = false 且超过指定年龄）
pub fn get_distillable_episodic(
    conn: &rusqlite::Connection,
    min_age_secs: i64,
) -> Result<Vec<Memory>> {
    let now = chrono::Utc::now().timestamp();
    let cutoff = now - min_age_secs;

    let mut stmt = conn.prepare(
        "SELECT * FROM memories
         WHERE tier = 'episodic' AND distilled = 0 AND created_at < ?1
         ORDER BY created_at ASC",
    )?;

    let rows = stmt.query_map(params![cutoff], Memory::from_row)?;
    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

/// 标记指定记忆为已蒸馏
pub fn mark_distilled(conn: &rusqlite::Connection, ids: &[i64]) -> Result<()> {
    for &id in ids {
        conn.execute(
            "UPDATE memories SET distilled = 1 WHERE id = ?1",
            params![id],
        )?;
    }
    Ok(())
}

/// 获取系统健康报告
pub fn health(conn: &rusqlite::Connection) -> Result<HealthReport> {
    let now = chrono::Utc::now().timestamp();
    let thirty_days_ago = now - 30 * 24 * 3600;

    // 总数和平均值
    let total_memories: i64 = conn
        .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))?;

    let avg_confidence: f64 = conn
        .query_row(
            "SELECT COALESCE(AVG(confidence), 0.0) FROM memories",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0.0);

    let distilled_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memories WHERE distilled = 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let expired_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memories WHERE expires_at IS NOT NULL AND expires_at < ?1",
            params![now],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let stale_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memories WHERE last_accessed_at < ?1",
            params![thirty_days_ago],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // 按层级统计
    let mut by_tier: HashMap<String, i64> = HashMap::new();
    for tier in MemoryTier::all() {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE tier = ?1",
                params![tier.as_str()],
                |row| row.get(0),
            )
            .unwrap_or(0);
        by_tier.insert(tier.to_string(), count);
    }

    let distilled_ratio = if total_memories > 0 {
        distilled_count as f64 / total_memories as f64
    } else {
        0.0
    };

    Ok(HealthReport {
        total_memories,
        by_tier,
        expired_count,
        distilled_ratio,
        avg_confidence,
        stale_count,
        db_size_bytes: 0, // 内存数据库无法获取文件大小
    })
}
