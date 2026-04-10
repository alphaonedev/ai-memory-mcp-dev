// Copyright (c) 2026 AlphaOne LLC. All rights reserved.
// Licensed under the MIT License. See LICENSE file in the project root.

//! Storage backend abstraction layer.
//!
//! `StorageBackend` defines the contract that every data store must satisfy.
//! `SqliteBackend` is the default (and currently only) implementation,
//! delegating to the battle-tested `db` module.
//!
//! A `BackendRegistry` maps backend names (e.g. `"sqlite"`) to factory
//! functions, wired into the CLI via `--backend <name>`.

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::db;
use crate::hnsw::VectorIndex;
use crate::models::*;

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Unified storage interface.
///
/// Every method mirrors a public function in `db.rs`.  Future backends
/// (PostgreSQL + pgvector, Qdrant, etc.) implement the same surface.
#[allow(clippy::too_many_arguments)]
/// Callers needing thread-safety wrap the backend in `Mutex<Box<dyn StorageBackend>>`.
/// `Sync` is intentionally omitted: `rusqlite::Connection` is `Send` but not `Sync`.
pub trait StorageBackend: Send {
    // -- CRUD ---------------------------------------------------------------

    /// Insert with upsert on title+namespace.  Returns the (possibly existing) ID.
    fn insert(&self, mem: &Memory) -> Result<String>;

    /// Retrieve a single memory by ID.
    fn get(&self, id: &str) -> Result<Option<Memory>>;

    /// Partial update — only provided fields are changed.
    fn update(
        &self,
        id: &str,
        title: Option<&str>,
        content: Option<&str>,
        tier: Option<&Tier>,
        namespace: Option<&str>,
        tags: Option<&Vec<String>>,
        priority: Option<i32>,
        confidence: Option<f64>,
        expires_at: Option<&str>,
    ) -> Result<bool>;

    /// Delete a memory by ID.  Returns `true` if it existed.
    fn delete(&self, id: &str) -> Result<bool>;

    // -- Recall & Search ----------------------------------------------------

    /// Fuzzy OR search + touch + auto-promote.
    fn recall(
        &self,
        context: &str,
        namespace: Option<&str>,
        limit: usize,
        tags_filter: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<Vec<(Memory, f64)>>;

    /// Exact keyword search (AND semantics via FTS5).
    fn search(
        &self,
        query: &str,
        namespace: Option<&str>,
        tier: Option<&Tier>,
        limit: usize,
        min_priority: Option<i32>,
        since: Option<&str>,
        until: Option<&str>,
        tags_filter: Option<&str>,
    ) -> Result<Vec<Memory>>;

    /// List memories with optional filters.
    fn list(
        &self,
        namespace: Option<&str>,
        tier: Option<&Tier>,
        limit: usize,
        offset: usize,
        min_priority: Option<i32>,
        since: Option<&str>,
        until: Option<&str>,
        tags_filter: Option<&str>,
    ) -> Result<Vec<Memory>>;

    /// Hybrid FTS + semantic recall.
    fn recall_hybrid(
        &self,
        context: &str,
        query_embedding: &[f32],
        namespace: Option<&str>,
        limit: usize,
        tags_filter: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
        vector_index: Option<&VectorIndex>,
    ) -> Result<Vec<(Memory, f64)>>;

    // -- Links --------------------------------------------------------------

    fn create_link(&self, source_id: &str, target_id: &str, relation: &str) -> Result<()>;
    fn get_links(&self, id: &str) -> Result<Vec<MemoryLink>>;
    fn delete_link(&self, source_id: &str, target_id: &str) -> Result<bool>;

    // -- Lifecycle ----------------------------------------------------------

    /// Bump access count, extend TTL, auto-promote.
    fn touch(&self, id: &str) -> Result<()>;

    /// Bulk delete by pattern/namespace/tier.
    fn forget(
        &self,
        namespace: Option<&str>,
        pattern: Option<&str>,
        tier: Option<&Tier>,
    ) -> Result<usize>;

    /// Garbage-collect expired memories.
    fn gc(&self) -> Result<usize>;

    /// Lightweight GC — only runs if expired rows exist.
    fn gc_if_needed(&self) -> Result<usize>;

    // -- Consolidation ------------------------------------------------------

    /// Merge N memories into one long-term summary.
    fn consolidate(
        &self,
        ids: &[String],
        title: &str,
        summary: &str,
        namespace: &str,
        tier: &Tier,
        source: &str,
    ) -> Result<String>;

    // -- Contradiction detection --------------------------------------------

    fn find_contradictions(&self, title: &str, namespace: &str) -> Result<Vec<Memory>>;

    // -- Export / Import / Sync ---------------------------------------------

    fn export_all(&self) -> Result<Vec<Memory>>;
    fn export_links(&self) -> Result<Vec<MemoryLink>>;

    /// Timestamp-aware upsert for sync.
    fn insert_if_newer(&self, mem: &Memory) -> Result<String>;

    // -- Embeddings ---------------------------------------------------------

    fn set_embedding(&self, id: &str, embedding: &[f32]) -> Result<()>;
    fn get_embedding(&self, id: &str) -> Result<Option<Vec<f32>>>;
    fn get_unembedded_ids(&self) -> Result<Vec<(String, String, String)>>;
    fn get_all_embeddings(&self) -> Result<Vec<(String, Vec<f32>)>>;

    // -- Metadata -----------------------------------------------------------

    fn stats(&self) -> Result<Stats>;
    fn list_namespaces(&self) -> Result<Vec<NamespaceCount>>;
    fn health_check(&self) -> Result<bool>;
    fn checkpoint(&self) -> Result<()>;
}

// ---------------------------------------------------------------------------
// SqliteBackend
// ---------------------------------------------------------------------------

/// Default backend — wraps a `rusqlite::Connection` and delegates to `db::*`.
pub struct SqliteBackend {
    conn: rusqlite::Connection,
    db_path: PathBuf,
}

impl SqliteBackend {
    /// Open (or create) a SQLite database at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = db::open(path)?;
        Ok(Self {
            conn,
            db_path: path.to_path_buf(),
        })
    }

    /// Borrow the raw connection (escape hatch for callers that still need it).
    pub fn conn(&self) -> &rusqlite::Connection {
        &self.conn
    }

    /// Borrow the database path.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }
}

#[allow(clippy::too_many_arguments)]
impl StorageBackend for SqliteBackend {
    fn insert(&self, mem: &Memory) -> Result<String> {
        db::insert(&self.conn, mem)
    }

    fn get(&self, id: &str) -> Result<Option<Memory>> {
        db::get(&self.conn, id)
    }

    fn update(
        &self,
        id: &str,
        title: Option<&str>,
        content: Option<&str>,
        tier: Option<&Tier>,
        namespace: Option<&str>,
        tags: Option<&Vec<String>>,
        priority: Option<i32>,
        confidence: Option<f64>,
        expires_at: Option<&str>,
    ) -> Result<bool> {
        db::update(
            &self.conn, id, title, content, tier, namespace, tags, priority, confidence, expires_at,
        )
    }

    fn delete(&self, id: &str) -> Result<bool> {
        db::delete(&self.conn, id)
    }

    fn recall(
        &self,
        context: &str,
        namespace: Option<&str>,
        limit: usize,
        tags_filter: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<Vec<(Memory, f64)>> {
        db::recall(&self.conn, context, namespace, limit, tags_filter, since, until)
    }

    fn search(
        &self,
        query: &str,
        namespace: Option<&str>,
        tier: Option<&Tier>,
        limit: usize,
        min_priority: Option<i32>,
        since: Option<&str>,
        until: Option<&str>,
        tags_filter: Option<&str>,
    ) -> Result<Vec<Memory>> {
        db::search(
            &self.conn,
            query,
            namespace,
            tier,
            limit,
            min_priority,
            since,
            until,
            tags_filter,
        )
    }

    fn list(
        &self,
        namespace: Option<&str>,
        tier: Option<&Tier>,
        limit: usize,
        offset: usize,
        min_priority: Option<i32>,
        since: Option<&str>,
        until: Option<&str>,
        tags_filter: Option<&str>,
    ) -> Result<Vec<Memory>> {
        db::list(
            &self.conn,
            namespace,
            tier,
            limit,
            offset,
            min_priority,
            since,
            until,
            tags_filter,
        )
    }

    fn recall_hybrid(
        &self,
        context: &str,
        query_embedding: &[f32],
        namespace: Option<&str>,
        limit: usize,
        tags_filter: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
        vector_index: Option<&VectorIndex>,
    ) -> Result<Vec<(Memory, f64)>> {
        db::recall_hybrid(
            &self.conn,
            context,
            query_embedding,
            namespace,
            limit,
            tags_filter,
            since,
            until,
            vector_index,
        )
    }

    fn create_link(&self, source_id: &str, target_id: &str, relation: &str) -> Result<()> {
        db::create_link(&self.conn, source_id, target_id, relation)
    }

    fn get_links(&self, id: &str) -> Result<Vec<MemoryLink>> {
        db::get_links(&self.conn, id)
    }

    fn delete_link(&self, source_id: &str, target_id: &str) -> Result<bool> {
        db::delete_link(&self.conn, source_id, target_id)
    }

    fn touch(&self, id: &str) -> Result<()> {
        db::touch(&self.conn, id)
    }

    fn forget(
        &self,
        namespace: Option<&str>,
        pattern: Option<&str>,
        tier: Option<&Tier>,
    ) -> Result<usize> {
        db::forget(&self.conn, namespace, pattern, tier)
    }

    fn gc(&self) -> Result<usize> {
        db::gc(&self.conn)
    }

    fn gc_if_needed(&self) -> Result<usize> {
        db::gc_if_needed(&self.conn)
    }

    fn consolidate(
        &self,
        ids: &[String],
        title: &str,
        summary: &str,
        namespace: &str,
        tier: &Tier,
        source: &str,
    ) -> Result<String> {
        db::consolidate(&self.conn, ids, title, summary, namespace, tier, source)
    }

    fn find_contradictions(&self, title: &str, namespace: &str) -> Result<Vec<Memory>> {
        db::find_contradictions(&self.conn, title, namespace)
    }

    fn export_all(&self) -> Result<Vec<Memory>> {
        db::export_all(&self.conn)
    }

    fn export_links(&self) -> Result<Vec<MemoryLink>> {
        db::export_links(&self.conn)
    }

    fn insert_if_newer(&self, mem: &Memory) -> Result<String> {
        db::insert_if_newer(&self.conn, mem)
    }

    fn set_embedding(&self, id: &str, embedding: &[f32]) -> Result<()> {
        db::set_embedding(&self.conn, id, embedding)
    }

    fn get_embedding(&self, id: &str) -> Result<Option<Vec<f32>>> {
        db::get_embedding(&self.conn, id)
    }

    fn get_unembedded_ids(&self) -> Result<Vec<(String, String, String)>> {
        db::get_unembedded_ids(&self.conn)
    }

    fn get_all_embeddings(&self) -> Result<Vec<(String, Vec<f32>)>> {
        db::get_all_embeddings(&self.conn)
    }

    fn stats(&self) -> Result<Stats> {
        db::stats(&self.conn, &self.db_path)
    }

    fn list_namespaces(&self) -> Result<Vec<NamespaceCount>> {
        db::list_namespaces(&self.conn)
    }

    fn health_check(&self) -> Result<bool> {
        db::health_check(&self.conn)
    }

    fn checkpoint(&self) -> Result<()> {
        db::checkpoint(&self.conn)
    }
}

// ---------------------------------------------------------------------------
// BackendRegistry
// ---------------------------------------------------------------------------

type BackendFactory = Box<dyn Fn(&Path) -> Result<Box<dyn StorageBackend>> + Send>;

/// Maps backend names to factory functions.
pub struct BackendRegistry {
    factories: HashMap<String, BackendFactory>,
}

impl BackendRegistry {
    /// Create a registry pre-loaded with the built-in SQLite backend.
    pub fn new() -> Self {
        let mut reg = Self {
            factories: HashMap::new(),
        };
        reg.register("sqlite", |path| {
            Ok(Box::new(SqliteBackend::open(path)?))
        });
        reg
    }

    /// Register a new backend factory.
    pub fn register<F>(&mut self, name: &str, factory: F)
    where
        F: Fn(&Path) -> Result<Box<dyn StorageBackend>> + Send + 'static,
    {
        self.factories.insert(name.to_string(), Box::new(factory));
    }

    /// Instantiate a backend by name.
    pub fn create(&self, name: &str, path: &Path) -> Result<Box<dyn StorageBackend>> {
        let factory = self
            .factories
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("unknown backend: {name}"))?;
        factory(path)
    }

    /// List registered backend names.
    pub fn names(&self) -> Vec<&str> {
        self.factories.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for BackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_backend() -> SqliteBackend {
        SqliteBackend::open(Path::new(":memory:")).unwrap()
    }

    fn make_memory(title: &str, ns: &str, tier: Tier) -> Memory {
        let now = chrono::Utc::now().to_rfc3339();
        Memory {
            id: uuid::Uuid::new_v4().to_string(),
            tier: tier.clone(),
            namespace: ns.to_string(),
            title: title.to_string(),
            content: format!("Content for {title}"),
            tags: vec![],
            priority: 5,
            confidence: 1.0,
            source: "test".to_string(),
            access_count: 0,
            created_at: now.clone(),
            updated_at: now,
            last_accessed_at: None,
            expires_at: tier
                .default_ttl_secs()
                .map(|s| (chrono::Utc::now() + chrono::Duration::seconds(s)).to_rfc3339()),
        }
    }

    #[test]
    fn trait_insert_and_get() {
        let backend = test_backend();
        let mem = make_memory("Trait test", "test", Tier::Long);
        let id = backend.insert(&mem).unwrap();
        let got = backend.get(&id).unwrap().unwrap();
        assert_eq!(got.title, "Trait test");
    }

    #[test]
    fn trait_recall() {
        let backend = test_backend();
        backend
            .insert(&make_memory("Rust language", "test", Tier::Long))
            .unwrap();
        let results = backend
            .recall("Rust", None, 10, None, None, None)
            .unwrap();
        assert!(!results.is_empty());
        assert!(results[0].1 > 0.0);
    }

    #[test]
    fn trait_search() {
        let backend = test_backend();
        backend
            .insert(&make_memory("PostgreSQL config", "test", Tier::Long))
            .unwrap();
        let results = backend
            .search("PostgreSQL", None, None, 10, None, None, None, None)
            .unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn trait_stats() {
        let backend = test_backend();
        let s = backend.stats().unwrap();
        assert_eq!(s.total, 0);
    }

    #[test]
    fn trait_health_check() {
        let backend = test_backend();
        assert!(backend.health_check().unwrap());
    }

    #[test]
    fn registry_creates_sqlite() {
        let reg = BackendRegistry::new();
        assert!(reg.names().contains(&"sqlite"));
        let backend = reg.create("sqlite", Path::new(":memory:")).unwrap();
        assert!(backend.health_check().unwrap());
    }

    #[test]
    fn registry_unknown_backend_errors() {
        let reg = BackendRegistry::new();
        assert!(reg.create("postgres", Path::new(":memory:")).is_err());
    }

    #[test]
    fn trait_links() {
        let backend = test_backend();
        let id1 = backend
            .insert(&make_memory("Link A", "test", Tier::Long))
            .unwrap();
        let id2 = backend
            .insert(&make_memory("Link B", "test", Tier::Long))
            .unwrap();
        backend.create_link(&id1, &id2, "related_to").unwrap();
        let links = backend.get_links(&id1).unwrap();
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn trait_gc() {
        let backend = test_backend();
        let mut mem = make_memory("Expired", "test", Tier::Short);
        mem.expires_at = Some("2020-01-01T00:00:00+00:00".to_string());
        backend.insert(&mem).unwrap();
        let removed = backend.gc().unwrap();
        assert_eq!(removed, 1);
    }
}
