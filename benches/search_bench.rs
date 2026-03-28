use criterion::{criterion_group, criterion_main, Criterion};
use contextgrep::indexer::trigram::extract_trigrams;

fn bench_extract_query_trigrams(c: &mut Criterion) {
    c.bench_function("extract_query_trigrams", |b| {
        b.iter(|| extract_trigrams("purchase agreement contract terms"));
    });
}

criterion_group!(benches, bench_extract_query_trigrams);
criterion_main!(benches);
