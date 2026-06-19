//! Criterion benchmarks. Real baselines are established at M2 (after chunking);
//! at M0 this just exercises the tick loop so the harness compiles and runs.

use criterion::{criterion_group, criterion_main, Criterion};
use pwdr_core::Grid;

fn bench_step(c: &mut Criterion) {
    c.bench_function("step_256_empty", |b| {
        let mut g = Grid::new(256, 256, 1);
        b.iter(|| {
            g.step();
            std::hint::black_box(&g);
        });
    });
}

criterion_group!(benches, bench_step);
criterion_main!(benches);
