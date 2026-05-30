use crate::error::Result;

/// 当前 schema 版本
const SCHEMA_VERSION: i32 = 1;

/// 初始化数据库 schema（建表、索引、触发器）
pub fn init_schema(conn: &rusqlite::Connection) -> Result<()> {
    // 检查是否已初始化
    let current_version = get_schema_version(conn)?;

    if current_version >= SCHEMA_VERSION {
        return Ok(());
    }

    // 开启 WAL 模式，提升并发读写性能
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;

    // 创建 schema_version 表
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version     INTEGER PRIMARY KEY,
            applied_at  INTEGER NOT NULL DEFAULT (strftime('%s','now')),
            description TEXT
        );",
    )?;

    // 创建记忆主表
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS memories (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            tier             TEXT    NOT NULL CHECK(tier IN ('episodic', 'semantic', 'procedural')),
            source           TEXT    NOT NULL,
            content          TEXT    NOT NULL,
            metadata         TEXT    NOT NULL DEFAULT '{}',
            tags             TEXT    NOT NULL DEFAULT '[]',
            created_at       INTEGER NOT NULL DEFAULT (strftime('%s','now')),
            last_accessed_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
            updated_at       INTEGER NOT NULL DEFAULT (strftime('%s','now')),
            expires_at       INTEGER,
            distilled        INTEGER NOT NULL DEFAULT 0,
            confidence       REAL    NOT NULL DEFAULT 1.0,
            access_count     INTEGER NOT NULL DEFAULT 0
        );",
    )?;

    // 索引
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_memories_tier       ON memories(tier);
         CREATE INDEX IF NOT EXISTS idx_memories_source     ON memories(source);
         CREATE INDEX IF NOT EXISTS idx_memories_distilled  ON memories(distilled);
         CREATE INDEX IF NOT EXISTS idx_memories_expires    ON memories(expires_at) WHERE expires_at IS NOT NULL;
         CREATE INDEX IF NOT EXISTS idx_memories_created    ON memories(created_at);
         CREATE INDEX IF NOT EXISTS idx_memories_confidence ON memories(confidence);",
    )?;

    // FTS5 全文检索虚拟表
    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
            content,
            source,
            tags,
            content='memories',
            content_rowid='id',
            tokenize='unicode61'
        );",
    )?;

    // FTS5 同步触发器
    conn.execute_batch(
        "CREATE TRIGGER IF NOT EXISTS memories_fts_insert AFTER INSERT ON memories BEGIN
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
        END;",
    )?;

    // 实体表
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS entities (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT    NOT NULL UNIQUE,
            entity_type TEXT    NOT NULL DEFAULT 'generic',
            properties  TEXT    NOT NULL DEFAULT '{}',
            created_at  INTEGER NOT NULL DEFAULT (strftime('%s','now')),
            updated_at  INTEGER NOT NULL DEFAULT (strftime('%s','now'))
        );
        CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(entity_type);",
    )?;

    // 关系表
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS relationships (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            source_id     INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
            target_id     INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
            relation_type TEXT    NOT NULL,
            properties    TEXT    NOT NULL DEFAULT '{}',
            created_at    INTEGER NOT NULL DEFAULT (strftime('%s','now'))
        );
        CREATE INDEX IF NOT EXISTS idx_rel_source ON relationships(source_id);
        CREATE INDEX IF NOT EXISTS idx_rel_target ON relationships(target_id);
        CREATE INDEX IF NOT EXISTS idx_rel_type   ON relationships(relation_type);",
    )?;

    // 记录 schema 版本
    conn.execute(
        "INSERT OR REPLACE INTO schema_version (version, description) VALUES (?1, ?2)",
        rusqlite::params![SCHEMA_VERSION, "initial schema"],
    )?;

    Ok(())
}

/// 获取当前 schema 版本号
fn get_schema_version(conn: &rusqlite::Connection) -> Result<i32> {
    // 表可能尚不存在
    let table_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_version'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)?;

    if !table_exists {
        return Ok(0);
    }

    let version: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_schema_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_schema(&conn).unwrap(); // 第二次应该是 no-op
    }

    #[test]
    fn test_schema_version_recorded() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        let version: i32 = conn
            .query_row(
                "SELECT MAX(version) FROM schema_version",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }
}
