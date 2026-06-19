//! Criterion baselines (M2). Two regimes per the perf doctrine:
//!   * fully active — every chunk awake, maximal movement work per tick
//!   * sparse       — a small active blob in a large mostly-asleep grid
//!
//! Each iteration is timed on a freshly-built state (`iter_batched`, setup
//! untimed) so we measure one representative tick, not a settling transient.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use pwdr_core::material::SAND;
use pwdr_core::Grid;

/// A grid with every even row filled with sand — one step moves every cell.
fn full_active(n: usize) -> Grid {
    let mut g = Grid::new(n, n, 1);
    for y in (0..n).step_by(2) {
        for x in 0..n {
            g.set(x, y, SAND);
        }
    }
    g
}

/// A large grid with a small falling blob; most chunks asleep.
fn sparse(n: usize) -> Grid {
    let mut g = Grid::new(n, n, 1);
    g.paint(n / 2, 8, 12, SAND);
    // Let it start moving so chunks around it are awake but the rest sleeps.
    g.step();
    g
}

fn bench(c: &mut Criterion) {
    let mut grp = c.benchmark_group("tick");

    grp.bench_function("full_active_256", |b| {
        b.iter_batched(|| full_active(256), |mut g| g.step(), BatchSize::SmallInput)
    });
    grp.bench_function("full_active_512", |b| {
        b.iter_batched(|| full_active(512), |mut g| g.step(), BatchSize::SmallInput)
    });
    grp.bench_function("sparse_512", |b| {
        b.iter_batched(|| sparse(512), |mut g| g.step(), BatchSize::SmallInput)
    });
    grp.bench_function("sparse_1024", |b| {
        b.iter_batched(|| sparse(1024), |mut g| g.step(), BatchSize::SmallInput)
    });

    grp.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
