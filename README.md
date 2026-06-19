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

60 fps budget = **16.67 ms/tick**. Numbers below are *indicative* — measured on a heavily
shared 22-core CI-class box, so absolute times swing run-to-run (e.g. `full_active_512` was
seen at 4–10 ms across runs). The relative serial↔parallel comparison and the
pass/fail-against-budget verdicts are stable; re-run `cargo bench` on your machine for hard
numbers.

| Benchmark | Regime | Time / tick | 60 fps? |
|-----------|--------|-------------|---------|
| `full_active_256`  | 256×256, every cell moving | ~1–2 ms   | ✓ |
| `full_active_512`  | 512×512, every cell moving | ~4–10 ms  | ✓ |
| `sparse_512`       | 512×512, small active blob  | ~1 ms     | ✓ |
| `sparse_1024`      | 1024×1024, small active blob| ~3.4 ms   | ✓ |

### Threading (M9)

The heat-diffusion stencil is parallelized over awake chunks with rayon (`step_parallel`).
It is a pure Jacobi pass, so the parallel tick is **byte-identical to the serial tick**
(verified by `parallel_step_matches_serial_bit_for_bit`). Movement and reactions stay
single-threaded so the seeded RNG is consumed in a fixed order — every determinism/golden
guarantee is preserved.

| Benchmark | Serial | Parallel |
|-----------|--------|----------|
| `full_active_512`  | ~5–10 ms  | ~10 ms (rayon overhead ≥ win at this size) |
| `full_active_1024` | ~44 ms    | ~37 ms |

**Verdict vs doctrine targets:** the hard floors — 256² fully active and 512² sparse @
60 fps single-threaded — are met with headroom. The **1024² fully-active @ 60 fps
stretch is not reached** (≈37 ms threaded): at that scale the *serial movement* pass
dominates, and movement is intentionally not parallelized to keep the strict
RNG-ordered determinism. Parallelizing movement (deterministic chunk-coloring + per-chunk
RNG) is the documented next step to chase the stretch.
