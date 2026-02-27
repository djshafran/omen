//! Semantic search module for natural language code queries.
//!
//! Uses TF-IDF with sublinear TF and bigram tokenization for fast, accurate code search.
//! No external model download required -- pure Rust implementation.
//!
//! # Architecture
//!
//! - **tfidf**: TF-IDF engine (vocabulary, IDF, sparse vector search)
//! - **chunking**: AST-aware chunking of long functions at statement boundaries
//! - **embed**: Text formatting for enriched symbol representation
//! - **cache**: SQLite storage for symbols and staleness tracking
//! - **sync**: Incremental indexing and staleness detection
//! - **search**: Query engine wrapping TF-IDF over cached symbols

pub mod cache;
pub mod chunking;
pub mod embed;
pub mod multi_repo;
pub mod search;
pub mod sync;
pub mod tfidf;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::core::{FileSet, Result};

pub use cache::EmbeddingCache;
pub use search::{SearchEngine, SearchFilters, SearchOutput, SearchResult};
pub use sync::{SyncManager, SyncStats};
pub use tfidf::TfidfEngine;

/// Configuration for semantic search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Path to the cache database (default: .omen/search.db)
    pub cache_path: Option<PathBuf>,
    /// Maximum number of results to return
    pub max_results: usize,
    /// Minimum similarity score (0-1)
    pub min_score: f32,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            cache_path: None,
            max_results: 20,
            min_score: 0.3,
        }
    }
}

/// High-level semantic search interface.
pub struct SemanticSearch {
    cache: EmbeddingCache,
    root_path: PathBuf,
    config: SearchConfig,
}

impl SemanticSearch {
    /// Create a new semantic search instance.
    pub fn new(config: &SearchConfig, root_path: impl AsRef<Path>) -> Result<Self> {
        let root_path = root_path.as_ref().to_path_buf();

        let cache_path = config
            .cache_path
            .clone()
            .unwrap_or_else(|| root_path.join(".omen").join("search.db"));

        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let cache = EmbeddingCache::open(&cache_path)?;

        Ok(Self {
            cache,
            root_path,
            config: config.clone(),
        })
    }

    /// Index the repository (or update the index).
    pub fn index(&self, file_config: &Config) -> Result<SyncStats> {
        let file_set = FileSet::from_path(&self.root_path, file_config)?;
        let sync_manager = SyncManager::new(&self.cache);
        sync_manager.sync(&file_set, &self.root_path)
    }

    /// Index only a subset of files/directories (comma-separated paths).
    ///
    /// Paths may be relative to root or absolute paths under the root.
    /// Non-matching paths are ignored.
    pub fn index_in_paths(&self, file_config: &Config, paths: &str) -> Result<SyncStats> {
        let file_set = FileSet::from_path(&self.root_path, file_config)?;
        let filtered = filter_file_set_by_paths(&file_set, paths);
        let sync_manager = SyncManager::new(&self.cache);
        sync_manager.sync(&filtered, &self.root_path)
    }

    /// Search for symbols matching the query.
    pub fn search(&self, query: &str, top_k: Option<usize>) -> Result<SearchOutput> {
        let top_k = top_k.unwrap_or(self.config.max_results);
        let search_engine = SearchEngine::new(&self.cache);

        let results = search_engine.search_with_threshold(query, top_k, self.config.min_score)?;
        let total_symbols = self.cache.symbol_count()?;

        Ok(SearchOutput::new(query.to_string(), total_symbols, results))
    }

    /// Search with combined filters (min score, max complexity).
    pub fn search_filtered(
        &self,
        query: &str,
        top_k: Option<usize>,
        filters: &SearchFilters,
    ) -> Result<SearchOutput> {
        let top_k = top_k.unwrap_or(self.config.max_results);
        let search_engine = SearchEngine::new(&self.cache);

        let results = search_engine.search_filtered(query, top_k, filters)?;
        let total_symbols = self.cache.symbol_count()?;

        Ok(SearchOutput::new(query.to_string(), total_symbols, results))
    }

    /// Search within specific files.
    pub fn search_in_files(
        &self,
        query: &str,
        file_paths: &[&str],
        top_k: Option<usize>,
    ) -> Result<SearchOutput> {
        let top_k = top_k.unwrap_or(self.config.max_results);
        let search_engine = SearchEngine::new(&self.cache);

        let results = search_engine.search_in_files(query, file_paths, top_k)?;
        let total_symbols = self.cache.symbol_count()?;

        Ok(SearchOutput::new(query.to_string(), total_symbols, results))
    }

    /// Get the number of indexed symbols.
    pub fn symbol_count(&self) -> Result<usize> {
        self.cache.symbol_count()
    }

    /// Get the root path.
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }
}

fn filter_file_set_by_paths(file_set: &FileSet, paths: &str) -> FileSet {
    let scopes = parse_scopes(paths, file_set.root());
    if scopes.is_empty() {
        return file_set.clone();
    }

    let matched: Vec<PathBuf> = file_set
        .files()
        .iter()
        .filter(|rel| {
            let rel_norm = normalize_scope(rel.to_string_lossy().as_ref());
            scopes
                .iter()
                .any(|scope| rel_norm == *scope || rel_norm.starts_with(&format!("{scope}/")))
        })
        .cloned()
        .collect();

    FileSet::from_files(file_set.root().to_path_buf(), matched)
}

fn parse_scopes(paths: &str, root: &Path) -> Vec<String> {
    let mut scopes = Vec::new();
    for raw in paths.split(',') {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }

        let path = Path::new(raw);
        let normalized = if path.is_absolute() {
            path.strip_prefix(root)
                .ok()
                .map(|p| normalize_scope(p.to_string_lossy().as_ref()))
        } else {
            Some(normalize_scope(raw))
        };

        if let Some(scope) = normalized {
            if !scope.is_empty() && !scopes.contains(&scope) {
                scopes.push(scope);
            }
        }
    }
    scopes
}

fn normalize_scope(value: &str) -> String {
    let mut normalized = value.trim().replace('\\', "/");
    while normalized.ends_with('/') {
        normalized.pop();
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_config_default() {
        let config = SearchConfig::default();
        assert_eq!(config.max_results, 20);
        assert_eq!(config.min_score, 0.3);
        assert!(config.cache_path.is_none());
    }

    #[test]
    fn test_search_config_serialization() {
        let config = SearchConfig {
            cache_path: Some(PathBuf::from("/tmp/search.db")),
            max_results: 10,
            min_score: 0.5,
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("10"));
        assert!(json.contains("0.5"));

        let deserialized: SearchConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.max_results, 10);
    }

    #[test]
    fn test_semantic_search_search_filtered() {
        use crate::semantic::cache::CachedSymbol;

        let temp = tempfile::tempdir().unwrap();
        let cache_path = temp.path().join("search.db");
        let config = SearchConfig {
            cache_path: Some(cache_path.clone()),
            max_results: 20,
            min_score: 0.0,
        };
        let search = SemanticSearch::new(&config, temp.path()).unwrap();

        // Manually insert symbols with different complexities
        let cache = cache::EmbeddingCache::open(&cache_path).unwrap();
        cache
            .upsert_symbol(&CachedSymbol {
                file_path: "src/a.rs".to_string(),
                symbol_name: "low_complexity".to_string(),
                symbol_type: "function".to_string(),
                parent_name: None,
                signature: "fn low_complexity()".to_string(),
                start_line: 1,
                end_line: 5,
                chunk_index: 0,
                total_chunks: 1,
                content_hash: "h1".to_string(),
                enriched_text: "[src/a.rs] low_complexity\nfn low_complexity() { return }"
                    .to_string(),
                cyclomatic_complexity: Some(1),
                cognitive_complexity: Some(0),
            })
            .unwrap();
        cache
            .upsert_symbol(&CachedSymbol {
                file_path: "src/b.rs".to_string(),
                symbol_name: "high_complexity".to_string(),
                symbol_type: "function".to_string(),
                parent_name: None,
                signature: "fn high_complexity()".to_string(),
                start_line: 1,
                end_line: 50,
                chunk_index: 0,
                total_chunks: 1,
                content_hash: "h2".to_string(),
                enriched_text: "[src/b.rs] high_complexity\nfn high_complexity() { return }"
                    .to_string(),
                cyclomatic_complexity: Some(20),
                cognitive_complexity: Some(25),
            })
            .unwrap();

        // Without filter: both appear
        let all = search.search("complexity", Some(10)).unwrap();
        assert_eq!(all.results.len(), 2);

        // With max_complexity=5: only low_complexity
        let filters = SearchFilters {
            min_score: 0.0,
            max_complexity: Some(5),
        };
        let filtered = search
            .search_filtered("complexity", Some(10), &filters)
            .unwrap();
        assert_eq!(filtered.results.len(), 1);
        assert_eq!(filtered.results[0].symbol_name, "low_complexity");
    }

    #[test]
    fn test_filter_file_set_by_paths_directory_scope() {
        let root = PathBuf::from("/repo");
        let file_set = FileSet::from_files(
            root.clone(),
            vec![
                PathBuf::from("src/a.rs"),
                PathBuf::from("src/nested/b.rs"),
                PathBuf::from("tests/c.rs"),
            ],
        );

        let filtered = filter_file_set_by_paths(&file_set, "src/");
        let files = filtered.files();

        assert_eq!(files.len(), 2);
        assert!(files.contains(&PathBuf::from("src/a.rs")));
        assert!(files.contains(&PathBuf::from("src/nested/b.rs")));
    }

    #[test]
    fn test_filter_file_set_by_paths_absolute_scope() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let file_set = FileSet::from_files(
            root.clone(),
            vec![PathBuf::from("src/a.rs"), PathBuf::from("src/b.rs")],
        );
        let absolute = root.join("src").join("a.rs");

        let filtered = filter_file_set_by_paths(&file_set, &absolute.to_string_lossy());
        let files = filtered.files();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0], PathBuf::from("src/a.rs"));
    }
}
