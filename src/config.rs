// Copyright (c) 2026 AlphaOne LLC. All rights reserved.
// Licensed under the MIT License. See LICENSE file in the project root.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Embedding models
// ---------------------------------------------------------------------------

/// Supported embedding models for semantic search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingModel {
    /// sentence-transformers/all-MiniLM-L6-v2 — 384-dim, ~90 MB
    MiniLmL6V2,
    /// nomic-ai/nomic-embed-text-v1.5 — 768-dim, ~270 MB
    NomicEmbedV15,
}

impl EmbeddingModel {
    /// Embedding vector dimensionality.
    pub fn dim(&self) -> usize {
        match self {
            Self::MiniLmL6V2 => 384,
            Self::NomicEmbedV15 => 768,
        }
    }

    /// HuggingFace model identifier.
    pub fn hf_model_id(&self) -> &str {
        match self {
            Self::MiniLmL6V2 => "sentence-transformers/all-MiniLM-L6-v2",
            Self::NomicEmbedV15 => "nomic-ai/nomic-embed-text-v1.5",
        }
    }
}

// ---------------------------------------------------------------------------
// LLM models
// ---------------------------------------------------------------------------

/// Supported LLM models (served via Ollama).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmModel {
    /// Google Gemma 4 Effective 2B — ~1 GB Q4
    Gemma4E2B,
    /// Google Gemma 4 Effective 4B — ~2.3 GB Q4
    Gemma4E4B,
}

impl LlmModel {
    /// Ollama model tag used to pull / run this model.
    pub fn ollama_model_id(&self) -> &str {
        match self {
            Self::Gemma4E2B => "gemma4:e2b",
            Self::Gemma4E4B => "gemma4:e4b",
        }
    }

    /// Human-readable display name.
    pub fn display_name(&self) -> &str {
        match self {
            Self::Gemma4E2B => "Gemma 4 Effective 2B (Q4)",
            Self::Gemma4E4B => "Gemma 4 Effective 4B (Q4)",
        }
    }
}

// ---------------------------------------------------------------------------
// Feature tiers
// ---------------------------------------------------------------------------

/// Feature tiers control which AI capabilities are active based on the
/// available memory budget on the host machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureTier {
    /// FTS5 keyword search only — 0 MB extra.
    Keyword,
    /// MiniLM embeddings + HNSW index — ~256 MB.
    Semantic,
    /// nomic-embed + Gemma 4 E2B via Ollama — ~1 GB.
    Smart,
    /// nomic-embed + Gemma 4 E4B + cross-encoder via Ollama — ~4 GB.
    Autonomous,
}

impl FeatureTier {
    /// Parse a tier name (case-insensitive).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "keyword" => Some(Self::Keyword),
            "semantic" => Some(Self::Semantic),
            "smart" => Some(Self::Smart),
            "autonomous" => Some(Self::Autonomous),
            _ => None,
        }
    }

    /// Canonical lowercase name.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Keyword => "keyword",
            Self::Semantic => "semantic",
            Self::Smart => "smart",
            Self::Autonomous => "autonomous",
        }
    }

    /// Build the full [`TierConfig`] for this tier.
    pub fn config(&self) -> TierConfig {
        match self {
            Self::Keyword => TierConfig {
                tier: *self,
                embedding_model: None,
                llm_model: None,
                cross_encoder: false,
                max_memory_mb: 0,
            },
            Self::Semantic => TierConfig {
                tier: *self,
                embedding_model: Some(EmbeddingModel::MiniLmL6V2),
                llm_model: None,
                cross_encoder: false,
                max_memory_mb: 256,
            },
            Self::Smart => TierConfig {
                tier: *self,
                embedding_model: Some(EmbeddingModel::NomicEmbedV15),
                llm_model: Some(LlmModel::Gemma4E2B),
                cross_encoder: false,
                max_memory_mb: 1024,
            },
            Self::Autonomous => TierConfig {
                tier: *self,
                embedding_model: Some(EmbeddingModel::NomicEmbedV15),
                llm_model: Some(LlmModel::Gemma4E4B),
                cross_encoder: true,
                max_memory_mb: 4096,
            },
        }
    }

    /// Automatically select the best tier that fits within `mb` megabytes.
    #[allow(dead_code)]
    pub fn from_memory_budget(mb: usize) -> Self {
        if mb >= 4096 {
            Self::Autonomous
        } else if mb >= 1024 {
            Self::Smart
        } else if mb >= 256 {
            Self::Semantic
        } else {
            Self::Keyword
        }
    }
}

impl std::fmt::Display for FeatureTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Tier configuration
// ---------------------------------------------------------------------------

/// Runtime configuration derived from a [`FeatureTier`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierConfig {
    pub tier: FeatureTier,
    pub embedding_model: Option<EmbeddingModel>,
    pub llm_model: Option<LlmModel>,
    pub cross_encoder: bool,
    pub max_memory_mb: usize,
}

impl TierConfig {
    /// Produce a [`Capabilities`] report suitable for JSON serialisation.
    pub fn capabilities(&self) -> Capabilities {
        let has_embeddings = self.embedding_model.is_some();
        let has_llm = self.llm_model.is_some();

        Capabilities {
            tier: self.tier.as_str().to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            features: CapabilityFeatures {
                keyword_search: true,
                semantic_search: has_embeddings,
                hybrid_recall: has_embeddings,
                query_expansion: has_llm,
                auto_consolidation: has_llm,
                auto_tagging: has_llm,
                contradiction_analysis: has_llm,
                cross_encoder_reranking: self.cross_encoder,
                memory_reflection: self.cross_encoder && has_llm,
            },
            models: CapabilityModels {
                embedding: self
                    .embedding_model
                    .map(|m| m.hf_model_id().to_string())
                    .unwrap_or_else(|| "none".to_string()),
                embedding_dim: self.embedding_model.map(|m| m.dim()).unwrap_or(0),
                llm: self
                    .llm_model
                    .map(|m| m.ollama_model_id().to_string())
                    .unwrap_or_else(|| "none".to_string()),
                cross_encoder: if self.cross_encoder {
                    "cross-encoder/ms-marco-MiniLM-L-6-v2".to_string()
                } else {
                    "none".to_string()
                },
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Capability reporting
// ---------------------------------------------------------------------------

/// Top-level capabilities report for a running instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub tier: String,
    pub version: String,
    pub features: CapabilityFeatures,
    pub models: CapabilityModels,
}

/// Boolean feature flags exposed in the capabilities report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityFeatures {
    pub keyword_search: bool,
    pub semantic_search: bool,
    pub hybrid_recall: bool,
    pub query_expansion: bool,
    pub auto_consolidation: bool,
    pub auto_tagging: bool,
    pub contradiction_analysis: bool,
    pub cross_encoder_reranking: bool,
    pub memory_reflection: bool,
}

/// Model identifiers exposed in the capabilities report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityModels {
    pub embedding: String,
    pub embedding_dim: usize,
    pub llm: String,
    pub cross_encoder: String,
}

// ---------------------------------------------------------------------------
// Persistent config file (~/.config/ai-memory/config.toml)
// ---------------------------------------------------------------------------

const CONFIG_DIR: &str = ".config/ai-memory";
const CONFIG_FILE: &str = "config.toml";

// ---------------------------------------------------------------------------
// Database path resolution
// ---------------------------------------------------------------------------

/// Compute the platform-appropriate default absolute path for the database.
///
/// Resolution order:
/// 1. `$XDG_DATA_HOME/ai-memory/ai-memory.db` (if `XDG_DATA_HOME` is set)
/// 2. `$HOME/.local/share/ai-memory/ai-memory.db`
/// 3. Falls back to `ai-memory.db` only if `HOME` cannot be determined
pub fn default_db_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("ai-memory").join("ai-memory.db");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return PathBuf::from(&home)
                .join(".local/share/ai-memory")
                .join("ai-memory.db");
        }
    }
    // Windows fallback
    if let Ok(profile) = std::env::var("USERPROFILE") {
        if !profile.is_empty() {
            return PathBuf::from(&profile)
                .join(".local/share/ai-memory")
                .join("ai-memory.db");
        }
    }
    // Last resort — preserves old behaviour but should never happen on a sane OS
    PathBuf::from("ai-memory.db")
}

/// Expand a leading `~` or `~user` to the home directory and canonicalise.
///
/// Returns the path unchanged if it does not start with `~`.
/// Logs a warning (but does **not** error) if the resolved path is relative,
/// because a relative database path causes silent fragmentation.
pub fn resolve_db_path(raw: &Path) -> PathBuf {
    let expanded = expand_tilde(raw);
    if expanded.is_relative() {
        tracing::warn!(
            "ai-memory: database path '{}' is relative — this may cause fragmentation across \
             working directories. Set an absolute path in config.toml or via --db.",
            expanded.display()
        );
    }
    expanded
}

/// Pure tilde-expansion helper: `~/foo` → `$HOME/foo`, `~` → `$HOME`.
fn expand_tilde(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if !s.starts_with('~') {
        return p.to_path_buf();
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    if home.is_empty() {
        return p.to_path_buf();
    }
    if s == "~" {
        return PathBuf::from(&home);
    }
    if let Some(rest) = s.strip_prefix("~/") {
        return PathBuf::from(&home).join(rest);
    }
    // ~otheruser — not expanded (we only handle current user)
    p.to_path_buf()
}

/// Ensure the parent directory of the database file exists.
/// Creates with mode 0700 on Unix to prevent other users reading memory data.
/// Returns an error only if the directory cannot be created.
pub fn ensure_db_parent(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                std::fs::DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(parent)?;
            }
            #[cfg(not(unix))]
            {
                std::fs::create_dir_all(parent)?;
            }
        }
    }
    Ok(())
}

/// Persistent configuration loaded from `~/.config/ai-memory/config.toml`.
///
/// All fields are optional — CLI flags override file values, which override
/// compiled defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    /// Feature tier: keyword, semantic, smart, autonomous
    pub tier: Option<String>,
    /// Path to the SQLite database file
    pub db: Option<String>,
    /// Ollama base URL for LLM generation (default: http://localhost:11434)
    pub ollama_url: Option<String>,
    /// Separate URL for embedding model (defaults to ollama_url if unset)
    pub embed_url: Option<String>,
    /// Embedding model override: mini_lm_l6_v2 or nomic_embed_v15
    pub embedding_model: Option<String>,
    /// LLM model override (Ollama tag, e.g. "gemma4:e2b")
    pub llm_model: Option<String>,
    /// Enable cross-encoder reranking (true/false)
    pub cross_encoder: Option<bool>,
    /// Default namespace for new memories
    pub default_namespace: Option<String>,
    /// Maximum memory budget in MB (used for auto tier selection)
    pub max_memory_mb: Option<usize>,
}

impl AppConfig {
    /// Returns the config file path: `~/.config/ai-memory/config.toml`
    pub fn config_path() -> Option<PathBuf> {
        let home = std::env::var("HOME").ok()?;
        Some(Path::new(&home).join(CONFIG_DIR).join(CONFIG_FILE))
    }

    /// Load config from disk. Returns `AppConfig::default()` if file is missing.
    /// Set `AI_MEMORY_NO_CONFIG=1` to skip config loading (used by integration tests).
    pub fn load() -> Self {
        if std::env::var("AI_MEMORY_NO_CONFIG").is_ok() {
            return Self::default();
        }
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        Self::load_from(&path)
    }

    /// Load config from a specific path.
    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(cfg) => {
                    eprintln!("ai-memory: loaded config from {}", path.display());
                    cfg
                }
                Err(e) => {
                    tracing::warn!("ai-memory: invalid config file, using defaults: {}", e);
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    /// Resolve the effective feature tier from config (CLI flag overrides).
    pub fn effective_tier(&self, cli_tier: Option<&str>) -> FeatureTier {
        let tier_str = cli_tier.or(self.tier.as_deref()).unwrap_or("semantic");
        match FeatureTier::from_str(tier_str) {
            Some(t) => t,
            None => {
                tracing::warn!(
                    "ai-memory: unknown tier '{}', falling back to semantic",
                    tier_str
                );
                FeatureTier::Semantic
            }
        }
    }

    /// Resolve the effective database path.
    ///
    /// Priority (highest wins):
    /// 1. `--db` explicitly passed on the command line (`cli_explicit == true`)
    /// 2. `db` key in `config.toml`
    /// 3. XDG-compliant default (`~/.local/share/ai-memory/ai-memory.db`)
    ///
    /// All paths are run through [`resolve_db_path`] for tilde expansion and
    /// relative-path warnings.  The parent directory is created automatically.
    pub fn effective_db(&self, cli_db: &Path, cli_explicit: bool) -> PathBuf {
        let raw = if cli_explicit {
            // User explicitly passed --db — honour it exactly
            cli_db.to_path_buf()
        } else if let Some(ref cfg_db) = self.db {
            // config.toml has a db key
            PathBuf::from(cfg_db)
        } else {
            // No --db flag, no config → use the safe absolute default
            default_db_path()
        };

        let resolved = resolve_db_path(&raw);

        // Best-effort directory creation
        if let Err(e) = ensure_db_parent(&resolved) {
            tracing::warn!(
                "ai-memory: could not create database directory {}: {}",
                resolved.parent().unwrap_or(&resolved).display(),
                e
            );
        }

        resolved
    }

    /// Resolve Ollama URL for LLM generation (config or default).
    pub fn effective_ollama_url(&self) -> &str {
        self.ollama_url
            .as_deref()
            .unwrap_or("http://localhost:11434")
    }

    /// Resolve URL for embedding model (falls back to ollama_url).
    pub fn effective_embed_url(&self) -> &str {
        self.embed_url
            .as_deref()
            .or(self.ollama_url.as_deref())
            .unwrap_or("http://localhost:11434")
    }

    /// Log warnings for non-localhost URLs (call once at startup).
    pub fn warn_non_localhost_urls(&self) {
        let ollama = self.effective_ollama_url();
        if !ollama.contains("localhost")
            && !ollama.contains("127.0.0.1")
            && !ollama.contains("[::1]")
        {
            tracing::warn!(
                "ollama_url points to non-localhost: {} — ensure this is intentional (SSRF risk)",
                ollama
            );
        }
        let embed = self.effective_embed_url();
        if embed != ollama
            && !embed.contains("localhost")
            && !embed.contains("127.0.0.1")
            && !embed.contains("[::1]")
        {
            tracing::warn!(
                "embed_url points to non-localhost: {} — ensure this is intentional (SSRF risk)",
                embed
            );
        }
    }

    /// Write a default config file if one doesn't exist yet.
    pub fn write_default_if_missing() {
        let Some(path) = Self::config_path() else {
            return;
        };
        if path.exists() {
            return;
        }
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let default_toml = r#"# ai-memory configuration
# See: https://github.com/alphaonedev/ai-memory-mcp

# Feature tier: keyword, semantic, smart, autonomous
# tier = "semantic"

# Path to SQLite database (absolute path recommended).
# When unset the default is ~/.local/share/ai-memory/ai-memory.db (XDG).
# Tilde (~) is expanded to $HOME automatically.
# WARNING: a relative path will create a separate database in every working
#          directory — do NOT use a relative path unless you know what you are doing.
# db = "~/.local/share/ai-memory/ai-memory.db"

# Ollama base URL (for smart/autonomous tiers)
# ollama_url = "http://localhost:11434"

# Embedding model: mini_lm_l6_v2 (384-dim) or nomic_embed_v15 (768-dim)
# embedding_model = "mini_lm_l6_v2"

# LLM model tag for Ollama
# llm_model = "gemma4:e2b"

# Enable neural cross-encoder reranking (autonomous tier)
# cross_encoder = true

# Default namespace for new memories
# default_namespace = "global"

# Memory budget in MB (for auto tier selection)
# max_memory_mb = 4096
"#;
        let _ = std::fs::write(&path, default_toml);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_roundtrip() {
        for tier in [
            FeatureTier::Keyword,
            FeatureTier::Semantic,
            FeatureTier::Smart,
            FeatureTier::Autonomous,
        ] {
            assert_eq!(FeatureTier::from_str(tier.as_str()), Some(tier));
        }
    }

    #[test]
    fn budget_selection() {
        assert_eq!(FeatureTier::from_memory_budget(0), FeatureTier::Keyword);
        assert_eq!(FeatureTier::from_memory_budget(128), FeatureTier::Keyword);
        assert_eq!(FeatureTier::from_memory_budget(256), FeatureTier::Semantic);
        assert_eq!(FeatureTier::from_memory_budget(512), FeatureTier::Semantic);
        assert_eq!(FeatureTier::from_memory_budget(1024), FeatureTier::Smart);
        assert_eq!(FeatureTier::from_memory_budget(2048), FeatureTier::Smart);
        assert_eq!(
            FeatureTier::from_memory_budget(4096),
            FeatureTier::Autonomous
        );
        assert_eq!(
            FeatureTier::from_memory_budget(8192),
            FeatureTier::Autonomous
        );
    }

    #[test]
    fn embedding_dimensions() {
        assert_eq!(EmbeddingModel::MiniLmL6V2.dim(), 384);
        assert_eq!(EmbeddingModel::NomicEmbedV15.dim(), 768);
    }

    #[test]
    fn autonomous_has_cross_encoder() {
        let cfg = FeatureTier::Autonomous.config();
        assert!(cfg.cross_encoder);
        assert!(cfg.capabilities().features.cross_encoder_reranking);
        assert!(cfg.capabilities().features.memory_reflection);
    }

    #[test]
    fn keyword_has_no_models() {
        let cfg = FeatureTier::Keyword.config();
        assert!(cfg.embedding_model.is_none());
        assert!(cfg.llm_model.is_none());
        assert!(!cfg.cross_encoder);
        assert_eq!(cfg.max_memory_mb, 0);
    }

    #[test]
    fn capabilities_serialize() {
        let caps = FeatureTier::Smart.config().capabilities();
        let json = serde_json::to_string_pretty(&caps).unwrap();
        assert!(json.contains("\"tier\": \"smart\""));
        assert!(json.contains("nomic"));
        assert!(json.contains("gemma4:e2b"));
    }

    #[test]
    fn config_default_is_empty() {
        let cfg = AppConfig::default();
        assert!(cfg.tier.is_none());
        assert!(cfg.db.is_none());
        assert!(cfg.ollama_url.is_none());
    }

    #[test]
    fn config_parse_toml() {
        let toml_str = r#"
            tier = "smart"
            db = "/tmp/test.db"
            ollama_url = "http://localhost:11434"
            cross_encoder = true
        "#;
        let cfg: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.tier.as_deref(), Some("smart"));
        assert_eq!(cfg.db.as_deref(), Some("/tmp/test.db"));
        assert!(cfg.cross_encoder.unwrap());
    }

    #[test]
    fn config_effective_tier() {
        let cfg = AppConfig {
            tier: Some("smart".to_string()),
            ..Default::default()
        };
        // CLI override wins
        assert_eq!(
            cfg.effective_tier(Some("autonomous")),
            FeatureTier::Autonomous
        );
        // Config value used when no CLI
        assert_eq!(cfg.effective_tier(None), FeatureTier::Smart);
    }

    // -------------------------------------------------------------------
    // Database path resolution tests
    // -------------------------------------------------------------------

    #[test]
    fn expand_tilde_home() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let result = super::expand_tilde(Path::new("~/foo/bar.db"));
        assert_eq!(result, PathBuf::from(&home).join("foo/bar.db"));
    }

    #[test]
    fn expand_tilde_bare() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let result = super::expand_tilde(Path::new("~"));
        assert_eq!(result, PathBuf::from(&home));
    }

    #[test]
    fn expand_tilde_absolute_unchanged() {
        let result = super::expand_tilde(Path::new("/absolute/path.db"));
        assert_eq!(result, PathBuf::from("/absolute/path.db"));
    }

    #[test]
    fn expand_tilde_relative_unchanged() {
        let result = super::expand_tilde(Path::new("relative.db"));
        assert_eq!(result, PathBuf::from("relative.db"));
    }

    #[test]
    fn expand_tilde_other_user_unchanged() {
        // ~otheruser should NOT be expanded
        let result = super::expand_tilde(Path::new("~otheruser/foo"));
        assert_eq!(result, PathBuf::from("~otheruser/foo"));
    }

    #[test]
    fn default_db_path_is_absolute() {
        let p = super::default_db_path();
        assert!(
            p.is_absolute(),
            "default_db_path() returned relative path: {}",
            p.display()
        );
    }

    #[test]
    fn default_db_path_contains_ai_memory() {
        let p = super::default_db_path();
        let s = p.to_string_lossy();
        assert!(
            s.contains("ai-memory"),
            "default path should contain 'ai-memory': {}",
            s
        );
    }

    #[test]
    fn resolve_db_path_absolute_passes_through() {
        let p = super::resolve_db_path(Path::new("/tmp/test.db"));
        assert_eq!(p, PathBuf::from("/tmp/test.db"));
    }

    #[test]
    fn resolve_db_path_tilde_expanded() {
        let p = super::resolve_db_path(Path::new("~/test.db"));
        assert!(
            p.is_absolute(),
            "tilde-expanded path should be absolute: {}",
            p.display()
        );
        assert!(
            p.to_string_lossy().ends_with("test.db"),
            "should end with test.db: {}",
            p.display()
        );
    }

    #[test]
    fn effective_db_explicit_cli_wins() {
        let cfg = AppConfig {
            db: Some("/config/path.db".to_string()),
            ..Default::default()
        };
        let result = cfg.effective_db(Path::new("/cli/path.db"), true);
        assert_eq!(result, PathBuf::from("/cli/path.db"));
    }

    #[test]
    fn effective_db_config_used_when_no_cli() {
        let cfg = AppConfig {
            db: Some("/config/path.db".to_string()),
            ..Default::default()
        };
        let result = cfg.effective_db(Path::new("ignored-default"), false);
        assert_eq!(result, PathBuf::from("/config/path.db"));
    }

    #[test]
    fn effective_db_default_when_no_cli_no_config() {
        let cfg = AppConfig::default();
        let result = cfg.effective_db(Path::new("ignored"), false);
        // Should be the XDG default (absolute)
        assert!(
            result.is_absolute(),
            "default effective_db should be absolute: {}",
            result.display()
        );
    }

    #[test]
    fn effective_db_config_tilde_expanded() {
        let cfg = AppConfig {
            db: Some("~/my-memories.db".to_string()),
            ..Default::default()
        };
        let result = cfg.effective_db(Path::new("ignored"), false);
        assert!(
            result.is_absolute(),
            "tilde in config should be expanded: {}",
            result.display()
        );
    }

    #[test]
    fn ensure_db_parent_creates_directory() {
        let tmp = std::env::temp_dir().join(format!(
            "ai-memory-test-parent-{}",
            uuid::Uuid::new_v4()
        ));
        let db = tmp.join("sub/dir/test.db");
        assert!(!tmp.exists());
        super::ensure_db_parent(&db).unwrap();
        assert!(tmp.join("sub/dir").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn ensure_db_parent_noop_for_existing() {
        let tmp = std::env::temp_dir();
        let db = tmp.join("test.db");
        // Should not error on existing directory
        super::ensure_db_parent(&db).unwrap();
    }
}
