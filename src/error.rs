use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Redb(#[from] redb::Error),

    #[error("Database error: {0}")]
    RedbDatabase(#[from] redb::DatabaseError),

    #[error("Database transaction error: {0}")]
    RedbTransaction(#[from] redb::TransactionError),

    #[error("Database table error: {0}")]
    RedbTable(#[from] redb::TableError),

    #[error("Database storage error: {0}")]
    RedbStorage(#[from] redb::StorageError),

    #[error("Database commit error: {0}")]
    RedbCommit(#[from] redb::CommitError),

    #[error("Mmap error: {0}")]
    Mmap(String),

    #[error("Failed to parse {path}: {reason}")]
    Parse { path: PathBuf, reason: String },

    #[error("Query syntax error: {0}")]
    QuerySyntax(String),

    #[error("Index not found at {0}. Run `scout index <path>` first.")]
    IndexNotFound(PathBuf),

    #[error("Corrupt index: {0}")]
    CorruptIndex(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Unsupported file type: {0}")]
    UnsupportedFileType(PathBuf),
}

pub type Result<T> = std::result::Result<T, SearchError>;
