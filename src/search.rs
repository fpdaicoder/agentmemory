use rusqlite::params;

use crate::error::Result;
use crate::models::*;

// ---------------------------------------------------------------------------
// 检索引擎
// ---------------------------------------------------------------------------

/// 全文搜索记忆
pub fn search(
    conn: &rusqlite::Connection,
    query: &str,
    opts: SearchOptions,
) -> Result<Vec<ScoredMemory>> {
    // 构建 FTS5 查询，附加过滤条件
    let mut where_extras = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    // FTS5 MATCH 参数放在第 1 位
    param_values.push(Box::new(query.to_string()));

    if let Some(ref tier) = opts.tier {
        where_extras.push("m.tier = ?".to_string());
        param_values.push(Box::new(tier.as_str().to_string()));
    }
    if let Some(ref source) = opts.source {
        where_extras.push("m.source = ?".to_string());
        param_values.push(Box::new(source.clone()));
    }
    if let Some(min_c) = opts.min_confidence {
        where_extras.push("m.confidence >= ?".to_string());
        param_values.push(Box::new(min_c));
    }

    let where_str = if where_extras.is_empty() {
        String::new()
    } else {
        format!("AND {}", where_extras.join(" AND "))
    };

    let sql = format!(
        "SELECT m.*, fts.rank as fts_rank
         FROM memories_fts fts
         JOIN memories m ON m.id = fts.rowid
         WHERE memories_fts MATCH ?1 {}
         ORDER BY fts.rank DESC
         LIMIT ? OFFSET ?",
        where_str,
    );

    param_values.push(Box::new(opts.limit + opts.offset)); // LIMIT = limit + offset
    param_values.push(Box::new(opts.offset));

    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn.prepare(&sql)?;
    let now = chrono::Utc::now().timestamp();

    let mut results: Vec<ScoredMemory> = Vec::new();
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        let memory = Memory::from_row(row)?;
        let fts_rank: f64 = row.get("fts_rank")?;
        Ok((memory, fts_rank))
    })?;

    for row_result in rows {
        let (memory, fts_rank) = row_result?;
        let score = if opts.time_decay {
            compute_score(&memory, fts_rank, now)
        } else {
            fts_rank
        };
        results.push(ScoredMemory { memory, score });
    }

    // 按综合分数降序排列
    if opts.time_decay {
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    }

    // 更新被检索到的记忆的访问计数
    for scored in &results {
        let _ = conn.execute(
            "UPDATE memories SET access_count = access_count + 1, last_accessed_at = ?1 WHERE id = ?2",
            params![now, scored.memory.id],
        );
    }

    Ok(results)
}

/// 按实体名称检索关联记忆
pub fn search_by_entity(
    conn: &rusqlite::Connection,
    entity_name: &str,
    opts: SearchOptions,
) -> Result<Vec<ScoredMemory>> {
    // 通过 tags LIKE 匹配实体名，或 metadata LIKE 匹配
    let pattern = format!("\"{}\"", entity_name);

    let mut where_extras = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    param_values.push(Box::new(pattern.clone()));
    param_values.push(Box::new(format!("%{}%", entity_name))); // content LIKE

    if let Some(ref tier) = opts.tier {
        where_extras.push("tier = ?".to_string());
        param_values.push(Box::new(tier.as_str().to_string()));
    }
    if let Some(min_c) = opts.min_confidence {
        where_extras.push("confidence >= ?".to_string());
        param_values.push(Box::new(min_c));
    }

    let where_str = if where_extras.is_empty() {
        String::new()
    } else {
        format!("AND {}", where_extras.join(" AND "))
    };

    let sql = format!(
        "SELECT * FROM memories
         WHERE (tags LIKE ?1 OR content LIKE ?2) {}
         ORDER BY confidence DESC, created_at DESC
         LIMIT ? OFFSET ?",
        where_str,
    );

    param_values.push(Box::new(opts.limit + opts.offset));
    param_values.push(Box::new(opts.offset));

    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn.prepare(&sql)?;
    let mut results: Vec<ScoredMemory> = Vec::new();
    let rows = stmt.query_map(param_refs.as_slice(), Memory::from_row)?;

    for row_result in rows {
        let memory = row_result?;
        results.push(ScoredMemory {
            score: memory.confidence,
            memory,
        });
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// 评分算法
// ---------------------------------------------------------------------------

/// 计算记忆的综合得分
///
/// `final_score = (fts × 0.5 + time_decay × 0.25 + frequency × 0.25) × confidence`
pub fn compute_score(memory: &Memory, fts_rank: f64, now: i64) -> f64 {
    // FTS5 BM25 归一化（sigmoid）
    let fts_score = 1.0 - (-fts_rank).exp();

    // 时间衰减
    let hours_elapsed = (now - memory.last_accessed_at).max(0) as f64 / 3600.0;
    let lambda = 0.01;
    let time_score = (-lambda * hours_elapsed).exp();

    // 访问频率（对数归一化）
    let freq_score = (1.0 + memory.access_count as f64).ln() / (101.0_f64).ln();

    // 加权综合 × 置信度
    (fts_score * 0.5 + time_score * 0.25 + freq_score * 0.25) * memory.confidence
}
