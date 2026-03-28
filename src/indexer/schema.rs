use std::path::PathBuf;
use std::time::SystemTime;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};


/// Stable numeric identifier for an indexed document.
pub type DocId = u32;

/// A 3-byte trigram key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Trigram(pub [u8; 3]);

impl Trigram {
    pub fn as_bytes(&self) -> &[u8; 3] {
        &self.0
    }
}

impl std::fmt::Display for Trigram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(&self.0))
    }
}

/// Byte positions within a document where a trigram occurs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostingEntry {
    pub doc_id: DocId,
    /// Byte offsets in the document text (delta-coded on disk, expanded in memory).
    pub positions: Vec<u32>,
}

/// Heuristically inferred structural metadata extracted from a document.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StructuralMeta {
    /// Inferred document category (e.g. "contract", "invoice", "report").
    pub doc_type: Option<String>,
    /// ISO dates found in the document text.
    pub dates: Vec<NaiveDate>,
    /// Currency amounts found (e.g. $1,000,000 → 1000000.0).
    pub amounts: Vec<f64>,
    /// Email addresses found.
    pub emails: Vec<String>,
    /// Capitalized noun phrases (simple heuristic named entities).
    pub entities: Vec<String>,
}

/// Full record stored in docstore.redb for each indexed document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocRecord {
    pub id: DocId,
    pub path: PathBuf,
    /// File modification time at index time.
    pub mtime: SystemTime,
    /// xxh64 of raw file bytes for change detection.
    pub file_hash: u64,
    /// Optional document title (first heading or filename stem).
    pub title: Option<String>,
    /// First ~256 chars of extracted text for display snippets.
    pub snippet: String,
    /// 64-bit SimHash fingerprint.
    pub simhash: u64,
}
