use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use contextgrep::indexer::simhash::{compute_simhash, find_similar};

const SAMPLE: &str = include_str!("../tests/fixtures/sample.txt");

fn bench_compute_simhash(c: &mut Criterion) {
    c.bench_with_input(
        BenchmarkId::new("compute_simhash", "fixture"),
        &SAMPLE,
        |b, text| {
            b.iter(|| compute_simhash(text));
        },
    );
}

fn bench_find_similar(c: &mut Criterion) {
    // Simulate 10k stored simhashes
    let base = compute_simhash(SAMPLE);
    let hashes: Vec<u64> = (0..10_000).map(|i| base ^ (i as u64)).collect();

    c.bench_function("find_similar_10k", |b| {
        b.iter(|| find_similar(base, &hashes, 8, 10));
    });
}

criterion_group!(benches, bench_compute_simhash, bench_find_similar);
criterion_main!(benches);
