use std::path::{Path, PathBuf};

use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};

use crate::error::{Result, SearchError};
use crate::indexer::schema::{DocId, DocRecord, StructuralMeta};

// Table: doc_id (u32) → bincode-encoded DocRecord
const DOCS: TableDefinition<u32, &[u8]> = TableDefinition::new("docs");

// Table: canonical path string → doc_id (u32)
const PATH_INDEX: TableDefinition<&str, u32> = TableDefinition::new("path_index");

// Table: doc_id (u32) → bincode-encoded StructuralMeta
const METADATA: TableDefinition<u32, &[u8]> = TableDefinition::new("metadata");

// Table: "next_id" → u32 for auto-incrementing doc IDs
const COUNTERS: TableDefinition<&str, u32> = TableDefinition::new("counters");

pub struct DocStore {
    db: Database,
}

impl DocStore {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Database::create(path)?;

        // Ensure tables exist
        let tx = db.begin_write()?;
        tx.open_table(DOCS)?;
        tx.open_table(PATH_INDEX)?;
        tx.open_table(METADATA)?;
        tx.open_table(COUNTERS)?;
        tx.commit()?;

        Ok(DocStore { db })
    }

    /// Allocate a new unique DocId.
    pub fn next_doc_id(&self) -> Result<DocId> {
        let tx = self.db.begin_write()?;
        let current = {
            let mut counters = tx.open_table(COUNTERS)?;
            let current = counters
                .get("next_id")?
                .map(|v| v.value())
                .unwrap_or(0);
            counters.insert("next_id", current + 1)?;
            current
        }; // `counters` dropped here, releasing the borrow on `tx`
        tx.commit()?;
        Ok(current)
    }

    /// Store a DocRecord and its StructuralMeta.
    pub fn put_doc(&self, record: &DocRecord, meta: &StructuralMeta) -> Result<()> {
        let doc_bytes = encode(record)?;
        let meta_bytes = encode(meta)?;
        let path_key = record.path.to_string_lossy().into_owned();

        let tx = self.db.begin_write()?;
        {
            let mut docs = tx.open_table(DOCS)?;
            docs.insert(record.id, doc_bytes.as_slice())?;

            let mut path_idx = tx.open_table(PATH_INDEX)?;
            path_idx.insert(path_key.as_str(), record.id)?;

            let mut metadata = tx.open_table(METADATA)?;
            metadata.insert(record.id, meta_bytes.as_slice())?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Retrieve a DocRecord by ID.
    pub fn get_doc(&self, id: DocId) -> Result<Option<DocRecord>> {
        let tx = self.db.begin_read()?;
        let docs = tx.open_table(DOCS)?;
        match docs.get(id)? {
            Some(v) => Ok(Some(decode(v.value())?)),
            None => Ok(None),
        }
    }

    /// Retrieve StructuralMeta by DocId.
    pub fn get_metadata(&self, id: DocId) -> Result<Option<StructuralMeta>> {
        let tx = self.db.begin_read()?;
        let metadata = tx.open_table(METADATA)?;
        match metadata.get(id)? {
            Some(v) => Ok(Some(decode(v.value())?)),
            None => Ok(None),
        }
    }

    /// Look up a DocId by file path.
    pub fn doc_id_for_path(&self, path: &Path) -> Result<Option<DocId>> {
        let key = path.to_string_lossy().into_owned();
        let tx = self.db.begin_read()?;
        let path_idx = tx.open_table(PATH_INDEX)?;
        Ok(path_idx.get(key.as_str())?.map(|v| v.value()))
    }

    /// Return all (DocId, path) pairs in the store.
    pub fn all_docs(&self) -> Result<Vec<(DocId, PathBuf)>> {
        let tx = self.db.begin_read()?;
        let path_idx = tx.open_table(PATH_INDEX)?;
        let mut result = Vec::new();
        for item in path_idx.iter()? {
            let item = item?;
            let doc_id = item.1.value();
            let path = PathBuf::from(item.0.value());
            result.push((doc_id, path));
        }
        Ok(result)
    }

    /// Remove a document (by path tombstoning).
    pub fn remove_doc(&self, id: DocId, path: &Path) -> Result<()> {
        let path_key = path.to_string_lossy().into_owned();
        let tx = self.db.begin_write()?;
        {
            let mut docs = tx.open_table(DOCS)?;
            docs.remove(id)?;
            let mut path_idx = tx.open_table(PATH_INDEX)?;
            path_idx.remove(path_key.as_str())?;
            let mut metadata = tx.open_table(METADATA)?;
            metadata.remove(id)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Total number of indexed documents.
    pub fn doc_count(&self) -> Result<usize> {
        let tx = self.db.begin_read()?;
        let docs = tx.open_table(DOCS)?;
        Ok(docs.len()? as usize)
    }
}

fn encode<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    bincode::serde::encode_to_vec(value, bincode::config::standard())
        .map_err(|e: bincode::error::EncodeError| SearchError::Serialization(e.to_string()))
}

fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    let (value, _) = bincode::serde::decode_from_slice(bytes, bincode::config::standard())
        .map_err(|e: bincode::error::DecodeError| SearchError::Serialization(e.to_string()))?;
    Ok(value)
}
