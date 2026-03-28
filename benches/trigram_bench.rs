use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use scout::indexer::trigram::{extract_trigrams, extract_trigrams_with_positions};

const SAMPLE_10KB: &str = include_str!("../tests/fixtures/sample.txt");

fn bench_extract_trigrams(c: &mut Criterion) {
    c.bench_with_input(
        BenchmarkId::new("extract_trigrams", "10kb"),
        &SAMPLE_10KB,
        |b, text| {
            b.iter(|| extract_trigrams(text));
        },
    );
}

fn bench_extract_with_positions(c: &mut Criterion) {
    c.bench_with_input(
        BenchmarkId::new("extract_trigrams_with_positions", "10kb"),
        &SAMPLE_10KB,
        |b, text| {
            b.iter(|| extract_trigrams_with_positions(text));
        },
    );
}

criterion_group!(benches, bench_extract_trigrams, bench_extract_with_positions);
criterion_main!(benches);
