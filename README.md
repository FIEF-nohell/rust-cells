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
| `full_active_256`  | 256×256, every cell moving | ~1.02 ms | ~980  | ✓ |
| `full_active_512`  | 512×512, every cell moving | ~3.94 ms | ~254  | ✓ |
| `sparse_512`       | 512×512, small active blob  | ~0.39 ms | ~2550 | ✓ |
| `sparse_1024`      | 1024×1024, small active blob| ~1.14 ms | ~875  | ✓ |

Baseline established at **M2** (after chunking); refreshed at **M5** after adding the
temperature/diffusion pass (movement + heat run only on awake chunks, so settled or
thermally-uniform regions still cost nothing). Targets from the doctrine — 256² fully
active @ 60 fps and 512² sparse @ 60 fps single-threaded — are met with large headroom;
512² *fully* active also clears 60 fps. The 1024² fully-active stretch target is reserved
for M9 (threading).
