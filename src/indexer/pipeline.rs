use std::path::Path;
use std::sync::Mutex;

use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;

use crate::config::IndexConfig;
use crate::error::Result;
use crate::indexer::schema::DocRecord;
use crate::indexer::simhash::compute_simhash;
use crate::indexer::trigram::TrigramAccumulator as TrigramAcc;
use crate::parser::{self, file_hash};
use crate::parser::walker::walk_directory;
use crate::storage::mmap::write_postings;
use crate::storage::segment::{new_segment, write_simhash};
use crate::storage::store::DocStore;

pub struct IndexStats {
    pub added: usize,
    pub skipped: usize,
    pub removed: usize,
    pub errors: usize,
}

/// Run the indexing pipeline on a directory.
///
/// - Discovers all indexable files via `walk_directory`
/// - Skips unchanged files (mtime + hash comparison)
/// - Parses new/changed files in parallel (rayon)
/// - Builds trigram posting lists + SimHash fingerprints
/// - Writes a new segment to `.searchindex/segments/NNNN/`
/// - Updates docstore.redb with all DocRecords
pub fn run_index(config: &IndexConfig, root: &Path, force_full: bool) -> Result<IndexStats> {
    config.ensure_dirs()?;

    let store = DocStore::open(&config.docstore_path)?;

    // --- Step 1: Walk directory ---
    eprintln!("Scanning {}...", root.display());
    let entries = walk_directory(root)?;
    eprintln!("Found {} indexable files.", entries.len());

    // --- Step 2: Determine what needs indexing ---
    let mut to_index = Vec::new();
    let mut skipped = 0;
    let mut walked_paths = std::collections::HashSet::new();

    for entry in &entries {
        walked_paths.insert(entry.path.clone());

        if force_full {
            to_index.push(entry);
            continue;
        }

        match store.doc_id_for_path(&entry.path)? {
            None => to_index.push(entry), // new file
            Some(id) => {
                if let Some(record) = store.get_doc(id)? {
                    // Check mtime first (cheap)
                    if record.mtime == entry.mtime {
                        skipped += 1;
                        continue;
                    }
                    // mtime changed — verify with hash
                    match file_hash(&entry.path) {
                        Ok(hash) if hash == record.file_hash => {
                            skipped += 1; // only mtime changed (e.g. touch), content unchanged
                            continue;
                        }
                        _ => to_index.push(entry), // content changed
                    }
                } else {
                    to_index.push(entry);
                }
            }
        }
    }

    // --- Step 3: Remove deleted files ---
    let all_stored = store.all_docs()?;
    let mut removed = 0;
    for (id, path) in &all_stored {
        if path.starts_with(root) && !walked_paths.contains(path) {
            store.remove_doc(*id, path)?;
            removed += 1;
        }
    }

    if to_index.is_empty() {
        eprintln!("Nothing to index ({} unchanged, {} removed).", skipped, removed);
        return Ok(IndexStats {
            added: 0,
            skipped,
            removed,
            errors: 0,
        });
    }

    eprintln!(
        "Indexing {} files ({} unchanged, {} removed)...",
        to_index.len(),
        skipped,
        removed
    );

    // --- Step 4: Parse in parallel ---
    let progress = ProgressBar::new(to_index.len() as u64);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap(),
    );

    let global_acc = Mutex::new(TrigramAcc::new());
    let global_simhashes: Mutex<Vec<(u32, u64)>> = Mutex::new(Vec::new());
    let errors = Mutex::new(0usize);

    // Collect parsed results into a vec so we can write to store after
    let results: Vec<_> = to_index
        .par_iter()
        .filter_map(|entry| {
            let result = (|| -> Result<(DocRecord, crate::indexer::schema::StructuralMeta)> {
                let hash = file_hash(&entry.path)?;
                let parsed = parser::parse(entry)?;
                let doc_id = store.next_doc_id()?;
                let simhash = compute_simhash(&parsed.text);

                // Build thread-local trigram accumulator
                let mut local_acc = TrigramAcc::new();
                local_acc.add_document(doc_id, &parsed.text);

                // Merge into global
                {
                    let mut g = global_acc.lock().unwrap();
                    g.merge(local_acc);
                }
                {
                    let mut sh = global_simhashes.lock().unwrap();
                    sh.push((doc_id, simhash));
                }

                let snippet = parsed.text.chars().take(256).collect();
                let record = DocRecord {
                    id: doc_id,
                    path: entry.path.clone(),
                    mtime: entry.mtime,
                    file_hash: hash,
                    title: parsed.title,
                    snippet,
                    simhash,
                };

                Ok((record, parsed.metadata))
            })();

            progress.inc(1);

            match result {
                Ok(pair) => Some(pair),
                Err(e) => {
                    *errors.lock().unwrap() += 1;
                    eprintln!("  error: {}", e);
                    None
                }
            }
        })
        .collect();

    progress.finish_with_message("done");

    // --- Step 5: Write to docstore ---
    for (record, meta) in &results {
        store.put_doc(record, meta)?;
    }

    let error_count = *errors.lock().unwrap();
    let added = results.len();

    // --- Step 6: Write segment ---
    let segment = new_segment(config)?;
    let acc = global_acc.into_inner().unwrap();
    let simhashes = global_simhashes.into_inner().unwrap();

    write_postings(&acc, &segment.postings_path)?;
    write_simhash(&segment.simhash_path, &simhashes)?;

    eprintln!(
        "Done. Added {}, skipped {}, removed {}, errors {}.",
        added, skipped, removed, error_count
    );
    eprintln!("Index segment: {}", segment.dir.display());

    Ok(IndexStats {
        added,
        skipped,
        removed,
        errors: error_count,
    })
}
