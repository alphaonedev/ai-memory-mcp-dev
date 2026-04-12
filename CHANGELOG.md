# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.4] — 2026-04-12

### Fixed (Critical — Database Path Resolution)

- **Default `--db` path changed from relative `ai-memory.db` to absolute XDG-compliant path** — the relative default caused silent database fragmentation across working directories, manifesting as total memory loss when invoking `ai-memory` from different directories. The new default is `~/.local/share/ai-memory/ai-memory.db` (respects `$XDG_DATA_HOME` if set)
- **Tilde expansion (`~`) now works in `--db` flag and `config.toml` `db` key** — previously `db = "~/.claude/ai-memory.db"` in config.toml was treated as a literal path starting with `~`, not expanded to `$HOME`
- **Config template updated** — default config.toml now documents the XDG default path and includes warnings about relative path fragmentation risk
- **Relative path warning** — if a relative database path is resolved (via `--db` or config), a `tracing::warn!` is emitted alerting the user to fragmentation risk
- **Parent directory auto-creation** — database parent directories are created automatically with mode 0700 (Unix) to prevent information disclosure to other system users

### Added

- **`doctor` subcommand** — diagnoses database fragmentation and config issues:
  - Scans standard locations (`$HOME`, `~/.claude`, `~/.local/share/ai-memory`, CWD) plus user-supplied `--scan-dir` paths for stray database files
  - Reports all found databases with memory count, file size, and primary/stray status
  - Config diagnosis: effective path, absolute vs relative, config.toml status
  - `--fix` flag: automatically merges stray databases into the primary via the existing sync engine, then renames merged files to `*.merged-YYYYMMDD` to prevent re-processing
  - Full JSON output support (`--json`) for programmatic consumption
  - Concurrent access warning emitted when `--fix` is used
- 15 new unit tests for path resolution (tilde expansion, default path, effective_db priority, directory creation)
- 7 new integration tests for doctor subcommand and path resolution behavior

### Fixed (Red Team Phase 2 — 21 findings across 5 bug classes)

**Critical:**
- `cmd_mine` nested transaction crash — outer `BEGIN` conflicted with `db::insert`'s own `BEGIN IMMEDIATE`; refactored to use new `insert_no_tx` for batch operations (RT-21)

**High:**
- Stale embeddings after `memory_update` — updating title/content now invalidates the embedding so it gets re-embedded on next backfill (RT-07)
- Zero-width Unicode bypass — invisible chars (ZWS, ZWJ, BOM, bidi overrides) now stripped from title/content before storage, matching FTS5 query sanitization (RT-10)
- `create_link` opaque errors — now verifies both source and target memory IDs exist before inserting, returns clear "memory not found" error (RT-02)
- HNSW rebuild panic safety — `catch_unwind` wraps HNSW index rebuild; on panic, overflow buffer is preserved for linear scan recovery (RT-03)
- FTS5 hyphen handling — hyphens replaced with spaces instead of stripped, so "BIND9-custom" correctly matches as separate tokens (RT-20)

**Medium:**
- MCP store dedup now prevents tier/priority/confidence downgrade — matches SQL upsert `MAX()` semantics (RT-04)
- Comma-separated tags filter — `--tags "rust,python"` now correctly matches memories with either tag across list/search/recall/recall_hybrid (RT-24)
- Title validation uses char count instead of byte count — CJK titles no longer rejected at ~170 chars (RT-18)
- Title length checked on trimmed string — whitespace-padded titles no longer falsely rejected (RT-19)

**Low:**
- `export_all` now filters expired memories — prevents resurrection of dead memories on import (RT-16)

### Fixed (Red Team Audit — 42 findings)

**Critical fixes:**
- `consolidate()` ON DELETE CASCADE no longer destroys provenance links — provenance now stored in tags and content footer (RT-1)
- HNSW `partial_cmp().unwrap()` replaced with `unwrap_or(Equal)` — NaN distances no longer crash search (RT-2)
- HNSW Mutex poisoning recovery via `unwrap_or_else(|e| e.into_inner())` — no more permanent panic cascade (RT-3)
- MCP non-ASCII ID slice panic fixed with `is_char_boundary()` check (RT-5)
- `unsafe impl Send/Sync` on Embedder guarded with Device::Cpu assertion (RT-4)

**High-severity fixes:**
- `update()` wrapped in transaction to prevent TOCTOU (RT-6)
- `insert()`/`insert_if_newer()` wrapped in transaction for atomicity (RT-7)
- `handle_promote()` checks return value — no more false-positive success for nonexistent IDs (RT-8)
- `cosine_similarity()` returns 0.0 on NaN/Infinity inputs (RT-9)
- FTS sanitizer now strips zero-width Unicode chars (U+200B-200F, U+202A-202E, U+2066-2069, U+FEFF) (RT-10)
- Bad timestamps now get minimum recency score (near-zero) instead of maximum (RT-11)
- `delete_link()` now accepts optional relation filter (RT-12)
- `search()`/`recall()` log warnings on row deserialization failure instead of silently dropping (RT-13)

**Medium-severity fixes:**
- Limit capped at 10,000 in recall/search/list to prevent overflow (RT-14)
- MCP stdin reader rejects lines >1MB (RT-15)
- MCP embedding backfill capped at 100 per startup (RT-16)
- `cmd_mine` transaction properly rolled back on error (RT-17)
- `is_clean_string()` now rejects ASCII control characters (RT-18)
- Validation checks untrimmed length for titles/namespaces (RT-19)
- FTS sanitizer strips backslash (RT-20)
- FTS empty-query sentinel changed to unique `__aimemory_empty_query__` (RT-42)
- Invalid config logged via `tracing::warn!` instead of silent fallback (RT-23)
- `mine.rs` Claude message sort uses f64 keys instead of String (RT-24)

**Low-severity fixes:**
- stdin size limit (10MB) in cmd_import/cmd_store (RT-26)
- GC errors logged instead of silently swallowed (RT-30)
- `MemoryError` implements `std::error::Error` (RT-31)
- `mine.rs` truncate uses char count not byte count (RT-38)
- `mine.rs` JSONL parser skips bad lines instead of failing (RT-39)
- Reranker bigram matching now case-insensitive (RT-40)
- HNSW dimension mismatch returns max distance instead of silently truncating (RT-41)
- Backend registry warns on overwrite (RT-37)
- `SqliteBackend::conn()` marked with safety warning (RT-36)
- `validate_source()` trims before allowlist check (RT-28)
- `validate_id()` rejects whitespace (RT-29)

**Security fixes (full codebase review — 9 findings):**
- `--auth-token` / `AI_MEMORY_AUTH_TOKEN` Bearer token authentication for HTTP API (F1/F7/F8)
- CORS restricted to localhost origins only — prevents cross-origin exfiltration (F2)
- `unsafe impl Send/Sync` guarded with `Device::Cpu` runtime assertion (F3)
- SSRF warning logged for non-localhost Ollama/embed URLs at startup (F5)
- Docker default changed from `--host 0.0.0.0` to `--host 127.0.0.1` (F6)
- Sync command validates remote database schema before operating (F9)

### Added

- 33 new unit tests covering all 42 red-team findings and 9 security findings
- 7 earlier unit tests covering original 17 Phase 0 gaps
- Explicit benchmark timeouts: `measurement_time(30s)` + `sample_size(10)` on all Criterion groups

### Changed

- Test count: 161 → **258** (209 unit + 49 integration)
- CLI command count: 25 → **26** (added `doctor`)
- Updated test counts across all docs: README, CLAUDE.md, ROADMAP, DEVELOPER_GUIDE, ADMIN_GUIDE
- 14 source files modified, +656 lines (Phase 0 audit); 3 additional files for path fix

## [0.5.2] — 2026-04-08

### Added

- Ubuntu PPA: `sudo add-apt-repository ppa:jbridger2021/ai-memory && sudo apt install ai-memory`
- Fedora COPR: `sudo dnf copr enable alpha-one-ai/ai-memory && sudo dnf install ai-memory`
- CI workflows for automated PPA and COPR uploads on tag push
- debian/ packaging directory (control, rules, changelog, copyright)
- RPM spec file (ai-memory.spec) for COPR builds
- OpenClaw as 9th supported AI platform across all docs
- Animated architecture SVG and benchmark SVG in README
- Fedora/RHEL COPR and Ubuntu PPA install cards on GitHub Pages (8 install methods)

### Changed

- GitHub Pages professionalized: condensed hero, 13→7 nav links, 7→4 stats
- Install method count updated to 8 across all docs

## [0.5.1] — 2026-04-08

### Added

- Docker image auto-published to GitHub Container Registry (ghcr.io) on tag push
- `server.json` manifest for Official MCP Registry (modelcontextprotocol/registry)
- CONTRIBUTING.md, CHANGELOG.md, CODE_OF_CONDUCT.md
- Open Graph and Twitter Card meta tags on GitHub Pages
- Scope tables for all 9 AI platform tabs on GitHub Pages
- `mine` command documented across all docs (USER_GUIDE, ADMIN_GUIDE, DEVELOPER_GUIDE, index.html)
- Error code reference in DEVELOPER_GUIDE (NOT_FOUND, VALIDATION_FAILED, DATABASE_ERROR, CONFLICT)
- config.toml reference section in ADMIN_GUIDE
- Store command flags (`--source`, `--expires-at`, `--ttl-secs`) documented in README

### Changed

- Dockerfile: Rust 1.82 → 1.86, added build-essential, added benches/ copy
- Dockerfile: version label 0.4.0 → 0.5.0
- CI workflow: added Docker (GHCR) job triggered on tag push
- Claude Code MCP config: corrected from `~/.claude/.mcp.json` to three-scope model (`~/.claude.json`, `.mcp.json`, project-local)
- All 8 AI platform configs: added Windows paths, env var syntax, scope tables
- Hybrid recall blend weights: corrected docs from 50/50 & 85/15 to 60/40 (matches code)
- Default tier: corrected docs from "keyword" to "semantic" (matches code)
- Test count: corrected from 167 to 161 (118 unit + 43 integration)
- Module count: corrected from 14 to 15 (added mine.rs)
- CLI command count: corrected from 24 to 25 (added mine)

### Fixed

- Dockerfile build failure: missing benches/ directory, outdated Rust version, missing C++ compiler

## [0.5.0] — 2026-04-08

### Added

- MCP server with 17 tools for AI-native memory management
- HTTP API with 20 endpoints for external integration
- CLI with 25 commands for local operation and scripting
- 4 feature tiers (Core, Standard, Advanced, Enterprise) for flexible deployment
- TOON format for structured, topology-aware memory representation
- Hybrid recall engine combining semantic search, keyword matching, and graph traversal
- Multi-node sync for distributed memory across instances
- Auto-consolidation to merge and deduplicate related memories
- `mine` command for importing memories from conversation history
- LongMemEval benchmark support achieving 97.8% Recall@5

### Changed

- Upgraded memory storage layer for improved write throughput
- Refined relevance scoring in hybrid recall for better precision
- Improved CLI output formatting and error messages

### Fixed

- Resolved race condition during concurrent memory writes
- Fixed encoding issue with non-ASCII content in TOON format
- Corrected sync conflict resolution when timestamps are identical

## [0.4.0]

### Added

- Initial MCP server implementation with core tool set
- Basic memory storage and retrieval
- CLI foundation with essential commands
- Semantic search over stored memories
- SQLite-backed persistent storage

### Changed

- Migrated internal data model to support richer metadata

### Fixed

- Fixed crash on empty query input
- Resolved file descriptor leak in long-running server mode

## [0.3.0]

### Added

- Embedding-based semantic search
- Memory tagging and filtering
- Configuration file support

### Changed

- Switched to async I/O for server operations

### Fixed

- Fixed memory leak during large batch imports

## [0.2.0]

### Added

- Persistent storage backend
- Basic CLI for memory CRUD operations
- JSON export and import

### Fixed

- Fixed incorrect timestamp handling across time zones

## [0.1.0]

### Added

- Initial prototype with in-memory storage
- Core data model for memory entries
- Basic search functionality

[0.5.2]: https://github.com/alphaonedev/ai-memory-mcp/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/alphaonedev/ai-memory-mcp/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/alphaonedev/ai-memory-mcp/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/alphaonedev/ai-memory-mcp/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/alphaonedev/ai-memory-mcp/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/alphaonedev/ai-memory-mcp/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/alphaonedev/ai-memory-mcp/releases/tag/v0.1.0
