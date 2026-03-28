use xxhash_rust::xxh64::xxh64;

use crate::indexer::schema::DocId;

/// Compute a 64-bit SimHash fingerprint from document text.
///
/// Algorithm:
/// 1. Tokenize into overlapping word bigram shingles
/// 2. Hash each shingle with xxh64
/// 3. Accumulate a [i64; 64] bit-weight vector
/// 4. Binarize: bit i = 1 if V[i] > 0
pub fn compute_simhash(text: &str) -> u64 {
    let mut v: [i64; 64] = [0; 64];
    let words: Vec<&str> = text.split_whitespace().collect();

    if words.is_empty() {
        return 0;
    }

    // Use unigrams if text is very short, bigrams otherwise
    if words.len() < 3 {
        for word in &words {
            let normalized = word.to_lowercase();
            accumulate(&mut v, normalized.as_bytes(), 1);
        }
    } else {
        // Overlapping bigram shingles: "the quick", "quick brown", ...
        for window in words.windows(2) {
            let shingle = format!("{} {}", window[0].to_lowercase(), window[1].to_lowercase());
            accumulate(&mut v, shingle.as_bytes(), 1);
        }
    }

    // Binarize
    let mut fingerprint: u64 = 0;
    for (i, &weight) in v.iter().enumerate() {
        if weight > 0 {
            fingerprint |= 1u64 << i;
        }
    }
    fingerprint
}

/// Add a shingle's hash contribution to the bit-weight vector.
fn accumulate(v: &mut [i64; 64], bytes: &[u8], weight: i64) {
    let h = xxh64(bytes, 0);
    for i in 0..64 {
        if (h >> i) & 1 == 1 {
            v[i] += weight;
        } else {
            v[i] -= weight;
        }
    }
}

/// Compute Hamming distance between two SimHash fingerprints.
/// Range: [0, 64]. Lower = more similar.
#[inline(always)]
pub fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Find the most similar documents to `needle`, ranked by hamming distance ascending.
/// If `threshold` is Some(n), only returns docs within n bits distance.
/// If `threshold` is None, returns the top `limit` closest docs regardless of distance.
pub fn find_similar(
    needle: u64,
    simhashes: &[u64], // indexed by doc_id
    threshold: Option<u32>,
    limit: usize,
) -> Vec<(DocId, u32)> {
    let mut results: Vec<(DocId, u32)> = simhashes
        .iter()
        .enumerate()
        .filter_map(|(id, &hash)| {
            if hash == 0 {
                return None; // skip unindexed slots
            }
            let dist = hamming_distance(needle, hash);
            if threshold.map(|t| dist <= t).unwrap_or(true) {
                Some((id as DocId, dist))
            } else {
                None
            }
        })
        .collect();

    results.sort_by_key(|&(_, dist)| dist);
    results.truncate(limit);
    results
}

/// Group documents into clusters using Locality-Sensitive Hashing (LSH).
/// Splits the 64-bit fingerprint into `num_bands` bands.
/// Documents sharing any band value land in the same bucket.
///
/// Returns: Vec of clusters, each cluster is a Vec<DocId>.
pub fn lsh_clusters(simhashes: &[u64], num_bands: u32) -> Vec<Vec<DocId>> {
    use std::collections::HashMap;

    let band_bits = 64 / num_bands.max(1);
    let mask = (1u64 << band_bits) - 1;

    // bucket_key → set of doc_ids in that bucket
    let mut buckets: HashMap<(u32, u64), Vec<DocId>> = HashMap::new();

    for (id, &hash) in simhashes.iter().enumerate() {
        if hash == 0 {
            continue;
        }
        for band in 0..num_bands {
            let shifted = hash >> (band * band_bits);
            let band_val = shifted & mask;
            buckets
                .entry((band, band_val))
                .or_default()
                .push(id as DocId);
        }
    }

    // Deduplicate: a cluster is meaningful only if it has >= 2 docs
    let mut seen = std::collections::HashSet::new();
    let mut clusters: Vec<Vec<DocId>> = Vec::new();

    for (_, mut members) in buckets {
        if members.len() < 2 {
            continue;
        }
        members.sort();
        members.dedup();
        // Use sorted member list as dedup key
        let key: Vec<DocId> = members.clone();
        if seen.insert(key) {
            clusters.push(members);
        }
    }

    // Sort clusters by size descending
    clusters.sort_by(|a, b| b.len().cmp(&a.len()));
    clusters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simhash_near_duplicates() {
        let a = compute_simhash("The quick brown fox jumps over the lazy dog");
        let b = compute_simhash("The quick brown fox leaps over the lazy dog");
        let dist = hamming_distance(a, b);
        // Near-duplicates should differ by few bits
        assert!(dist < 15, "hamming distance was {}", dist);
    }

    #[test]
    fn test_simhash_unrelated() {
        let a = compute_simhash("The quick brown fox jumps over the lazy dog");
        let c = compute_simhash("Quarterly earnings report fiscal year 2025 revenue growth");
        let dist = hamming_distance(a, c);
        // Unrelated docs should differ by many bits
        assert!(dist > 15, "hamming distance was {}", dist);
    }

    #[test]
    fn test_hamming_distance_identical() {
        let a = compute_simhash("hello world");
        assert_eq!(hamming_distance(a, a), 0);
    }

    #[test]
    fn test_find_similar() {
        let a = compute_simhash("contract agreement legal terms");
        let b = compute_simhash("contract agreement legal terms conditions");
        let c = compute_simhash("quarterly earnings financial report");
        let hashes = vec![a, b, c];

        let results = find_similar(a, &hashes, Some(10), 5);
        // a itself (distance 0) and b (similar) should be returned
        assert!(!results.is_empty());
        assert_eq!(results[0].1, 0); // identical to itself
    }
}
