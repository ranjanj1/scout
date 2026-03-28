use std::path::PathBuf;

use contextgrep::config::IndexConfig;
use contextgrep::indexer::pipeline::run_index;
use contextgrep::search::filters::matches_filters;
use contextgrep::search::query::parse_query;
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
fn test_query_type_contract() {
    let (_tmp, config) = setup_index();
    let store = DocStore::open(&config.docstore_path).unwrap();
    let all = store.all_docs().unwrap();

    let query = parse_query("type:contract").unwrap();

    let matches: Vec<_> = all
        .iter()
        .filter_map(|(id, _)| {
            let doc = store.get_doc(*id).ok()??;
            let meta = store.get_metadata(*id).ok()??;
            if matches_filters(&query, &doc, &meta) {
                Some(doc.path)
            } else {
                None
            }
        })
        .collect();

    // sample.txt contains "This agreement is entered into" → should be classified as contract
    assert!(
        matches.iter().any(|p| p.to_string_lossy().contains("sample.txt")),
        "sample.txt should match type:contract, got: {:?}",
        matches
    );
}

#[test]
fn test_query_amount_filter() {
    let (_tmp, config) = setup_index();
    let store = DocStore::open(&config.docstore_path).unwrap();
    let all = store.all_docs().unwrap();

    // sample.txt has amounts: $50,000 and $150,000
    let query = parse_query("amount:>100000").unwrap();

    let matches: Vec<_> = all
        .iter()
        .filter_map(|(id, _)| {
            let doc = store.get_doc(*id).ok()??;
            let meta = store.get_metadata(*id).ok()??;
            if matches_filters(&query, &doc, &meta) {
                Some(doc.path)
            } else {
                None
            }
        })
        .collect();

    assert!(
        matches.iter().any(|p| p.to_string_lossy().contains("sample.txt")
            || p.to_string_lossy().contains("sample.md")),
        "Expected a fixture to match amount:>100000"
    );
}
