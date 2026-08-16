use std::sync::Arc;
use std::thread::available_parallelism;

use criterion::{BatchSize, BenchmarkId, Criterion};
use ncp_engine::{Config, Nucleo};
use rayon::prelude::*;

const TINY_LINE_COUNT: u32 = 100;
const SMALL_LINE_COUNT: u32 = 1_000;
const MEDIUM_LINE_COUNT: u32 = 50_000;
const LARGE_LINE_COUNT: u32 = 500_000;

#[derive(Clone, Copy)]
enum LowerBound {
    MuchSmaller,
    Exact,
}

struct SizeHint<I> {
    iter: I,
    lower_bound: LowerBound,
}

impl<I> SizeHint<I> {
    fn new(iter: I, lower_bound: LowerBound) -> Self {
        Self { iter, lower_bound }
    }
}

impl<I: Iterator> Iterator for SizeHint<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_, upper) = self.iter.size_hint();
        let remaining = upper.expect("the benchmark's underlying iterator has an exact size");
        let lower = match self.lower_bound {
            LowerBound::MuchSmaller => remaining / 100,
            LowerBound::Exact => remaining,
        };
        (lower, Some(remaining))
    }
}

fn grow_injector(c: &mut Criterion) {
    let mut group = c.benchmark_group("grow_injector");
    for line_count in line_counts() {
        let lines = random_lines(line_count);

        group.bench_with_input(BenchmarkId::new("push", line_count), &lines, |b, lines| {
            let mut nucleo = new_nucleo();
            b.iter_batched(
                || {
                    nucleo.restart(false);
                    nucleo.injector()
                },
                |injector| {
                    for line in lines {
                        injector.push(Arc::clone(line), |_, _| {});
                    }
                },
                BatchSize::SmallInput,
            );
        });

        for (name, lower_bound) in [
            ("extend_lower_hint_much_smaller", LowerBound::MuchSmaller),
            ("extend_lower_hint_exact", LowerBound::Exact),
        ] {
            group.bench_with_input(BenchmarkId::new(name, line_count), &lines, |b, lines| {
                let mut nucleo = new_nucleo();
                b.iter_batched(
                    || {
                        nucleo.restart(false);
                        nucleo.injector()
                    },
                    |injector| {
                        let values = SizeHint::new(lines.iter().cloned(), lower_bound);
                        injector.extend(values, |_, _| {});
                    },
                    BatchSize::SmallInput,
                );
            });
        }
    }
    group.finish();
}

fn grow_injector_threaded(c: &mut Criterion) {
    let mut group = c.benchmark_group("grow_injector_push_threaded");
    for line_count in line_counts() {
        let lines = random_lines(line_count);
        let thread_count = usize::from(available_parallelism().unwrap());
        let batch_size = lines.len().div_ceil(thread_count).max(1);

        group.bench_with_input(BenchmarkId::new("push", line_count), &lines, |b, lines| {
            let mut nucleo = new_nucleo();
            b.iter_batched(
                || {
                    nucleo.restart(false);
                    nucleo.injector()
                },
                |injector| {
                    lines.par_chunks(batch_size).for_each(|batch| {
                        for line in batch {
                            injector.push(Arc::clone(line), |_, _| {});
                        }
                    });
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn new_nucleo() -> Nucleo<Arc<str>> {
    Nucleo::new(Config::DEFAULT, Arc::new(|| {}), Some(1), 1)
}

fn line_counts() -> [u32; 4] {
    [
        TINY_LINE_COUNT,
        SMALL_LINE_COUNT,
        MEDIUM_LINE_COUNT,
        LARGE_LINE_COUNT,
    ]
}

fn random_lines(count: u32) -> Vec<Arc<str>> {
    let mut state = u64::from(count);
    (0..count)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            Arc::from(format!("benchmark line {state:016x}"))
        })
        .collect()
}

criterion::criterion_group!(benches, grow_injector, grow_injector_threaded);
criterion::criterion_main!(benches);
