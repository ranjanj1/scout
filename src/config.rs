use std::path::{Path, PathBuf};

use crate::error::{Result, SearchError};

/// Resolved paths for all index files.
#[derive(Debug, Clone)]
pub struct IndexConfig {
    /// Root directory: `.searchindex/`
    pub root: PathBuf,
    /// `docstore.redb` — DocRecord KV store
    pub docstore_path: PathBuf,
    /// `metadata.redb` — StructuralMeta KV store
    pub metadata_path: PathBuf,
    /// `segments/` — directory containing segment subdirectories
    pub segments_dir: PathBuf,
    /// `write.lock` — file lock for concurrent safety
    pub lock_path: PathBuf,
}

impl IndexConfig {
    pub fn new(root: PathBuf) -> Self {
        IndexConfig {
            docstore_path: root.join("docstore.redb"),
            metadata_path: root.join("metadata.redb"),
            segments_dir: root.join("segments"),
            lock_path: root.join("write.lock"),
            root,
        }
    }

    /// Path for a specific segment directory, e.g. `segments/0001/`
    pub fn segment_dir(&self, id: u32) -> PathBuf {
        self.segments_dir.join(format!("{:04}", id))
    }

    /// Ensure all index directories exist.
    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::create_dir_all(&self.segments_dir)?;
        Ok(())
    }
}

/// Resolve the index root with the following precedence:
/// 1. Explicit `--index <path>` CLI override
/// 2. Walk up from cwd looking for `.searchindex/`
/// 3. Fall back to `~/.searchindex/`
pub fn resolve_index(hint: Option<PathBuf>) -> Result<IndexConfig> {
    if let Some(path) = hint {
        return Ok(IndexConfig::new(path));
    }

    // Walk up from cwd
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir: &Path = &cwd;
        loop {
            let candidate = dir.join(".searchindex");
            if candidate.exists() {
                return Ok(IndexConfig::new(candidate));
            }
            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }
    }

    // Fall back to home directory
    let home = home_dir().ok_or_else(|| {
        SearchError::IndexNotFound(PathBuf::from("~/.searchindex"))
    })?;
    Ok(IndexConfig::new(home.join(".searchindex")))
}

/// Resolve index, returning error if it doesn't exist yet (for search commands).
pub fn require_index(hint: Option<PathBuf>) -> Result<IndexConfig> {
    let config = resolve_index(hint)?;
    if !config.root.exists() {
        return Err(SearchError::IndexNotFound(config.root));
    }
    Ok(config)
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
