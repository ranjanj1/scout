use std::path::PathBuf;

use scout::config::IndexConfig;
use scout::indexer::pipeline::run_index;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn test_index_creates_searchindex() {
    let tmp = tempfile::tempdir().unwrap();
    let config = IndexConfig::new(tmp.path().join(".searchindex"));

    run_index(&config, &fixtures_dir(), false).unwrap();

    assert!(config.root.exists(), ".searchindex/ should exist");
    assert!(config.docstore_path.exists(), "docstore.redb should exist");
    assert!(config.segments_dir.exists(), "segments/ should exist");
}

#[test]
fn test_index_detects_no_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let config = IndexConfig::new(tmp.path().join(".searchindex"));

    let stats1 = run_index(&config, &fixtures_dir(), false).unwrap();
    assert!(stats1.added > 0, "First run should add documents");

    let stats2 = run_index(&config, &fixtures_dir(), false).unwrap();
    assert_eq!(stats2.added, 0, "Second run should add nothing (no changes)");
    assert!(stats2.skipped > 0, "Second run should skip unchanged files");
}

#[test]
fn test_index_doc_count() {
    let tmp = tempfile::tempdir().unwrap();
    let config = IndexConfig::new(tmp.path().join(".searchindex"));

    let stats = run_index(&config, &fixtures_dir(), false).unwrap();

    // We have sample.txt, sample.md, sample.rs in fixtures/
    assert!(stats.added >= 3, "Should have indexed at least 3 fixture files");
}
