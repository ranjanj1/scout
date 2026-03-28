use std::collections::HashSet;

use crate::indexer::schema::{DocId, DocRecord, StructuralMeta, Trigram};
use crate::search::filters::field_match_score;
use crate::search::query::QueryNode;

/// Weights for the scoring formula. Configurable in future via weights.toml.
#[derive(Debug, Clone)]
pub struct ScoringWeights {
    pub trigram: f64,
    pub proximity: f64,
    pub recency: f64,
    pub structural: f64,
    pub title: f64,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        ScoringWeights {
            trigram: 0.45,
            proximity: 0.20,
            recency: 0.10,
            structural: 0.20,
            title: 0.05,
        }
    }
}

/// All signals needed to compute a score for one document.
pub struct ScoringInput<'a> {
    pub doc: &'a DocRecord,
    pub meta: &'a StructuralMeta,
    pub query_text: &'a str,
    pub query_trigrams: &'a HashSet<Trigram>,
    pub query_node: &'a QueryNode,
    pub doc_trigrams: HashSet<Trigram>, // trigrams present in this doc
    pub min_position_span: Option<u32>, // from proximity module
    /// Total number of positions across all matched query trigrams (term frequency signal).
    /// Higher = the query terms appear more often in this document.
    pub total_match_positions: usize,
}

/// Compute the weighted composite score for a document.
pub fn score(input: &ScoringInput, weights: &ScoringWeights) -> f64 {
    let trgm = trigram_score(input.query_trigrams, &input.doc_trigrams, input.total_match_positions);
    let prox = proximity_score(input.min_position_span);
    let rec = recency_score(input.doc);
    let st = field_match_score(input.query_node, input.doc, input.meta);
    let title = title_boost(input.query_text, input.doc);

    weights.trigram * trgm
        + weights.proximity * prox
        + weights.recency * rec
        + weights.structural * st
        + weights.title * title
}

/// TF-weighted trigram score:
/// - Base: matched trigrams / total query trigrams (coverage)
/// - Boost: log(1 + total_positions) to reward documents with more occurrences
/// Result is normalized to [0, 1] range.
fn trigram_score(query: &HashSet<Trigram>, doc: &HashSet<Trigram>, total_positions: usize) -> f64 {
    if query.is_empty() {
        return 0.0;
    }
    let matched = query.intersection(doc).count();
    let coverage = matched as f64 / query.len() as f64;

    // Frequency boost: log-scaled so 2 occurrences beats 1, but 100 doesn't
    // dominate over relevance. Capped at 1.0 via tanh-like normalization.
    let tf_boost = if total_positions > 0 {
        (1.0 + (total_positions as f64).ln()) / (1.0 + (total_positions as f64).ln() + 1.0)
    } else {
        0.0
    };

    // Blend coverage (primary) with frequency boost (secondary)
    coverage * 0.7 + tf_boost * 0.3
}

/// Proximity score based on minimum span covering all matched terms.
/// Higher score = terms appear close together in the document.
fn proximity_score(min_span: Option<u32>) -> f64 {
    match min_span {
        None => 0.0,
        Some(0) => 1.0,
        Some(span) => 1.0 / (1.0 + (span as f64).ln().max(0.0)),
    }
}

/// Exponential decay based on file modification time.
/// Half-life = 180 days. Recent docs score closer to 1.0.
fn recency_score(doc: &DocRecord) -> f64 {
    const HALF_LIFE_SECS: f64 = 180.0 * 86_400.0; // 180 days in seconds

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let mtime = doc
        .mtime
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let age = (now - mtime).max(0.0);
    (-age / HALF_LIFE_SECS).exp()
}

/// 1.0 if any query word appears in the document title, else 0.0.
fn title_boost(query_text: &str, doc: &DocRecord) -> f64 {
    let Some(title) = &doc.title else {
        return 0.0;
    };
    let title_lower = title.to_lowercase();
    for word in query_text.split_whitespace() {
        if title_lower.contains(&word.to_lowercase()) {
            return 1.0;
        }
    }
    0.0
}

/// A scored search result ready for ranking.
#[derive(Debug, Clone)]
pub struct ScoredDoc {
    pub doc_id: DocId,
    pub score: f64,
    pub path: std::path::PathBuf,
    pub snippet: String,
    pub doc_type: Option<String>,
}

impl ScoredDoc {
    pub fn new(doc: &DocRecord, meta: &StructuralMeta, score: f64) -> Self {
        ScoredDoc {
            doc_id: doc.id,
            score,
            path: doc.path.clone(),
            snippet: doc.snippet.clone(),
            doc_type: meta.doc_type.clone(),
        }
    }
}

/// Sort results by score descending, truncate to limit.
pub fn rank(mut results: Vec<ScoredDoc>, limit: usize) -> Vec<ScoredDoc> {
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    results
}
