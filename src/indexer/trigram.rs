use ahash::AHashMap;

use crate::indexer::schema::{DocId, PostingEntry, Trigram};

/// Accumulates trigram posting lists across multiple documents.
#[derive(Default)]
pub struct TrigramAccumulator {
    /// trigram → list of posting entries (one per document)
    pub postings: AHashMap<Trigram, Vec<PostingEntry>>,
}

impl TrigramAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Index all trigrams from a document's text.
    pub fn add_document(&mut self, doc_id: DocId, text: &str) {
        let positions = extract_trigrams_with_positions(text);
        for (trigram, pos_list) in positions {
            self.postings
                .entry(trigram)
                .or_default()
                .push(PostingEntry {
                    doc_id,
                    positions: pos_list,
                });
        }
    }

    /// Merge another accumulator's postings into this one (for parallel builds).
    pub fn merge(&mut self, other: TrigramAccumulator) {
        for (trigram, entries) in other.postings {
            self.postings.entry(trigram).or_default().extend(entries);
        }
    }

    pub fn trigram_count(&self) -> usize {
        self.postings.len()
    }
}

/// Extract all trigrams from text, returning a map of trigram → byte positions.
/// - Text is lowercased and whitespace is normalized before extraction.
/// - Positions are byte offsets in the *original* (pre-normalized) text.
pub fn extract_trigrams_with_positions(text: &str) -> AHashMap<Trigram, Vec<u32>> {
    let normalized = normalize_text(text);
    let bytes = normalized.as_bytes();
    let mut map: AHashMap<Trigram, Vec<u32>> = AHashMap::new();

    if bytes.len() < 3 {
        return map;
    }

    for i in 0..=(bytes.len() - 3) {
        let tg = Trigram([bytes[i], bytes[i + 1], bytes[i + 2]]);
        map.entry(tg).or_default().push(i as u32);
    }

    map
}

/// Extract just the set of trigrams (no positions) — used for query time.
pub fn extract_trigrams(text: &str) -> std::collections::HashSet<Trigram> {
    let normalized = normalize_text(text);
    let bytes = normalized.as_bytes();
    let mut set = std::collections::HashSet::new();

    if bytes.len() < 3 {
        return set;
    }

    for i in 0..=(bytes.len() - 3) {
        set.insert(Trigram([bytes[i], bytes[i + 1], bytes[i + 2]]));
    }

    set
}

/// Normalize text for trigram extraction:
/// - Lowercase
/// - Collapse runs of whitespace to single space
/// - Keep all other chars including punctuation (important for code)
pub fn normalize_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_space = false;

    for c in text.chars() {
        if c.is_whitespace() || c == '\n' || c == '\r' {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push_str(&c.to_lowercase().to_string());
            prev_space = false;
        }
    }

    out.trim().to_string()
}

/// Count how many query trigrams appear in a document's trigram set.
/// Returns a score in [0.0, 1.0]: matched / total_query_trigrams.
pub fn trigram_overlap_score(
    query_trigrams: &std::collections::HashSet<Trigram>,
    doc_posting_keys: &std::collections::HashSet<Trigram>,
) -> f64 {
    if query_trigrams.is_empty() {
        return 0.0;
    }
    let matched = query_trigrams.intersection(doc_posting_keys).count();
    matched as f64 / query_trigrams.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigram_extraction() {
        let tgs = extract_trigrams("hello");
        // "hello" → normalized "hello" → hel, ell, llo
        assert!(tgs.contains(&Trigram(*b"hel")));
        assert!(tgs.contains(&Trigram(*b"ell")));
        assert!(tgs.contains(&Trigram(*b"llo")));
        assert_eq!(tgs.len(), 3);
    }

    #[test]
    fn test_whitespace_normalization() {
        let normalized = normalize_text("Hello  World\nFoo");
        assert_eq!(normalized, "hello world foo");
    }

    #[test]
    fn test_trigram_overlap() {
        let query = extract_trigrams("helo"); // "helo" → hel, elo
        let doc = extract_trigrams("hello world");
        let score = trigram_overlap_score(&query, &doc);
        // "hel" matches, "elo" does not (doc has "ell","llo")
        assert!(score > 0.0);
        assert!(score <= 1.0);
    }

    #[test]
    fn test_accumulator() {
        let mut acc = TrigramAccumulator::new();
        acc.add_document(0, "hello world");
        acc.add_document(1, "hello rust");
        // "hel", "ell", "llo" appear in both docs
        let hel = Trigram(*b"hel");
        assert_eq!(acc.postings[&hel].len(), 2);
    }
}
