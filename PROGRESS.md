# PROGRESS — pwdr

Living log of decisions, roster, test status, perf numbers. One section per milestone.

## Status board

| Milestone | State | Tests |
|-----------|-------|-------|
| M0 Workspace & harness | ✅ done | 7 green |
| M1 Grid + first powder | ✅ done | 11 green |
| M2 Chunks & dirty rects | ✅ done | 16 green |
| M3 Liquids | ✅ done | 19 unit + proptest |
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

---

## M2 — Chunks & dirty rects ✅

**Decisions**
- Grid tiled by `CHUNK=64` chunks. Two bit-vectors: `active` (process this tick) and
  `wake` (accumulated for next tick). `begin_tick` copies `wake`→`active`, clears `wake`.
- **Chunk-granular dirty tracking.** Every write/move calls `touch(x,y)`, which wakes the
  cell's chunk plus any chunk whose border (incl. diagonals) the cell sits against. That
  is exactly "crossing a boundary wakes the neighbor" — a settled-but-edge cell is re-woken
  when the cell across the border changes.
- Movement still scans **global rows bottom-up** (correct fall order preserved); it only
  *skips cells in sleeping chunks*. Skipped cells are provably static, so the result is
  **byte-identical to a full scan** — verified by `chunking_matches_full_scan`.
  Chose chunk-granular over sub-chunk rectangles: it keeps exact scan order (hence
  determinism) and already meets every perf target.
- Added `hash()` (FNV-1a over material/life/tint) for golden determinism tests.

**Tests (16 green):** distant chunks sleep; fully-settled grid sleeps completely + no-op
step; crossing boundary wakes neighbor; chunked == full scan; golden hash stable; all M1
behavior intact.

**Perf baselines (criterion, see README):** full_active 256² ≈ 0.40 ms, 512² ≈ 1.60 ms;
sparse 512² ≈ 0.24 ms, 1024² ≈ 0.86 ms. All within the 16.67 ms/60 fps budget. Doctrine
floors (256² full, 512² sparse @ 60 fps single-thread) met with large headroom.

---

## M3 — Liquids ✅

**Decisions**
- Liquid movement: down → down-diagonal → **horizontal level-seeking**. The
  level-seek (`scan_descent`) scans each side, up to `dispersion`, *through passable
  cells only* (empty/lighter — water cannot pass water) for the nearest column where
  it can fall; it steps one cell toward the nearer descent.
- **Guaranteed settling, no infinite oscillation:** a liquid only flows toward a place
  it can descend. On flat ground with no lower neighbour it rests. Verified by asserting
  `awake_chunk_count()==0` after settling in the basin and spread tests.
- Density displacement is the same generalized `displaces()` rule from M1 — sand sinks
  through water, water bubbles up, no per-pair code. Water density 1000 < sand 1600.
- Roster +Water (Liquid, dispersion 5).

**Tests (19 unit + proptest):** basin fills level + conserved + settles; thin column
spreads along floor; denser powder sinks through liquid (both conserved); plus the new
**proptest** suite (200 cases): no panic / no OOB, all ids valid, movement conserves mass.

**App:** keys 1/2/3 select Sand/Water/Stone, `[`/`]` brush size, HUD shows selection.
