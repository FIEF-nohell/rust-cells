# Bootstrap context: what is `pwdr` and how is it built

**Question:** What is this project, how is it structured, and what conventions govern changes?

## Short answer
`pwdr` (binary `rust-cells`) is a native desktop falling-sand / powder simulation in the spirit of The Powder Toy. It is a two-crate Cargo workspace: `pwdr-core` is the pure simulation (zero graphics deps, fully headless-testable), and `pwdr-app` is the macroquad frontend (window, input, render, palette UI). The whole project ran milestones M0-M9 to completion plus a post-M9 UX pass; it is mature and test-covered. The governing contract is `.docs/PROJECT.md` (locked architecture + doctrine) with a living `.docs/PROGRESS.md` log.

## Evidence
- Workspace: `Cargo.toml` (members `pwdr-core`, `pwdr-app`, resolver 2, release `lto = "thin"`, `codegen-units = 1`).
- `pwdr-core/Cargo.toml`: version 0.4.7, edition 2021, deps `rayon` only; dev-deps `proptest`, `criterion`. Bench `sim_bench` (`harness = false`).
- `pwdr-app/Cargo.toml`: bin `rust-cells` at `src/main.rs`; deps `pwdr-core` (path), `macroquad 0.4`, `rfd 0.15` (file dialogs); windows build-dep `winresource`.
- Core modules: `pwdr-core/src/lib.rs` re-exports `grid`, `material`, `rng`. Sizes: `grid.rs` ~2989 LOC (the engine: Cell, Grid, movement, chunks/dirty-rects, temperature field, reactions, serialize, `step` + `step_parallel`, `render_rgba`, paint/flood-fill helpers), `material.rs` ~1586 LOC (data-driven `MATERIALS` table, `REACTIONS` table, phases, blurbs), `rng.rs` ~121 LOC (seeded xoshiro256** + SplitMix64). App: `pwdr-app/src/main.rs` ~1206 LOC.
- Tests live in `pwdr-core`: `tests/proptest_invariants.rs` plus in-module unit tests; ~88 unit + 2 proptest reported green in PROGRESS.md. Bench `pwdr-core/benches/sim_bench.rs`; example `pwdr-core/examples/showcase.rs`.
- CI: `.github/workflows/ci.yml` runs `cargo fmt --all -- --check`, `cargo clippy -p pwdr-core -- -D warnings`, `cargo test -p pwdr-core`, release builds of both crates (app on windows). `.github/workflows/release.yml` exists.
- Doctrine: `.docs/PROJECT.md` locks 8 decisions (macroquad; two-crate split; flat `Vec<Cell>` indexed `y*w+x`; data-driven materials; in-place update with per-cell moved/gen flag + alternating scan; chunked dirty rects (64x64); seeded reproducible RNG; threading last). Testing doctrine: behavioral + determinism-golden (FNV hash) + proptest invariants + criterion perf, all headless.
- Roster has grown past the documented M7 list of 20: `README.md` references critters (Fish, Worms, Ants, Plant) with their own movement, so element count and behavior categories now exceed PROGRESS.md M7.

## Key invariants (treat as hard constraints)
- **Zero graphics deps in `pwdr-core`.** If sim logic needs macroquad, the design is wrong. Verify with `cargo tree -p pwdr-core`.
- **Determinism.** Same seed + inputs => byte-identical grid after N ticks. No `thread_rng`, no globals, no `Date.now`-style nondeterminism. Golden FNV hash tests guard this; `step_parallel` is byte-identical to `step` (Jacobi heat pass only is parallelized).
- **Flat grid + `y*w+x` indexing**, `Cell` <= 8 bytes (currently 4: material/gen/life/tint). Temperature is a parallel `Vec<f32>`, deliberately out of `Cell`.
- **Adding an element = adding data** (a `MATERIALS` row + optional `REACTIONS` rows + a `blurb`), not rewriting the hot loop. Every paintable element needs a blurb (a test enforces it) and a defining-behavior headless test.
- **TDD, headless.** Tests run without a window. A feature that cannot be tested headlessly means the design is wrong.

## Open questions
- Current exact element count and the full post-M7 roster (critters etc.) are not enumerated in PROGRESS.md; read `material.rs` `MATERIALS`/`REACTIONS` when that detail matters.
- Latest committed version is v0.4.7 (git log); PROGRESS.md "final status" predates the critter additions, so PROGRESS.md trails the code in places.
