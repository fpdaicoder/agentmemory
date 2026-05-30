# AgentMemory

A lightweight embedded memory system for AI agents, written in Rust and powered by SQLite.

## Features

- **Three-tier memory model** — Episodic (raw events), Semantic (distilled facts), Procedural (rules & preferences)
- **Full-text search** — FTS5-powered search with hybrid ranking (BM25 + time decay + access frequency × confidence)
- **Lifecycle management** — Automatic expiration cleanup, distillation marking, health monitoring
- **Knowledge graph** — Entity and relationship storage for structured knowledge
- **Embedded & lightweight** — Single-file SQLite database, no external services needed
- **Rust native** — Type-safe, zero-cost abstractions, `rusqlite` with bundled SQLite

## Quick Start

```rust
use agentmemory::{AgentMemory, MemoryTier, StoreRequest, SearchOptions};

fn main() -> anyhow::Result<()> {
    // Open a persistent database
    let mem = AgentMemory::open("agent_memory.db")?;

    // Store an episodic memory
    mem.store(StoreRequest::new(
        MemoryTier::Episodic,
        "user:alice",
        "Alice prefers PostgreSQL over MySQL for new projects.",
    ).with_tags(vec!["database".into(), "preference".into()])
     .with_confidence(0.9))?;

    // Store a procedural rule
    mem.store(StoreRequest::new(
        MemoryTier::Procedural,
        "system",
        "Always use UTF-8 encoding for user input.",
    ))?;

    // Search memories
    let results = mem.search("database preference", SearchOptions::new().with_limit(5))?;
    for scored in &results {
        println!("[{:.2}] {}", scored.score, scored.memory.content);
    }

    // Lifecycle management
    let cleanup = mem.cleanup_expired()?;
    println!("Cleaned up {} expired memories", cleanup.deleted_count);

    let health = mem.health()?;
    println!("Total memories: {}", health.total_memories);

    Ok(())
}
```

## Memory Tiers

| Tier | Description | Default TTL |
|------|-------------|-------------|
| **Episodic** | Raw conversation logs, event records | 7 days |
| **Semantic** | Distilled facts and knowledge | 90 days |
| **Procedural** | Rules, preferences, workflows | Permanent |

## Project Structure

```
src/
├── lib.rs              # Library entry point & public API re-exports
├── error.rs            # Unified error type (MemoryError)
├── schema.rs           # SQLite schema init, FTS5, triggers, migrations
├── store.rs            # AgentMemory struct (CRUD + entity operations)
├── search.rs           # FTS5 full-text search + hybrid ranking
├── lifecycle.rs        # Expiration cleanup, distillation, health reports
└── models/
    ├── memory.rs       # Memory, MemoryTier, StoreRequest, Filter, etc.
    ├── entity.rs       # Entity, EntityType
    └── relationship.rs # Relationship
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| `rusqlite` (bundled) | SQLite access |
| `serde` + `serde_json` | JSON serialization for metadata/tags |
| `thiserror` | Error type derivation |
| `chrono` | Timestamp handling |

## Building & Testing

```bash
cargo build
cargo test
```

## License

MIT
