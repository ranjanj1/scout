use std::collections::HashSet;
use std::path::PathBuf;

use contextgrep::config::IndexConfig;
use contextgrep::indexer::pipeline::run_index;
use contextgrep::indexer::trigram::extract_trigrams;
use contextgrep::search::query::QueryNode;
use contextgrep::search::scorer::{rank, score, ScoredDoc, ScoringInput, ScoringWeights};
use contextgrep::storage::mmap::PostingsReader;
use contextgrep::storage::segment::list_segments;
use contextgrep::storage::store::DocStore;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn setup_index() -> (tempfile::TempDir, IndexConfig) {
    let tmp = tempfile::tempdir().unwrap();
    let config = IndexConfig::new(tmp.path().join(".searchindex"));
    run_index(&config, &fixtures_dir(), false).unwrap();
    (tmp, config)
}

#[test]
fn test_search_finds_agreement() {
    let (_tmp, config) = setup_index();
    let store = DocStore::open(&config.docstore_path).unwrap();
    let segments = list_segments(&config).unwrap();

    let query = "agreement";
    let query_trigrams = extract_trigrams(query);

    let mut found_paths = Vec::new();
    for seg in &segments {
        if !seg.postings_path.exists() {
            continue;
        }
        let reader = PostingsReader::open(&seg.postings_path).unwrap();
        for tg in &query_trigrams {
            for doc_id in reader.doc_ids_for_trigram(tg) {
                if let Ok(Some(doc)) = store.get_doc(doc_id) {
                    found_paths.push(doc.path);
                }
            }
        }
    }

    found_paths.dedup();
    let paths_str: Vec<String> = found_paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();

    assert!(
        paths_str.iter().any(|p| p.contains("sample.txt") || p.contains("sample.md")),
        "Expected 'agreement' to match sample.txt or sample.md, got: {:?}",
        paths_str
    );
}

#[test]
fn test_search_no_results_for_gibberish() {
    let (_tmp, config) = setup_index();
    let segments = list_segments(&config).unwrap();

    let query = "xyzqwertyuiopasdfghjklzxcvbnm123456";
    let query_trigrams = extract_trigrams(query);

    let mut any_found = false;
    for seg in &segments {
        if !seg.postings_path.exists() {
            continue;
        }
        let reader = PostingsReader::open(&seg.postings_path).unwrap();
        for tg in &query_trigrams {
            if !reader.doc_ids_for_trigram(tg).is_empty() {
                any_found = true;
            }
        }
    }

    assert!(!any_found, "Gibberish query should return no results");
}
