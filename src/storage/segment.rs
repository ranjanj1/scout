use std::path::{Path, PathBuf};

use crate::config::IndexConfig;
use crate::error::{Result, SearchError};

pub const MAX_SEGMENTS: usize = 8;

/// Metadata about a single index segment.
#[derive(Debug, Clone)]
pub struct Segment {
    pub id: u32,
    pub dir: PathBuf,
    pub postings_path: PathBuf,
    pub simhash_path: PathBuf,
}

impl Segment {
    pub fn new(id: u32, dir: PathBuf) -> Self {
        Segment {
            postings_path: dir.join("postings.trgm"),
            simhash_path: dir.join("simhash.bin"),
            id,
            dir,
        }
    }
}

/// List all existing segments in the index, sorted by ID ascending (oldest first).
pub fn list_segments(config: &IndexConfig) -> Result<Vec<Segment>> {
    if !config.segments_dir.exists() {
        return Ok(Vec::new());
    }

    let mut segments = Vec::new();
    for entry in std::fs::read_dir(&config.segments_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Ok(id) = name.parse::<u32>() {
            let dir = entry.path();
            segments.push(Segment::new(id, dir));
        }
    }
    segments.sort_by_key(|s| s.id);
    Ok(segments)
}

/// Allocate a new segment directory with the next available ID.
pub fn new_segment(config: &IndexConfig) -> Result<Segment> {
    let segments = list_segments(config)?;
    let next_id = segments.last().map(|s| s.id + 1).unwrap_or(0);
    let dir = config.segment_dir(next_id);
    std::fs::create_dir_all(&dir)?;
    Ok(Segment::new(next_id, dir))
}

/// Check if the number of segments exceeds MAX_SEGMENTS.
pub fn needs_merge(config: &IndexConfig) -> Result<bool> {
    Ok(list_segments(config)?.len() > MAX_SEGMENTS)
}

/// Write simhash.bin for a segment: flat array of u64, indexed by doc_id.
/// Slots for missing doc_ids are written as 0.
pub fn write_simhash(path: &Path, simhashes: &[(u32, u64)]) -> Result<()> {
    if simhashes.is_empty() {
        std::fs::write(path, &[])?;
        return Ok(());
    }

    let max_id = simhashes.iter().map(|(id, _)| *id).max().unwrap_or(0);
    let mut buf = vec![0u8; (max_id as usize + 1) * 8];
    for &(id, hash) in simhashes {
        let offset = id as usize * 8;
        buf[offset..offset + 8].copy_from_slice(&hash.to_le_bytes());
    }
    std::fs::write(path, &buf)?;
    Ok(())
}

/// Read simhash.bin into a flat Vec<u64> indexed by doc_id.
pub fn read_simhash(path: &Path) -> Result<Vec<u64>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(path)?;
    if bytes.len() % 8 != 0 {
        return Err(SearchError::CorruptIndex("simhash.bin size not multiple of 8".into()));
    }
    Ok(bytes
        .chunks_exact(8)
        .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
        .collect())
}
