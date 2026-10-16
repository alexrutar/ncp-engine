use std::env;
use std::hint::black_box;
use std::sync::Arc;
use std::thread::available_parallelism;
use std::time::{Duration, Instant};

use criterion::{Bencher, BenchmarkId, Criterion, Throughput};
use ncp_engine::Nucleo;
use ncp_matcher::Matcher;
use ncp_matcher::pattern::{CaseMatching, Normalization, Pattern};

mod support;
use support::{Corpus, DATASETS, Dataset};

// The preparation script rejects NULs, so this cannot match a fixture candidate.
const RESET_QUERY: &str = "\0";

fn matcher_benches(c: &mut Criterion, dataset: &Dataset, corpus: &Corpus) {
    use CaseMatching::{Ignore, Respect, Smart};

    for (name, case, normalization, prefer_prefix) in [
        ("smart", Smart, Normalization::Smart, false),
        ("ignore_case", Ignore, Normalization::Never, false),
        ("respect_case", Respect, Normalization::Never, false),
        ("smart_prefix", Smart, Normalization::Smart, true),
    ] {
        let mut group = c.benchmark_group(format!("matcher/{}/{name}", dataset.name));
        group.throughput(Throughput::Elements(corpus.items.len() as u64));
        let mut matcher = Matcher::new(corpus.config.clone());
        matcher.config.prefer_prefix = prefer_prefix;
        for &(label, query) in dataset.queries {
            let pattern = Pattern::parse(query, case, normalization);
            group.bench_function(BenchmarkId::new(label, corpus.items.len()), |b| {
                b.iter(|| {
                    for item in &corpus.items {
                        black_box(pattern.score(black_box(item.slice(..)), &mut matcher));
                    }
                });
            });
        }
        group.finish();
    }
}

fn search(engine: &mut Nucleo<usize>, query: &str, append: bool) {
    engine
        .pattern
        .reparse(0, query, CaseMatching::Smart, Normalization::Smart, append);
    // Include completion and snapshot publication, even if a tick times out.
    while engine.tick(1000).running {}
}

fn bench_search(
    b: &mut Bencher,
    engine: &mut Nucleo<usize>,
    query: &str,
    append: bool,
    prepare: impl Fn(&mut Nucleo<usize>),
) {
    b.iter_custom(|iters| {
        let mut elapsed = Duration::ZERO;
        for _ in 0..iters {
            prepare(engine);
            let start = Instant::now();
            search(engine, black_box(query), append);
            elapsed += start.elapsed();
            black_box(engine.snapshot().matches());
        }
        elapsed
    });
}

fn inject(engine: &Nucleo<usize>, corpus: &Corpus) {
    engine
        .injector()
        .extend(0..corpus.items.len(), |&index, columns| {
            columns[0] = corpus.items[index].clone();
        });
}

fn engine_benches(c: &mut Criterion, dataset: &Dataset, corpus: &Corpus, threads: usize) {
    let mut engine = Nucleo::new(corpus.config.clone(), Arc::new(|| {}), Some(threads), 1);
    inject(&engine, corpus);
    search(&mut engine, RESET_QUERY, false);

    let mut group = c.benchmark_group(format!("engine/{}/threads_{threads}", dataset.name));
    group.throughput(Throughput::Elements(corpus.items.len() as u64));
    for &(label, query) in dataset.queries {
        group.bench_function(BenchmarkId::new(label, corpus.items.len()), |b| {
            bench_search(b, &mut engine, query, false, |engine| {
                // Force full rescoring instead of an unchanged-query no-op.
                search(engine, RESET_QUERY, false);
            });
        });
    }
    group.finish();

    let mut group = c.benchmark_group(format!("editing/{}/threads_{threads}", dataset.name));
    // Report edit latency: append searches only the preceding query's matches.
    for (label, before, after, append) in [
        ("append", dataset.short_query, dataset.long_query, true),
        ("backspace", dataset.long_query, dataset.short_query, false),
    ] {
        group.bench_function(BenchmarkId::new(label, corpus.items.len()), |b| {
            bench_search(b, &mut engine, after, append, |engine| {
                search(engine, before, false);
            });
        });
    }
    group.finish();

    let mut group = c.benchmark_group(format!("new_items/{}/threads_{threads}", dataset.name));
    group.throughput(Throughput::Elements(corpus.items.len() as u64));
    for &(label, query) in dataset.queries {
        if !matches!(label, "broad" | "selective" | "no_match") {
            continue;
        }
        group.bench_function(BenchmarkId::new(label, corpus.items.len()), |b| {
            bench_search(b, &mut engine, query, false, |engine| {
                engine.restart(true);
                // Finish dropping the old generation before timing new items.
                while engine.tick(1000).running {}
                inject(engine, corpus);
            });
        });
    }
    group.finish();
}

fn matching(c: &mut Criterion) {
    let size = env::var("NCP_BENCH_SIZE").unwrap_or_else(|_| "100000".to_owned());
    assert!(
        matches!(size.as_str(), "10000" | "100000" | "full"),
        "NCP_BENCH_SIZE must be 10000, 100000, or full (full Chromium, 100k per Wikipedia)"
    );
    let mut threads: Vec<usize> = match env::var("NCP_BENCH_THREADS") {
        Ok(value) => value
            .split(',')
            .map(|s| {
                s.trim()
                    .parse()
                    .expect("NCP_BENCH_THREADS must be comma-separated positive integers")
            })
            .collect(),
        Err(_) => vec![1, available_parallelism().map_or(1, usize::from).min(8)],
    };
    assert!(threads.iter().all(|&count| count > 0));
    threads.sort_unstable();
    threads.dedup();
    for dataset in DATASETS {
        let corpus = dataset.load(&size);
        matcher_benches(c, dataset, &corpus);
        for &threads in &threads {
            engine_benches(c, dataset, &corpus, threads);
        }
    }
}

criterion::criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(20)
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(2));
    targets = matching
}
criterion::criterion_main!(benches);
