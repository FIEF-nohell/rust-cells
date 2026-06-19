# PROGRESS — pwdr

Living log of decisions, roster, test status, perf numbers. One section per milestone.

## Status board

| Milestone | State | Tests |
|-----------|-------|-------|
| M0 Workspace & harness | ✅ done | 7 green |
| M1 Grid + first powder | ✅ done | 11 green |
| M2 Chunks & dirty rects | ✅ done | 16 green |
| M3 Liquids | ✅ done | 19 unit + proptest |
| M4 Gases | ✅ done | 22 unit + proptest |
| M5 Temperature & transitions | ✅ done | 30 unit + proptest |
| M6 Reactions & energy | ✅ done | 35 unit + proptest |
| M7 Full roster | ✅ done | 40 unit + 2 proptest |
| M8 App polish | ✅ done | 42 unit + 2 proptest |
| M9 Threading | ✅ done | 43 unit + 2 proptest |

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

---

## M4 — Gases ✅

**Decisions**
- Replaced the displacement rule with **direction-aware** `can_move_into(mover, target, dy)`:
  sinking → denser wins, rising → lighter wins, lateral → denser pushes lighter, empty
  always passable. This *one* rule now yields sand-through-water, oil-on-water, and
  gas-rising with zero per-pair code (locked decision #4 / the generalized density rule).
- Gas movement = inverse of liquid: up → up-diagonal → sideways dispersion.
- **Transient life:** `MaterialProps.life` / `decay_to`; cells seed `life` on placement,
  a `life_pass` decrements and converts on expiry. A `transients` counter skips the whole
  pass when none exist → **zero cost** for non-transient scenes (full-active perf intact).
- Roster +Oil (Liquid, density 800 < water → floats; flammable later) and +Smoke (Gas,
  density −50, life 180 → fades to Empty).

**Tests (22 unit + proptest):** gas rises one row/tick; finite-life gas fades to empty
and the grid re-sleeps; lighter liquid floats at/above the water surface (conserved).
proptest broadened to Empty..Oil (all conservative).

**App:** keys 1–5 = Sand/Water/Stone/Oil/Smoke.

---

## M5 — Temperature & transitions ✅

**Decisions**
- **Temperature field** = parallel `Vec<f32>` (one per cell). Kept *out* of `Cell`:
  heat needs float range (−∞..1200+) and precision a packed byte would lose, and it is
  only touched in the temperature pass — never in the movement hot path. Temperature
  travels with a cell (swapped on move).
- Diffusion: explicit 4-neighbour, **insulated boundary** (OOB neighbour = self, zero
  flux), rate = per-material `conductivity` (≤0.25 for stability). Runs only on awake
  chunks; a cell whose temp changes >EPS re-wakes via `touch`, so thermal activity rides
  the existing chunk wake-set and a thermally-uniform region sleeps. (A static-but-hot
  block still processes because `set_temperature`/diffusion call `touch`.)
- Transitions are data-driven thresholds on each material: `high_temp→high_to`,
  `low_temp→low_to`. Energy (temperature) is preserved across the change.
- Roster +Ice (Water⇄, melt/freeze @0), +Steam (Water⇄, boil@100/condense@99),
  +Lava (→Basalt@500) +Basalt (→Lava@1000). **Lava+water emergently** makes basalt +
  steam via diffusion alone — no special-case reaction.

**Tests (30 unit + proptest):** freeze, melt, boil, condense, solidify (one each);
neighbour diffusion converges to the mean; emergent lava+water → basalt+steam.

**Perf (M5 refresh, README):** full_active 256² ≈ 1.0 ms / 512² ≈ 3.9 ms; sparse 512² ≈
0.39 ms / 1024² ≈ 1.1 ms. Regression vs M2 is the new heat pass on awake chunks —
expected, justified, all within the 16.67 ms/60 fps budget.

**App:** keys 1–9 select Sand/Water/Stone/Oil/Smoke/Ice/Steam/Lava/Basalt.

---

## M6 — Reactions & energy ✅

**Decisions**
- **Data-driven reaction table** `REACTIONS: [(a,b)->(a',b'), prob, min_temp]`; the hot
  loop just walks it (`reaction_for`). Reactions run in their own pass **before movement**,
  4-neighbour, one reaction per cell per tick. Reacted cells are gen-tagged so they neither
  move nor react again that tick; both endpoints wake their chunks. `transform()` keeps the
  transient count, life, gen and temperature consistent (a hot product raises cell temp so
  cascades work).
- **Fire** (Gas, life 60 → Smoke byproduct, 700°C): spreads into oil `(Fire,Oil)->(Fire,
  Fire)`, quenched by water `(Fire,Water)->(Smoke,Steam)`. Oil also **autoignites** at
  350°C (temperature path) — heat and contact both work.
- **Spark/conduction:** Copper conductor; a **Spark** (Energy, life 2) energizes adjacent
  plain copper `(Spark,Copper)->(Spark,Spark)` and leaves a refractory **Charged** trail
  (life 4 → Copper). The refractory trail makes a clean traveling wave that doesn't bounce,
  and the wire fully restores + sleeps afterward. Spark ignites oil `(Spark,Oil)->(Charged,
  Fire)`.
- **Acid** (corrosive Liquid): dissolves sand/stone/copper/basalt and is **consumed**
  `(Acid,X)->(Empty,Empty)` at per-material probabilities.

**Tests (35 unit + proptest):** fire consumes oil + emits smoke; fire quenched by water →
steam; acid dissolves solid + is consumed; spark conducts end-to-end then settles (wire
restored, grid sleeps); spark ignites oil.

**App:** +Q Copper, E Spark, F Fire, C Acid.

---

## M7 — Full roster ✅

### The roster (20 materials incl. Empty)

| # | Name | Phase | Defining behavior |
|---|------|-------|-------------------|
| 0 | Empty | — | vacuum / air (carries ambient temperature) |
| 1 | Stone | Solid | inert wall, blast-proof |
| 2 | Sand | Powder | inert heavy powder; melts to **Glass** at 1100°C |
| 3 | Water | Liquid | seeks level; freezes→Ice@0, boils→Steam@100 |
| 4 | Oil | Liquid | **floats on water** (ρ800<1000); flammable, autoignites@350 |
| 5 | Smoke | Gas | rises; **finite life**→fades to Empty |
| 6 | Ice | Solid | melts→Water@0 |
| 7 | Steam | Gas | rises; **condenses**→Water@99 |
| 8 | Lava | Liquid | hot (1200°); **cools→Basalt@500** (e.g. on water) |
| 9 | Basalt | Solid | remelts→Lava@1000 |
| 10 | Copper | Solid | **conductor** (electrical + thermal) |
| 11 | Spark | Energy | travels along copper, leaves refractory Charged trail |
| 12 | Charged | Solid | spark's refractory trail → back to Copper |
| 13 | Fire | Gas | **spreads**, finite life, byproduct **Smoke** |
| 14 | Acid | Liquid | **corrosive**: dissolves solids, is consumed |
| 15 | Fume | Gas | **flammable gas**, propagates fire upward |
| 16 | Gunpowder | Powder | **explosive**: blast radius 4, chain-detonates |
| 17 | Cryo | Solid | **cold source**: freezes adjacent water→Ice |
| 18 | Wood | Solid | **flammable solid**: fire creeps along it |
| 19 | Glass | Solid | inert; melt product of sand, remelts@1450 |

### Reaction web (data-driven, `material::REACTIONS` + temperature transitions + blast hook)
Combustion (Fire+Oil/Fume/Wood), quench (Fire+Water→Smoke+Steam), conduction
(Spark+Copper), ignition (Spark+Oil/Fume→Fire), corrosion (Acid+Sand/Stone/Copper/
Basalt/Wood→consumed), freezing (Cryo+Water→Ice), explosion (Fire/Spark + Gunpowder →
radial blast, chains). Phase transitions: freeze/melt/boil/condense/solidify/remelt/
sand-melt — all threshold-driven on the temperature field.

**Archetype checklist (all covered):** inert + reactive powder (Sand, Gunpowder); two
liquids with float ordering (Oil/Water); corrosive consumed (Acid); hot-cools-to-solid
(Lava→Basalt on water); flammable liquid + gas (Oil, Fume); explosive (Gunpowder);
rising gas that fades (Smoke) + one that condenses (Steam); fire w/ byproduct (Fire→Smoke);
conductor + traveling spark (Copper/Spark); cold source (Cryo).

**Tests (40 unit + 2 proptest):** one defining-behavior test per new element
(fume propagation, gunpowder radial blast, cryo freezing, wood burning, sand→glass) plus
all prior. New **full-roster fuzz proptest**: random ops over *every* material + random
ticks never panic / never corrupt ids.

**App:** keys 1–9 + Q/E/F/C/G/T/Z/V/B cover the roster (proper palette UI in M8).

---

## M8 — App polish ✅

**Decisions**
- **Save/load** is pure-core: `Grid::serialize()`/`deserialize()` produce/consume a byte
  blob (magic+version, dims, RNG state, per-cell material/gen/life/tint, temperature field,
  chunk wake bits). Saving the RNG state *and* the wake set makes a reloaded grid evolve
  **bit-for-bit identically** (RNG consumption depends on which chunks are awake). File IO
  stays in the app (`F5`/`F9` ↔ `pwdr.save`) so the core keeps zero platform assumptions.
- App UI: **categorized + searchable palette** (grouped by phase; type to filter by name —
  text input feeds the search box, controls use non-text keys so they never collide),
  click-to-select with swatches; brush size `[`/`]`; **pause** (Space) + **single-step**
  (→); coordinate + material + temperature readout under the cursor; FPS + per-tick time;
  clear (Del). Fixed-timestep sim capped at 8 substeps/frame to avoid spiral-of-death.

**Tests (42 unit + 2 proptest):** added serialize round-trip (state + identical future
evolution) and deserialize-rejects-garbage; all prior green.

---

## M9 — Threading ✅

**Decisions**
- `pwdr-core` gains **rayon** (a compute dep — still zero graphics deps; verified via
  `cargo tree`). New `Grid::step_parallel()` alongside the serial `Grid::step()`.
- The **heat-diffusion stencil** is parallelized **over awake chunks** with rayon. It is a
  pure **Jacobi** pass (reads the old field, writes the `temp_next` scratch); chunks tile
  the grid so each task writes a disjoint index range — one contained, documented `unsafe`
  (a `Sync` pointer wrapper) justified by that tiling. Boundary handling is explicit:
  insulated edges (OOB neighbour = self), neighbour reads from the old field.
- **Determinism is the strongest possible:** the parallel tick is **byte-identical to the
  serial tick** (Jacobi is order-independent), proven by
  `parallel_step_matches_serial_bit_for_bit` (cells + temperature field, 300 ticks on a
  lava/water/fire scene). Movement and reactions stay single-threaded so the seeded RNG is
  consumed in a fixed order — **all earlier golden/behavioral/proptest tests stay green
  and unchanged.**

**Tests (43 unit + 2 proptest):** all prior + parallel↔serial bit-for-bit equivalence.

**Perf (see README, indicative on a shared 22-core box):** hard floors met — 256² fully
active and 512² sparse @ 60 fps single-threaded, with headroom. Parallel diffusion gives a
real speedup on the diffusion-bound 1024² case (~44 ms → ~37 ms). The **1024² fully-active
@ 60 fps stretch is not reached**: serial movement dominates at that scale and is
intentionally left serial to preserve strict determinism. Documented next step:
deterministic parallel movement via chunk-coloring + per-chunk RNG.

---

## Final status

All milestones M0–M9 green. `cargo test -p pwdr-core`: **43 unit + 2 proptest (incl.
behavioral, determinism-golden, and property invariants)** pass. `pwdr-core` compiles with
zero graphics dependencies (rayon only). `cargo bench` baselines recorded in the README.
The macroquad app builds and runs the full self-designed 20-element roster with a
categorized + searchable palette, pause/step, readouts, and deterministic save/load.

---

## Post-M9 fixes (user feedback)

- **Conduction reworked.** Spark is now a pure igniter that **vanishes** (decay→Empty) — a
  free-painted spark no longer materializes copper. The charge travels as energized copper:
  `Spark+Copper→Charged`, `Charged+Copper→Charged`, with a refractory `Charged→Cooled→Copper`
  trail (new **Cooled** material, id 20) that stops the wave bouncing. Charged/Cooled are
  internal (`material::user_paintable` hides them from the palette).
- **Paint writes into empty cells only** (left-click never overwrites matter); right-click
  erase still clears anything.
- **Brush size = Shift + mouse wheel** (replaces `[`/`]`). Material hotkeys removed —
  selection is via the click/search palette only (no collision with search typing).
- **Window is resizable**; a resize shows a "this wipes the canvas" prompt — Enter rebuilds
  the grid to fit (correct mouse mapping at the new size), Esc keeps the current canvas.
- Tests: +paint-empty-only, +free-spark-leaves-no-copper; spark conduction test updated.
  45 unit + 2 proptest green; golden regenerated (paint RNG consumption changed).
