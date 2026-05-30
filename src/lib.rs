//! # AgentMemory
//!
//! A lightweight embedded memory system for AI agents powered by SQLite.
//!
//! ## Quick Start
//!
//! ```no_run
//! use agentmemory::{AgentMemory, MemoryTier, StoreRequest, SearchOptions};
//!
//! // Open a persistent database
//! let mem = AgentMemory::open("agent_memory.db").unwrap();
//!
//! // Store an episodic memory
//! mem.store(StoreRequest::new(
//!     MemoryTier::Episodic,
//!     "user:alice",
//!     "Alice prefers PostgreSQL over MySQL for new projects.",
//! )).unwrap();
//!
//! // Search memories
//! let results = mem.search("database preference", SearchOptions::new()).unwrap();
//! for scored in &results {
//!     println!("[{:.2}] {}", scored.score, scored.memory.content);
//! }
//! ```

mod error;
mod lifecycle;
pub mod models;
mod schema;
mod search;
mod store;

// Re-export public API
pub use error::{MemoryError, Result};
pub use lifecycle::{CleanupResult, DistillResult, HealthReport};
pub use models::*;
pub use store::AgentMemory;
