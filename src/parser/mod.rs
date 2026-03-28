pub mod code;
pub mod docx;
pub mod metadata;
pub mod pdf;
pub mod text;
pub mod walker;

use std::path::Path;

use xxhash_rust::xxh64::xxh64;

use crate::error::{Result, SearchError};
use crate::indexer::schema::StructuralMeta;
use crate::parser::walker::{FileKind, WalkEntry};

/// Fully parsed document ready for indexing.
#[derive(Debug, Clone)]
pub struct ParsedDoc {
    /// Extracted plain text (normalized, stripped of markup).
    pub text: String,
    /// Document title (first heading, PDF title field, or filename stem).
    pub title: Option<String>,
    /// First ~256 chars for display snippets.
    pub snippet: String,
    /// Heuristically extracted structural metadata.
    pub metadata: StructuralMeta,
}

/// Parse a file based on its detected kind.
/// Extracts text + metadata. Returns error for unsupported or unreadable files.
pub fn parse(entry: &WalkEntry) -> Result<ParsedDoc> {
    let mut doc = match &entry.kind {
        FileKind::PlainText => text::parse_text(&entry.path, &entry.kind)?,
        FileKind::Markdown => text::parse_text(&entry.path, &entry.kind)?,
        FileKind::Pdf => pdf::parse_pdf(&entry.path)?,
        FileKind::Docx => docx::parse_docx(&entry.path)?,
        FileKind::Code(lang) => code::parse_code(&entry.path, lang)?,
        FileKind::Unknown => {
            return Err(SearchError::UnsupportedFileType(entry.path.clone()));
        }
    };

    // Enrich with structural metadata extracted from full text
    doc.metadata = metadata::extract_metadata(&doc.text, &entry.path);

    Ok(doc)
}

/// Compute xxh64 hash of raw file bytes for change detection.
pub fn file_hash(path: &Path) -> Result<u64> {
    let bytes = std::fs::read(path).map_err(|e| SearchError::Parse {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;
    Ok(xxh64(&bytes, 0))
}
