# pwdr

A native Rust falling-sand / powder simulation in the spirit of The Powder Toy.
Powders pile, liquids seek level, gases rise, and reactive/energy elements burn,
conduct, freeze, and transform each other. Fast, deterministic, fully test-covered,
with a pure simulation core (`pwdr-core`) that has **zero graphics dependencies** and a
thin macroquad frontend (`pwdr-app`).

```
cargo run -p pwdr-app --release     # run the simulation
cargo test -p pwdr-core             # headless test suite
cargo bench -p pwdr-core            # criterion baselines
```

See `PROJECT.md` for the architecture contract and `PROGRESS.md` for the build log,
element roster, and design decisions.

## Performance baselines

Single-threaded, one fixed tick. `release`/`bench` profile (LTO thin, codegen-units 1).
60 fps budget is **16.67 ms/tick**; "✓" = comfortably within budget.

Measured on the dev machine (criterion, `--measurement-time 4`). Hardware varies; these
are the regression reference, not absolute claims.

| Benchmark | Regime | Time / tick | Ticks/s | 60 fps? |
|-----------|--------|-------------|---------|---------|
| `full_active_256`  | 256×256, every cell moving | ~0.40 ms | ~2500 | ✓ |
| `full_active_512`  | 512×512, every cell moving | ~1.60 ms | ~625  | ✓ |
| `sparse_512`       | 512×512, small active blob  | ~0.24 ms | ~4150 | ✓ |
| `sparse_1024`      | 1024×1024, small active blob| ~0.86 ms | ~1160 | ✓ |

Baseline established at **M2** (after chunking). Targets from the doctrine —
256² fully active @ 60 fps and 512² sparse @ 60 fps single-threaded — are met with
large headroom; 512² *fully* active also clears 60 fps. The 1024² fully-active stretch
target is reserved for M9 (threading).
