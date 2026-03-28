use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use memmap2::Mmap;

use crate::error::{Result, SearchError};
use crate::indexer::schema::{DocId, PostingEntry, Trigram};
use crate::indexer::trigram::TrigramAccumulator;

// File header magic bytes
const MAGIC: &[u8; 4] = b"TRGM";
const VERSION: u32 = 1;

// Trigram index entry stride (bytes): 3 (trigram) + 1 (pad) + 4 (pad) + 8 (data_offset) + 4 (doc_count) + 4 (pad) = 24
const INDEX_ENTRY_STRIDE: usize = 24;

/// Write a `postings.trgm` file from an accumulator.
///
/// File layout:
/// - 32-byte header
/// - Sorted trigram index (fixed 24-byte stride entries)
/// - Variable-length data section (delta-coded varint posting lists)
pub fn write_postings(acc: &TrigramAccumulator, path: &Path) -> Result<()> {
    // Sort trigrams for binary search
    let mut sorted: BTreeMap<Trigram, &Vec<PostingEntry>> = BTreeMap::new();
    for (tg, entries) in &acc.postings {
        sorted.insert(*tg, entries);
    }

    let trigram_count = sorted.len() as u32;

    // Build data section first (we need offsets)
    let mut data_section: Vec<u8> = Vec::new();
    let mut index_entries: Vec<(Trigram, u64, u32)> = Vec::new(); // (trigram, data_offset, doc_count)

    for (trigram, entries) in &sorted {
        let data_offset = data_section.len() as u64;
        let doc_count = entries.len() as u32;

        encode_posting_list(entries, &mut data_section);
        index_entries.push((*trigram, data_offset, doc_count));
    }

    // Header layout (32 bytes):
    // [0..4]   magic
    // [4..8]   version
    // [8..12]  trigram_count
    // [12..16] reserved
    // [16..24] index_offset (always 32)
    // [24..32] data_offset (32 + trigram_count * INDEX_ENTRY_STRIDE)
    let index_offset: u64 = 32;
    let data_offset: u64 = index_offset + (trigram_count as u64 * INDEX_ENTRY_STRIDE as u64);

    let file = File::create(path).map_err(|e| SearchError::Mmap(e.to_string()))?;
    let mut writer = BufWriter::new(file);

    // Header
    writer.write_all(MAGIC)?;
    writer.write_all(&VERSION.to_le_bytes())?;
    writer.write_all(&trigram_count.to_le_bytes())?;
    writer.write_all(&0u32.to_le_bytes())?; // reserved
    writer.write_all(&index_offset.to_le_bytes())?;
    writer.write_all(&data_offset.to_le_bytes())?;

    // Trigram index entries (24 bytes each)
    for (trigram, rel_data_offset, doc_count) in &index_entries {
        let abs_data_offset = data_offset + rel_data_offset;
        writer.write_all(&trigram.0)?;      // 3 bytes
        writer.write_all(&[0u8; 5])?;       // 5 bytes padding → align to 8
        writer.write_all(&abs_data_offset.to_le_bytes())?;  // 8 bytes
        writer.write_all(&doc_count.to_le_bytes())?;        // 4 bytes
        writer.write_all(&[0u8; 4])?;       // 4 bytes padding → total 24
    }

    // Data section
    writer.write_all(&data_section)?;
    writer.flush()?;

    Ok(())
}

/// Memory-mapped reader for `postings.trgm`.
pub struct PostingsReader {
    mmap: Mmap,
    trigram_count: u32,
    index_offset: usize,
    #[allow(dead_code)]
    data_offset: usize,
}

impl PostingsReader {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|e| SearchError::Mmap(e.to_string()))?;
        let mmap = unsafe { Mmap::map(&file) }.map_err(|e| SearchError::Mmap(e.to_string()))?;

        if mmap.len() < 32 {
            return Err(SearchError::CorruptIndex("postings.trgm too small".into()));
        }
        if &mmap[0..4] != MAGIC {
            return Err(SearchError::CorruptIndex("bad magic bytes".into()));
        }

        let trigram_count = u32::from_le_bytes(mmap[8..12].try_into().unwrap());
        let index_offset = u64::from_le_bytes(mmap[16..24].try_into().unwrap()) as usize;
        let data_offset = u64::from_le_bytes(mmap[24..32].try_into().unwrap()) as usize;

        Ok(PostingsReader {
            mmap,
            trigram_count,
            index_offset,
            data_offset,
        })
    }

    /// Look up the posting list for a trigram.
    pub fn lookup(&self, trigram: &Trigram) -> Option<Vec<PostingEntry>> {
        let (data_offset, doc_count) = self.binary_search(trigram)?;
        let entries = self.decode_posting_list(data_offset, doc_count);
        Some(entries)
    }

    /// Get the set of all doc_ids that contain a trigram.
    pub fn doc_ids_for_trigram(&self, trigram: &Trigram) -> Vec<DocId> {
        self.lookup(trigram)
            .map(|entries| entries.iter().map(|e| e.doc_id).collect())
            .unwrap_or_default()
    }

    /// Binary search the fixed-stride index for a trigram.
    /// Returns (absolute_data_offset, doc_count) or None.
    fn binary_search(&self, trigram: &Trigram) -> Option<(usize, usize)> {
        let count = self.trigram_count as usize;
        if count == 0 {
            return None;
        }

        let mut lo = 0usize;
        let mut hi = count;

        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let entry_start = self.index_offset + mid * INDEX_ENTRY_STRIDE;
            let entry_tg = &self.mmap[entry_start..entry_start + 3];

            match entry_tg.cmp(trigram.as_bytes().as_slice()) {
                std::cmp::Ordering::Equal => {
                    let off_start = entry_start + 8;
                    let data_offset =
                        u64::from_le_bytes(self.mmap[off_start..off_start + 8].try_into().unwrap())
                            as usize;
                    let count_start = off_start + 8;
                    let doc_count =
                        u32::from_le_bytes(self.mmap[count_start..count_start + 4].try_into().unwrap())
                            as usize;
                    return Some((data_offset, doc_count));
                }
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
            }
        }
        None
    }

    fn decode_posting_list(&self, abs_offset: usize, doc_count: usize) -> Vec<PostingEntry> {
        let mut entries = Vec::with_capacity(doc_count);
        let mut cursor = abs_offset;
        let mut prev_doc_id: u32 = 0;

        for _ in 0..doc_count {
            if cursor >= self.mmap.len() {
                break;
            }
            let (delta, n) = read_varint(&self.mmap[cursor..]);
            cursor += n;
            let doc_id = prev_doc_id + delta;
            prev_doc_id = doc_id;

            let (pos_count, n) = read_varint(&self.mmap[cursor..]);
            cursor += n;

            let mut positions = Vec::with_capacity(pos_count as usize);
            let mut prev_pos: u32 = 0;
            for _ in 0..pos_count {
                let (pos_delta, n) = read_varint(&self.mmap[cursor..]);
                cursor += n;
                let pos = prev_pos + pos_delta;
                prev_pos = pos;
                positions.push(pos);
            }

            entries.push(PostingEntry { doc_id, positions });
        }
        entries
    }
}

// ---- Varint encoding/decoding (7-bit groups, MSB = continuation) ----

fn encode_varint(mut value: u32, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            break;
        } else {
            out.push(byte | 0x80);
        }
    }
}

fn read_varint(data: &[u8]) -> (u32, usize) {
    let mut result: u32 = 0;
    let mut shift = 0;
    let mut i = 0;
    loop {
        if i >= data.len() {
            break;
        }
        let byte = data[i];
        i += 1;
        result |= ((byte & 0x7F) as u32) << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            break;
        }
    }
    (result, i)
}

fn encode_posting_list(entries: &[PostingEntry], out: &mut Vec<u8>) {
    // Entries must be sorted by doc_id for delta coding
    let mut sorted = entries.to_vec();
    sorted.sort_by_key(|e| e.doc_id);

    let mut prev_doc_id: u32 = 0;
    for entry in sorted {
        encode_varint(entry.doc_id - prev_doc_id, out);
        prev_doc_id = entry.doc_id;

        encode_varint(entry.positions.len() as u32, out);
        let mut prev_pos: u32 = 0;
        let mut sorted_pos = entry.positions.clone();
        sorted_pos.sort();
        for pos in sorted_pos {
            encode_varint(pos - prev_pos, out);
            prev_pos = pos;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::trigram::TrigramAccumulator;
    use tempfile::NamedTempFile;

    #[test]
    fn test_varint_roundtrip() {
        for val in [0u32, 1, 127, 128, 255, 16383, 16384, u32::MAX / 2] {
            let mut buf = Vec::new();
            encode_varint(val, &mut buf);
            let (decoded, _) = read_varint(&buf);
            assert_eq!(decoded, val, "failed for {}", val);
        }
    }

    #[test]
    fn test_postings_roundtrip() {
        let mut acc = TrigramAccumulator::new();
        acc.add_document(0, "hello world");
        acc.add_document(1, "hello rust");

        let tmp = NamedTempFile::new().unwrap();
        write_postings(&acc, tmp.path()).unwrap();

        let reader = PostingsReader::open(tmp.path()).unwrap();
        let hel = Trigram(*b"hel");
        let entries = reader.lookup(&hel).unwrap();
        // Both docs contain "hel"
        assert_eq!(entries.len(), 2);
    }
}
