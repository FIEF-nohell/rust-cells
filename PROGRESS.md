# PROGRESS — pwdr

Living log of decisions, roster, test status, perf numbers. One section per milestone.

## Status board

| Milestone | State | Tests |
|-----------|-------|-------|
| M0 Workspace & harness | ✅ done | 7 green |
| M1 Grid + first powder | ✅ done | 11 green |
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

---

## M1 — Grid + first powder ✅

**Decisions**
- `Cell` = 4 bytes: `material: u8`, `gen: u8` (moved-this-tick tag), `life: u8`
  (transients, M6+), `tint: u8` (per-cell color jitter seed). Flat `Vec<Cell>`,
  index `y*w+x`.
- Moved flag = generation tag cycling `1..=255`, `0` = untouched sentinel; on wrap
  we clear all tags (amortized O(N)/255 ticks) — no false "moved" collisions, cheap.
- Movement: **bottom-up** row scan so a cell falls exactly one row/tick; **horizontal
  scan direction alternates** each frame (anti-bias). Powder: down, else a randomized
  down-diagonal.
- **Generalized density swap** (`displaces()`) lives from day one: into empty always;
  through a lighter Liquid/Gas iff mover denser. One rule, no per-pair hacks — ready
  for sand-through-water, oil-on-water, gas-rising in later milestones.
- Data-driven `MATERIALS` table; M1 roster = Empty, Stone (static solid), Sand (inert
  heavy powder). Adding an element = adding a table row.
- Render in core → flat RGBA8 with per-cell brightness jitter from `tint`.

**Tests (11 green):** falls one row/tick; rests on floor; rests on a solid row;
pile conserved + near-symmetric (fixed seed); cell ≤ 8 bytes; material table; RNG suite.

**App:** left-drag paints sand, right-drag erases.
