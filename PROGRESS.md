# PROGRESS — pwdr

Living log of decisions, roster, test status, perf numbers. One section per milestone.

## Status board

| Milestone | State | Tests |
|-----------|-------|-------|
| M0 Workspace & harness | ✅ done | 7 green |
| M1 Grid + first powder | ⏳ | |
| M2 Chunks & dirty rects | ⏳ | |
| M3 Liquids | ⏳ | |
| M4 Gases | ⏳ | |
| M5 Temperature & transitions | ⏳ | |
| M6 Reactions & energy | ⏳ | |
| M7 Full roster | ⏳ | |
| M8 App polish | ⏳ | |
| M9 Threading | ⏳ | |

---

## M0 — Workspace & harness ✅

**Decisions**
- Two-crate workspace: `pwdr-core` (zero graphics deps) + `pwdr-app` (macroquad 0.4).
- RNG: SplitMix64-seeded **xoshiro256\*\*** in `pwdr-core::rng`. Fast, long period,
  `Clone` for exact snapshot/restore. No `thread_rng`, no globals — determinism contract.
- Framebuffer produced *by the core* as flat RGBA8 `Vec<u8>` (`Grid::render_rgba`). This is
  pure data, not graphics — keeps the graphics-free rule while giving the app a zero-logic
  blit path (single `Texture2D::update`, no per-particle draw calls).
- Fixed-timestep sim at 60 Hz in the app, decoupled from render via an accumulator.

**Tests (7 green):** RNG determinism / divergence / range / chance bounds; grid dims;
framebuffer size+opacity; deterministic step.

**Perf:** baselines deferred to M2 per doc (criterion harness compiles + runs now).
