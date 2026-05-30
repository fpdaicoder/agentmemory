use agentmemory::*;

// ---------------------------------------------------------------------------
// 基础 CRUD
// ---------------------------------------------------------------------------

#[test]
fn test_store_and_get() {
    let mem = AgentMemory::open_in_memory().unwrap();

    let stored = mem
        .store(
            StoreRequest::new(MemoryTier::Episodic, "user:alice", "Alice likes PostgreSQL")
                .with_tags(vec!["database".into(), "preference".into()])
                .with_confidence(0.9),
        )
        .unwrap();

    assert!(stored.id > 0);
    assert_eq!(stored.tier, MemoryTier::Episodic);
    assert_eq!(stored.source, "user:alice");
    assert_eq!(stored.content, "Alice likes PostgreSQL");
    assert_eq!(stored.tags, vec!["database", "preference"]);
    assert!((stored.confidence - 0.9).abs() < f64::EPSILON);
    // store() 内部调用 get() 返回结果，所以 access_count 已经是 1
    assert_eq!(stored.access_count, 1);
    assert!(!stored.distilled);

    // 再 get 应该增加 access_count
    let got = mem.get(stored.id).unwrap();
    assert_eq!(got.id, stored.id);
    assert_eq!(got.access_count, 2);

    // 再 get 一次
    let got2 = mem.get(stored.id).unwrap();
    assert_eq!(got2.access_count, 3);
}

#[test]
fn test_store_validation_empty_content() {
    let mem = AgentMemory::open_in_memory().unwrap();
    let result = mem.store(StoreRequest::new(MemoryTier::Semantic, "test", ""));
    assert!(result.is_err());
    match result.unwrap_err() {
        MemoryError::Validation(msg) => assert!(msg.contains("content")),
        _ => panic!("Expected Validation error"),
    }
}

#[test]
fn test_store_validation_empty_source() {
    let mem = AgentMemory::open_in_memory().unwrap();
    let result = mem.store(StoreRequest::new(MemoryTier::Semantic, "", "some content"));
    assert!(result.is_err());
}

#[test]
fn test_get_not_found() {
    let mem = AgentMemory::open_in_memory().unwrap();
    let result = mem.get(99999);
    assert!(matches!(result, Err(MemoryError::NotFound { id: 99999 })));
}

#[test]
fn test_update() {
    let mem = AgentMemory::open_in_memory().unwrap();

    let stored = mem
        .store(StoreRequest::new(MemoryTier::Semantic, "test", "original content"))
        .unwrap();

    let updated = mem
        .update(
            stored.id,
            "updated content",
            Some(serde_json::json!({"key": "value"})),
        )
        .unwrap();

    assert_eq!(updated.content, "updated content");
    assert_eq!(updated.metadata["key"], "value");
    assert!(updated.updated_at >= stored.updated_at);
}

#[test]
fn test_update_not_found() {
    let mem = AgentMemory::open_in_memory().unwrap();
    let result = mem.update(99999, "new content", None);
    assert!(matches!(result, Err(MemoryError::NotFound { id: 99999 })));
}

#[test]
fn test_add_tags() {
    let mem = AgentMemory::open_in_memory().unwrap();

    let stored = mem
        .store(StoreRequest::new(MemoryTier::Episodic, "test", "content"))
        .unwrap();

    mem.add_tags(stored.id, &["tag1".into(), "tag2".into()])
        .unwrap();

    let got = mem.get(stored.id).unwrap();
    assert!(got.tags.contains(&"tag1".to_string()));
    assert!(got.tags.contains(&"tag2".to_string()));

    // 重复添加不应该重复
    mem.add_tags(stored.id, &["tag1".into()]).unwrap();
    let got2 = mem.get(stored.id).unwrap();
    assert_eq!(got2.tags.iter().filter(|t| **t == "tag1").count(), 1);
}

#[test]
fn test_delete() {
    let mem = AgentMemory::open_in_memory().unwrap();

    let stored = mem
        .store(StoreRequest::new(MemoryTier::Episodic, "test", "to be deleted"))
        .unwrap();

    mem.delete(stored.id).unwrap();
    let result = mem.get(stored.id);
    assert!(result.is_err());
}

#[test]
fn test_delete_not_found() {
    let mem = AgentMemory::open_in_memory().unwrap();
    let result = mem.delete(99999);
    assert!(matches!(result, Err(MemoryError::NotFound { id: 99999 })));
}

#[test]
fn test_delete_batch() {
    let mem = AgentMemory::open_in_memory().unwrap();

    let m1 = mem
        .store(StoreRequest::new(MemoryTier::Episodic, "test", "m1"))
        .unwrap();
    let m2 = mem
        .store(StoreRequest::new(MemoryTier::Episodic, "test", "m2"))
        .unwrap();
    let m3 = mem
        .store(StoreRequest::new(MemoryTier::Episodic, "test", "m3"))
        .unwrap();

    mem.delete_batch(&[m1.id, m3.id]).unwrap();

    assert!(mem.get(m1.id).is_err());
    assert!(mem.get(m2.id).is_ok()); // m2 还在
    assert!(mem.get(m3.id).is_err());
}

// ---------------------------------------------------------------------------
// 批量写入
// ---------------------------------------------------------------------------

#[test]
fn test_store_batch() {
    let mem = AgentMemory::open_in_memory().unwrap();

    let requests = vec![
        StoreRequest::new(MemoryTier::Episodic, "test", "batch 1"),
        StoreRequest::new(MemoryTier::Semantic, "test", "batch 2"),
        StoreRequest::new(MemoryTier::Procedural, "test", "batch 3"),
    ];

    let results = mem.store_batch(requests).unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].tier, MemoryTier::Episodic);
    assert_eq!(results[1].tier, MemoryTier::Semantic);
    assert_eq!(results[2].tier, MemoryTier::Procedural);
}

// ---------------------------------------------------------------------------
// 列表和过滤
// ---------------------------------------------------------------------------

#[test]
fn test_list_all() {
    let mem = AgentMemory::open_in_memory().unwrap();

    mem.store(StoreRequest::new(MemoryTier::Episodic, "s1", "c1"))
        .unwrap();
    mem.store(StoreRequest::new(MemoryTier::Semantic, "s2", "c2"))
        .unwrap();
    mem.store(StoreRequest::new(MemoryTier::Procedural, "s3", "c3"))
        .unwrap();

    let all = mem.list(&MemoryFilter::new()).unwrap();
    assert_eq!(all.len(), 3);
}

#[test]
fn test_list_filter_by_tier() {
    let mem = AgentMemory::open_in_memory().unwrap();

    mem.store(StoreRequest::new(MemoryTier::Episodic, "s1", "c1"))
        .unwrap();
    mem.store(StoreRequest::new(MemoryTier::Semantic, "s2", "c2"))
        .unwrap();
    mem.store(StoreRequest::new(MemoryTier::Procedural, "s3", "c3"))
        .unwrap();

    let episodic = mem
        .list(&MemoryFilter::new().with_tier(MemoryTier::Episodic))
        .unwrap();
    assert_eq!(episodic.len(), 1);
    assert_eq!(episodic[0].tier, MemoryTier::Episodic);
}

#[test]
fn test_list_filter_by_source() {
    let mem = AgentMemory::open_in_memory().unwrap();

    mem.store(StoreRequest::new(MemoryTier::Episodic, "alice", "c1"))
        .unwrap();
    mem.store(StoreRequest::new(MemoryTier::Episodic, "bob", "c2"))
        .unwrap();

    let alice = mem
        .list(&MemoryFilter::new().with_source("alice"))
        .unwrap();
    assert_eq!(alice.len(), 1);
    assert_eq!(alice[0].source, "alice");
}

#[test]
fn test_list_filter_by_tags() {
    let mem = AgentMemory::open_in_memory().unwrap();

    mem.store(
        StoreRequest::new(MemoryTier::Episodic, "test", "c1")
            .with_tags(vec!["rust".into(), "database".into()]),
    )
    .unwrap();
    mem.store(
        StoreRequest::new(MemoryTier::Episodic, "test", "c2")
            .with_tags(vec!["python".into()]),
    )
    .unwrap();

    let rust = mem
        .list(&MemoryFilter::new().with_tags(vec!["rust".into()]))
        .unwrap();
    assert_eq!(rust.len(), 1);
    assert_eq!(rust[0].content, "c1");
}

#[test]
fn test_list_pagination() {
    let mem = AgentMemory::open_in_memory().unwrap();

    for i in 0..10 {
        mem.store(StoreRequest::new(MemoryTier::Episodic, "test", format!("item {}", i)))
            .unwrap();
    }

    let page1 = mem
        .list(&MemoryFilter::new().with_limit(3).with_offset(0))
        .unwrap();
    let page2 = mem
        .list(&MemoryFilter::new().with_limit(3).with_offset(3))
        .unwrap();

    assert_eq!(page1.len(), 3);
    assert_eq!(page2.len(), 3);
    assert_ne!(page1[0].id, page2[0].id);
}

#[test]
fn test_count() {
    let mem = AgentMemory::open_in_memory().unwrap();

    mem.store(StoreRequest::new(MemoryTier::Episodic, "test", "c1"))
        .unwrap();
    mem.store(StoreRequest::new(MemoryTier::Semantic, "test", "c2"))
        .unwrap();

    let total = mem.count(&MemoryFilter::new()).unwrap();
    assert_eq!(total, 2);

    let episodic = mem
        .count(&MemoryFilter::new().with_tier(MemoryTier::Episodic))
        .unwrap();
    assert_eq!(episodic, 1);
}

// ---------------------------------------------------------------------------
// 全文检索
// ---------------------------------------------------------------------------

#[test]
fn test_search_basic() {
    let mem = AgentMemory::open_in_memory().unwrap();

    mem.store(StoreRequest::new(
        MemoryTier::Episodic,
        "user:alice",
        "Alice prefers PostgreSQL over MySQL for new projects",
    ))
    .unwrap();
    mem.store(StoreRequest::new(
        MemoryTier::Episodic,
        "user:bob",
        "Bob is working on a React frontend application",
    ))
    .unwrap();
    mem.store(StoreRequest::new(
        MemoryTier::Semantic,
        "system",
        "The project uses a PostgreSQL database with UTF-8 encoding",
    ))
    .unwrap();

    let results = mem
        .search("PostgreSQL", SearchOptions::new().with_limit(5))
        .unwrap();

    assert!(!results.is_empty());
    // 应该找到两条包含 PostgreSQL 的记忆
    assert!(results.len() >= 2);
}

#[test]
fn test_search_with_tier_filter() {
    let mem = AgentMemory::open_in_memory().unwrap();

    mem.store(StoreRequest::new(
        MemoryTier::Episodic,
        "test",
        "database design review meeting notes",
    ))
    .unwrap();
    mem.store(StoreRequest::new(
        MemoryTier::Semantic,
        "test",
        "PostgreSQL is the chosen database system",
    ))
    .unwrap();

    let episodic = mem
        .search(
            "database",
            SearchOptions::new().with_tier(MemoryTier::Episodic),
        )
        .unwrap();

    for scored in &episodic {
        assert_eq!(scored.memory.tier, MemoryTier::Episodic);
    }
}

#[test]
fn test_search_with_min_confidence() {
    let mem = AgentMemory::open_in_memory().unwrap();

    mem.store(
        StoreRequest::new(MemoryTier::Semantic, "test", "high confidence fact")
            .with_confidence(0.95),
    )
    .unwrap();
    mem.store(
        StoreRequest::new(MemoryTier::Semantic, "test", "low confidence fact")
            .with_confidence(0.3),
    )
    .unwrap();

    let high = mem
        .search("fact", SearchOptions::new().with_min_confidence(0.8))
        .unwrap();

    for scored in &high {
        assert!(scored.memory.confidence >= 0.8);
    }
}

#[test]
fn test_search_updates_access_count() {
    let mem = AgentMemory::open_in_memory().unwrap();

    let stored = mem
        .store(StoreRequest::new(
            MemoryTier::Episodic,
            "test",
            "searchable content about Rust programming",
        ))
        .unwrap();

    // store() 内部调用了 get()，所以 access_count 已经是 1
    assert_eq!(stored.access_count, 1);

    let results = mem.search("Rust", SearchOptions::new()).unwrap();
    assert!(!results.is_empty());

    // search 会增加 access_count，get 再增加一次
    let got = mem.get(stored.id).unwrap();
    assert!(got.access_count >= 3); // store: +1, search: +1, get: +1
}

#[test]
fn test_search_by_entity() {
    let mem = AgentMemory::open_in_memory().unwrap();

    mem.store(
        StoreRequest::new(MemoryTier::Episodic, "test", "Alice works on the project")
            .with_tags(vec!["alice".into()]),
    )
    .unwrap();
    mem.store(
        StoreRequest::new(MemoryTier::Episodic, "test", "Bob manages the team")
            .with_tags(vec!["bob".into()]),
    )
    .unwrap();

    let alice = mem
        .search_by_entity("alice", SearchOptions::new())
        .unwrap();
    assert_eq!(alice.len(), 1);
}

// ---------------------------------------------------------------------------
// 生命周期
// ---------------------------------------------------------------------------

#[test]
fn test_cleanup_expired() {
    let mem = AgentMemory::open_in_memory().unwrap();

    let now = chrono::Utc::now().timestamp();

    // 创建一条已经过期的记忆
    mem.store(
        StoreRequest::new(MemoryTier::Episodic, "test", "expired memory")
            .with_expires_at(now - 3600), // 1 小时前过期
    )
    .unwrap();

    // 创建一条未过期的记忆
    mem.store(
        StoreRequest::new(MemoryTier::Episodic, "test", "valid memory")
            .with_expires_at(now + 360000), // 很久之后才过期
    )
    .unwrap();

    // 创建一条永不过期的记忆
    mem.store(StoreRequest::new(MemoryTier::Procedural, "test", "permanent rule"))
        .unwrap();

    let cleanup = mem.cleanup_expired().unwrap();
    assert_eq!(cleanup.deleted_count, 1);

    let remaining = mem.list(&MemoryFilter::new()).unwrap();
    assert_eq!(remaining.len(), 2);
}

#[test]
fn test_distillable_episodic() {
    let mem = AgentMemory::open_in_memory().unwrap();

    // 创建一条老的 Episodic 记忆
    let old = mem
        .store(StoreRequest::new(MemoryTier::Episodic, "test", "old episodic"))
        .unwrap();

    // 创建一条 Semantic 记忆（不应该出现）
    mem.store(StoreRequest::new(MemoryTier::Semantic, "test", "semantic fact"))
        .unwrap();

    // min_age_secs = -1 确保 created_at < cutoff 成立（刚创建的记录）
    // 使用负值让 cutoff > now，使所有记忆都满足条件
    let distillable = mem.get_distillable_episodic(-1).unwrap();
    assert_eq!(distillable.len(), 1);
    assert_eq!(distillable[0].id, old.id);

    // 标记为已蒸馏
    mem.mark_distilled(&[old.id]).unwrap();

    let distillable2 = mem.get_distillable_episodic(0).unwrap();
    assert_eq!(distillable2.len(), 0); // 已被标记，不再出现
}

#[test]
fn test_health_report() {
    let mem = AgentMemory::open_in_memory().unwrap();

    mem.store(StoreRequest::new(MemoryTier::Episodic, "test", "episodic 1"))
        .unwrap();
    mem.store(StoreRequest::new(MemoryTier::Episodic, "test", "episodic 2"))
        .unwrap();
    mem.store(StoreRequest::new(MemoryTier::Semantic, "test", "semantic 1"))
        .unwrap();
    mem.store(StoreRequest::new(MemoryTier::Procedural, "test", "procedural 1"))
        .unwrap();

    let health = mem.health().unwrap();

    assert_eq!(health.total_memories, 4);
    assert_eq!(health.by_tier.get("episodic"), Some(&2));
    assert_eq!(health.by_tier.get("semantic"), Some(&1));
    assert_eq!(health.by_tier.get("procedural"), Some(&1));
    assert!(health.avg_confidence > 0.0);
}

// ---------------------------------------------------------------------------
// 实体和关系
// ---------------------------------------------------------------------------

#[test]
fn test_entities_and_relationships() {
    let mem = AgentMemory::open_in_memory().unwrap();

    mem.store(
        StoreRequest::new(MemoryTier::Episodic, "test", "Alice works on ProjectX")
            .with_entities(vec!["alice".into(), "projectx".into()]),
    )
    .unwrap();

    let entities = mem.list_entities().unwrap();
    assert!(entities.len() >= 2);

    let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"alice"));
    assert!(names.contains(&"projectx"));

    // 添加关系
    mem.add_relationship("alice", "projectx", "works_on")
        .unwrap();

    // 验证关系存在（通过 search_by_entity 间接验证）
    let results = mem.search_by_entity("alice", SearchOptions::new()).unwrap();
    assert!(!results.is_empty());
}

// ---------------------------------------------------------------------------
// 三种记忆层级
// ---------------------------------------------------------------------------

#[test]
fn test_all_tiers() {
    let mem = AgentMemory::open_in_memory().unwrap();

    let epi = mem
        .store(StoreRequest::new(
            MemoryTier::Episodic,
            "session:1",
            "User asked about database design",
        ))
        .unwrap();
    assert_eq!(epi.tier, MemoryTier::Episodic);

    let sem = mem
        .store(StoreRequest::new(
            MemoryTier::Semantic,
            "llm:summary",
            "The project uses SQLite for storage",
        ))
        .unwrap();
    assert_eq!(sem.tier, MemoryTier::Semantic);

    let proc = mem
        .store(StoreRequest::new(
            MemoryTier::Procedural,
            "system",
            "Always use UTF-8 encoding",
        ))
        .unwrap();
    assert_eq!(proc.tier, MemoryTier::Procedural);

    // 验证过滤
    let procedural = mem
        .list(&MemoryFilter::new().with_tier(MemoryTier::Procedural))
        .unwrap();
    assert_eq!(procedural.len(), 1);
    assert_eq!(procedural[0].content, "Always use UTF-8 encoding");
}

// ---------------------------------------------------------------------------
// 元数据
// ---------------------------------------------------------------------------

#[test]
fn test_metadata() {
    let mem = AgentMemory::open_in_memory().unwrap();

    let stored = mem
        .store(
            StoreRequest::new(MemoryTier::Semantic, "test", "test content")
                .with_metadata(serde_json::json!({
                    "session_id": "abc123",
                    "topic": "database",
                    "importance": 0.8
                })),
        )
        .unwrap();

    assert_eq!(stored.metadata["session_id"], "abc123");
    assert_eq!(stored.metadata["topic"], "database");
    assert!((stored.metadata["importance"].as_f64().unwrap() - 0.8).abs() < f64::EPSILON);
}
