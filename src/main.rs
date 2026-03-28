use std::collections::HashSet;
use std::path::Path;

use clap::Parser as ClapParser;

use contextgrep::cli::commands::{Cli, Command};
use contextgrep::cli::output::{render_error, render_results, SearchResult};
use contextgrep::config::{require_index, resolve_index};
use contextgrep::error::Result;
use contextgrep::indexer::pipeline::run_index;
use contextgrep::indexer::simhash::{compute_simhash, find_similar, lsh_clusters};
use contextgrep::indexer::trigram::extract_trigrams;
use contextgrep::parser::walker::{detect_kind, WalkEntry};
use contextgrep::search::filters::{matches_filters, parse_duration_cutoff};
use contextgrep::search::query::{parse_query, QueryNode};
use contextgrep::search::scorer::{rank, score, ScoredDoc, ScoringInput, ScoringWeights};
use contextgrep::storage::mmap::PostingsReader;
use contextgrep::storage::segment::{list_segments, read_simhash};
use contextgrep::storage::store::DocStore;

mod mcp;

fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        render_error(&e.to_string());
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Index { path, full } => {
            let config = resolve_index(cli.index)?;
            run_index(&config, &path, full)?;
        }

        Command::Search { query, limit, context_size, full_content } => {
            let config = require_index(cli.index)?;
            let store = DocStore::open(&config.docstore_path)?;
            let segments = list_segments(&config)?;
            let weights = ScoringWeights::default();

            let query_node = QueryNode::Term(query.to_lowercase());
            // Extract trigrams per-word and union them — avoids inter-word trigrams
            // like "n p" from "ranjan provide" which require exact adjacency.
            let query_trigrams: HashSet<_> = query
                .split_whitespace()
                .flat_map(|w| extract_trigrams(w).into_iter())
                .collect();

            let mut scored: Vec<ScoredDoc> = Vec::new();

            for seg in &segments {
                if !seg.postings_path.exists() {
                    continue;
                }
                let reader = PostingsReader::open(&seg.postings_path)?;

                // Retrieve candidate doc_ids from trigram intersection
                let candidate_ids = intersect_trigram_candidates(&reader, &query);

                for doc_id in candidate_ids {
                    let Some(doc) = store.get_doc(doc_id)? else { continue };
                    let Some(meta) = store.get_metadata(doc_id)? else { continue };

                    // Build doc trigram set + count total positions (term frequency)
                    let mut doc_trgms = HashSet::new();
                    let mut total_positions = 0usize;
                    for tg in &query_trigrams {
                        if let Some(entries) = reader.lookup(tg) {
                            if let Some(entry) = entries.iter().find(|e| e.doc_id == doc_id) {
                                doc_trgms.insert(*tg);
                                total_positions += entry.positions.len();
                            }
                        }
                    }

                    let input = ScoringInput {
                        doc: &doc,
                        meta: &meta,
                        query_text: &query,
                        query_trigrams: &query_trigrams,
                        query_node: &query_node,
                        doc_trigrams: doc_trgms,
                        min_position_span: None,
                        total_match_positions: total_positions,
                    };

                    let s = score(&input, &weights);
                    if s > 0.0 {
                        scored.push(ScoredDoc::new(&doc, &meta, s));
                    }
                }
            }

            let results = rank(scored, limit);
            render_results(
                &results
                    .iter()
                    .map(|r| SearchResult {
                        path: r.path.to_string_lossy().into_owned(),
                        score: r.score,
                        snippet: contextual_snippet(&r.path, &query, &r.snippet, context_size),
                        doc_type: r.doc_type.clone(),
                        content: if full_content {
                            read_full_content(&r.path, 8000)
                        } else {
                            None
                        },
                    })
                    .collect::<Vec<_>>(),
                &cli.output,
            );
        }

        Command::Query { dsl, limit } => {
            let config = require_index(cli.index)?;
            let store = DocStore::open(&config.docstore_path)?;
            let segments = list_segments(&config)?;
            let weights = ScoringWeights::default();

            let query_node = parse_query(&dsl).map_err(|e| e)?;
            let query_text = extract_text_from_query(&query_node);
            let query_trigrams: HashSet<_> = query_text
                .split_whitespace()
                .flat_map(|w| extract_trigrams(w).into_iter())
                .collect();

            let mut scored: Vec<ScoredDoc> = Vec::new();

            for seg in &segments {
                if !seg.postings_path.exists() {
                    continue;
                }
                let reader = PostingsReader::open(&seg.postings_path)?;
                let candidate_ids = if query_trigrams.is_empty() {
                    // Pure structural query — scan all docs
                    store.all_docs()?.into_iter().map(|(id, _)| id).collect()
                } else {
                    intersect_trigram_candidates(&reader, &query_text)
                };

                for doc_id in candidate_ids {
                    let Some(doc) = store.get_doc(doc_id)? else { continue };
                    let Some(meta) = store.get_metadata(doc_id)? else { continue };

                    // Apply hard structural filters first
                    if !matches_filters(&query_node, &doc, &meta) {
                        continue;
                    }

                    let mut doc_trgms = HashSet::new();
                    let mut total_positions = 0usize;
                    for tg in &query_trigrams {
                        if let Some(entries) = reader.lookup(tg) {
                            if let Some(entry) = entries.iter().find(|e| e.doc_id == doc_id) {
                                doc_trgms.insert(*tg);
                                total_positions += entry.positions.len();
                            }
                        }
                    }

                    let input = ScoringInput {
                        doc: &doc,
                        meta: &meta,
                        query_text: &query_text,
                        query_trigrams: &query_trigrams,
                        query_node: &query_node,
                        doc_trigrams: doc_trgms,
                        min_position_span: None,
                        total_match_positions: total_positions,
                    };

                    let s = score(&input, &weights);
                    scored.push(ScoredDoc::new(&doc, &meta, s));
                }
            }

            let results = rank(scored, limit);
            render_results(
                &results
                    .iter()
                    .map(|r| SearchResult {
                        path: r.path.to_string_lossy().into_owned(),
                        score: r.score,
                        snippet: contextual_snippet(&r.path, &query_text, &r.snippet, 120),
                        content: None,
                        doc_type: r.doc_type.clone(),
                    })
                    .collect::<Vec<_>>(),
                &cli.output,
            );
        }

        Command::Similar {
            file,
            threshold,
            limit,
        } => {
            let config = require_index(cli.index)?;
            let store = DocStore::open(&config.docstore_path)?;
            let segments = list_segments(&config)?;

            // Compute fingerprint of the query file
            let needle_hash = if let Some(doc_id) = store.doc_id_for_path(&file)? {
                store.get_doc(doc_id)?.map(|d| d.simhash).unwrap_or(0)
            } else {
                // File not indexed — compute on the fly
                let mtime = std::fs::metadata(&file)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                let kind = detect_kind(&file);
                let entry = WalkEntry {
                    path: file.clone(),
                    kind,
                    mtime,
                    size: 0,
                };
                let parsed = contextgrep::parser::parse(&entry)?;
                compute_simhash(&parsed.text)
            };

            let mut all_results: Vec<(u32, u32)> = Vec::new(); // (doc_id, hamming)
            for seg in &segments {
                let hashes = read_simhash(&seg.simhash_path)?;
                let mut seg_results = find_similar(needle_hash, &hashes, threshold, limit * 2);
                all_results.append(&mut seg_results);
            }

            if all_results.is_empty() {
                eprintln!("No documents indexed yet. Run `ds index <path>` first.");
                return Ok(());
            }

            all_results.sort_by_key(|&(_, dist)| dist);
            all_results.dedup_by_key(|(id, _)| *id);
            all_results.truncate(limit + 1); // +1 to account for self being filtered out

            // Canonicalize the query file path for reliable self-comparison
            let file_canonical = std::fs::canonicalize(&file).unwrap_or(file.clone());

            let results: Vec<SearchResult> = all_results
                .iter()
                .filter_map(|(doc_id, dist)| {
                    let doc = store.get_doc(*doc_id).ok()??;
                    // Skip the file itself using canonical path comparison
                    let doc_canonical = std::fs::canonicalize(&doc.path).unwrap_or(doc.path.clone());
                    if doc_canonical == file_canonical {
                        return None;
                    }
                    let meta = store.get_metadata(*doc_id).ok()??;
                    Some(SearchResult {
                        path: doc.path.to_string_lossy().into_owned(),
                        score: 1.0 - (*dist as f64 / 64.0),
                        snippet: format!("similarity: {:.0}%  (hamming distance: {})", (1.0 - *dist as f64 / 64.0) * 100.0, dist),
                        doc_type: meta.doc_type,
                        content: None,
                    })
                })
                .collect();

            render_results(&results, &cli.output);
        }

        Command::Recent { since, limit } => {
            let config = require_index(cli.index)?;
            let store = DocStore::open(&config.docstore_path)?;

            let cutoff = parse_duration_cutoff(&since).unwrap_or(0);
            let all = store.all_docs()?;

            let mut results: Vec<SearchResult> = all
                .iter()
                .filter_map(|(id, _)| {
                    let doc = store.get_doc(*id).ok()??;
                    let mtime_secs = doc
                        .mtime
                        .duration_since(std::time::UNIX_EPOCH)
                        .ok()?
                        .as_secs();
                    if mtime_secs < cutoff {
                        return None;
                    }
                    let meta = store.get_metadata(*id).ok()??;
                    Some((mtime_secs, SearchResult {
                        path: doc.path.to_string_lossy().into_owned(),
                        score: mtime_secs as f64,
                        snippet: doc.snippet,
                        doc_type: meta.doc_type,
                        content: None,
                    }))
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|(_, r)| r)
                .collect();

            // Sort by mtime descending (most recent first)
            results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
            results.truncate(limit);

            // Normalize scores to [0,1] for display
            let max_score = results.first().map(|r| r.score).unwrap_or(1.0);
            for r in &mut results {
                r.score /= max_score;
            }

            render_results(&results, &cli.output);
        }

        Command::Mcp => {
            crate::mcp::run_mcp_server(cli.index)?;
        }

        Command::Clusters { path, bits } => {
            let config = require_index(cli.index)?;
            let store = DocStore::open(&config.docstore_path)?;
            let segments = list_segments(&config)?;

            let mut all_hashes: Vec<(u32, u64)> = Vec::new(); // (doc_id, simhash)
            for seg in &segments {
                let hashes = read_simhash(&seg.simhash_path)?;
                for (id, hash) in hashes.iter().enumerate() {
                    if *hash != 0 {
                        all_hashes.push((id as u32, *hash));
                    }
                }
            }

            // Filter by path prefix if given
            let all_hashes: Vec<(u32, u64)> = if let Some(ref prefix) = path {
                all_hashes
                    .into_iter()
                    .filter(|(id, _)| {
                        store.get_doc(*id)
                            .ok()
                            .flatten()
                            .map(|d| d.path.starts_with(prefix))
                            .unwrap_or(false)
                    })
                    .collect()
            } else {
                all_hashes
            };

            // Build flat hash array indexed by position (not doc_id) for LSH
            let hash_array: Vec<u64> = all_hashes.iter().map(|(_, h)| *h).collect();
            let clusters = lsh_clusters(&hash_array, bits);

            println!("Found {} clusters (--bits={}):", clusters.len(), bits);
            for (i, cluster) in clusters.iter().enumerate().take(20) {
                println!("\nCluster {} ({} docs):", i + 1, cluster.len());
                for &local_idx in cluster.iter().take(5) {
                    if let Some(&(doc_id, _)) = all_hashes.get(local_idx as usize) {
                        if let Ok(Some(doc)) = store.get_doc(doc_id) {
                            println!("  {}", doc.path.display());
                        }
                    }
                }
                if cluster.len() > 5 {
                    println!("  ... and {} more", cluster.len() - 5);
                }
            }
        }
    }

    Ok(())
}

/// Find candidate doc_ids for a multi-word query.
///
/// Strategy: per-word trigram intersection + union across words.
/// - Each word's trigrams are intersected (doc must contain ALL trigrams of that word)
/// - Results across words are unioned (doc needs to match ANY word)
///
/// This avoids inter-word trigrams (e.g. "n p" from "ranjan provide") that would
/// require words to be adjacent, causing multi-word queries to return no results.
fn intersect_trigram_candidates(
    reader: &PostingsReader,
    query: &str,
) -> Vec<u32> {
    use contextgrep::indexer::trigram::extract_trigrams;

    let mut result: HashSet<u32> = HashSet::new();

    for word in query.split_whitespace() {
        if word.len() < 3 {
            continue; // trigrams need at least 3 chars
        }
        let word_trigrams = extract_trigrams(word);
        if word_trigrams.is_empty() {
            continue;
        }

        // Intersect: doc must contain ALL trigrams of this word
        let mut sets: Vec<HashSet<u32>> = word_trigrams
            .iter()
            .map(|tg| reader.doc_ids_for_trigram(tg).into_iter().collect())
            .collect();

        if sets.is_empty() {
            continue;
        }

        sets.sort_by_key(|s| s.len());
        let mut word_matches: HashSet<u32> = sets[0].clone();
        for set in &sets[1..] {
            word_matches.retain(|id| set.contains(id));
            if word_matches.is_empty() {
                break;
            }
        }

        // Union: any doc matching this word is a candidate
        result.extend(word_matches);
    }

    result.into_iter().collect()
}

/// Read full file content for RAG context (plain text only, truncated at max_chars).
fn read_full_content(path: &Path, max_chars: usize) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    if text.len() <= max_chars {
        Some(text)
    } else {
        // Truncate at a word boundary
        let truncated = &text[..max_chars];
        let cut = truncated.rfind(|c: char| c.is_whitespace()).unwrap_or(max_chars);
        Some(format!("{}...", &text[..cut]))
    }
}

/// Extract a contextual snippet around the first occurrence of any query term.
/// Falls back to the pre-computed doc snippet if the file can't be read.
fn contextual_snippet(path: &Path, query: &str, fallback: &str, context_chars: usize) -> String {
    let context = context_chars;

    let Ok(text) = std::fs::read_to_string(path) else {
        return fallback.to_string();
    };
    let text_lower = text.to_lowercase();

    // Find the first query word that appears in the document
    let match_pos = query
        .split_whitespace()
        .filter(|w| w.len() >= 3)
        .find_map(|word| text_lower.find(&word.to_lowercase()));

    let Some(pos) = match_pos else {
        return fallback.to_string();
    };

    // Extract surrounding context
    let start = text
        .char_indices()
        .map(|(i, _)| i)
        .filter(|&i| i <= pos)
        .rev()
        .nth(context)
        .unwrap_or(0);

    let end = text
        .char_indices()
        .map(|(i, _)| i)
        .filter(|&i| i >= pos)
        .nth(context)
        .unwrap_or(text.len());

    let snippet = text[start..end].trim().replace('\n', " ");
    let prefix = if start > 0 { "..." } else { "" };
    let suffix = if end < text.len() { "..." } else { "" };
    format!("{}{}{}", prefix, snippet, suffix)
}

/// Extract all text terms from a query node for trigram search.
fn extract_text_from_query(node: &QueryNode) -> String {
    match node {
        QueryNode::Term(t) => t.clone(),
        QueryNode::Phrase(p) => p.clone(),
        QueryNode::Field { .. } => String::new(),
        QueryNode::And(l, r) | QueryNode::Or(l, r) => {
            let lt = extract_text_from_query(l);
            let rt = extract_text_from_query(r);
            format!("{} {}", lt, rt).trim().to_string()
        }
        QueryNode::Not(inner) => extract_text_from_query(inner),
    }
}
