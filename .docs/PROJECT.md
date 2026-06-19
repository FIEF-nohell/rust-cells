# PROJECT.md — `pwdr`: a native Rust falling-sand / powder simulation

A native desktop powder simulation in the spirit of The Powder Toy. A grid of cells
where powders fall and pile, liquids spread and seek level, gases rise and disperse, and
reactive/energy elements burn, conduct, freeze, and transform each other. Fast (large
grids at 60fps), correct (deterministic, fully test-covered), clean (a pure simulation
core that knows nothing about graphics).

Personal project, single user. No code-signing, no installer, no web target.

The verifiable end state lives in the `/goal` condition. This doc is the contract for
*how* to get there: the locked architecture, the build order, and the testing bar.

---

## OPERATING DOCTRINE

You are the entire team: architect, implementer, test engineer, performance engineer,
reviewer. Build milestone by milestone, in order. For each milestone:

1. **Architect** — short plan: what changes, what the acceptance test looks like. Decide;
   do not ask.
2. **Test engineer** — write the milestone's tests first (or alongside). They start red.
   Tests are automated and run without a GUI.
3. **Implementer** — build until green.
4. **Performance engineer** — once a baseline exists, check it; investigate regressions
   before advancing.
5. **Reviewer** — confirm every acceptance criterion is met and tests pass, then commit
   and advance.

Hard rules:
- Never advance with failing or skipped tests.
- Never stub and move on. Finish, or defer explicitly in writing.
- Do not stop between milestones for permission. Advance whenever the current one is
  green. Only stop on a genuine ambiguity this doc can't resolve.
- Profile before optimizing; state the measured bottleneck before each optimization.
- Keep `PROGRESS.md` current: decisions, roster, test status, perf numbers. Commit per
  milestone.

---

## LOCKED DECISIONS (do not change)

1. **Engine: macroquad.** Window + input + GPU-blitted pixel framebuffer, minimal
   boilerplate. Not Tauri, not raw winit/wgpu.

2. **Two-crate workspace.** `pwdr-core` is the simulation with **zero graphics
   dependencies** — pure logic on a grid, fully testable headlessly. `pwdr-app` is the
   macroquad frontend (window, input, render, UI), depends on `pwdr-core`, holds as
   little logic as possible. If sim logic depends on macroquad, you have failed.

3. **Flat `Vec<Cell>`** indexed `y * width + x`. No nested vecs. Cache locality dominates.

4. **Data-driven materials.** A `Material` id maps to a properties table (phase, density,
   flammability, reactions, color, heat). Adding an element means adding data, not
   rewriting the hot loop.

5. **In-place update with a per-cell moved flag, alternating horizontal scan direction
   each frame** (bottom-up for gravity). Not naive top-left-to-bottom-right.

6. **Chunked dirty rectangles.** Grid split into chunks (start 64x64). Sleeping chunks
   are skipped. Crossing a boundary wakes the neighbor.

7. **Seeded, reproducible RNG.** Small fast PRNG, no `thread_rng`, no global nondeterminism
   in the core. Same seed + inputs => byte-identical grid after N ticks.

8. **Threading is the last milestone**, via rayon over chunks with explicit boundary
   handling, only after correctness tests pass.

---

## ELEMENTS — YOU DESIGN THE ROSTER

Design the element set yourself; do not implement a list handed to you. Constraints:

- ~20-30 elements in categories that drive the palette: Powders, Liquids, Gas, Energy
  (plus Solid/static if useful), plus Empty and Erase.
- Every phase represented: powder, liquid, gas, energy, solid.
- A real **reaction web**, not isolated elements: combustion, dissolving, freezing/melting,
  condensation, cooling-to-solid, explosion. Favor emergent interactions.
- Cover these archetypes (your naming/flavor): an inert heavy powder and a reactive
  powder; two liquids with a density relationship (one floats); a corrosive liquid that
  dissolves materials and is consumed; a hot liquid that cools to a solid on contact with
  a cold liquid; flammable liquid and gas that propagate fire; at least one explosive;
  rising gases, one with finite life that fades and one that condenses; a fire element
  that spreads, has finite life, and emits a byproduct; a conductor that carries a
  traveling spark to ignite/react; a cold source that freezes liquids.
- A **generalized density rule** so heavier movable cells swap through lighter ones (covers
  sand-through-water, oil-on-water, light-gas-rising) without per-pair hacks.
- A temperature field drives phase transitions and diffuses between neighbors.

Document the chosen roster and reaction rules in `PROGRESS.md`.

---

## CELL & MATERIAL MODEL

- `Cell` small (aim <= 8 bytes): material id + packed state.
- Grid also carries: a per-tick moved/generation flag, a temperature field (parallel
  `Vec` or packed field, justify in a comment), a `life`/decay counter for transients.
- `MaterialProps` table by id: phase, density, flammability + ignition, dispersion
  (sideways spread per tick for fluids/gases), base color (+ optional jitter), reaction
  rules, heat output/capacity/default temperature.

Movement by phase:
- **Powder**: down, else down-diagonals (randomized order); rests when blocked; sinks
  through lighter fluids via density swap.
- **Liquid**: down, else down-diagonals, else spread horizontally up to `dispersion`;
  density swap with lighter fluids/gases.
- **Gas**: inverse of liquid; disperses; may have finite life.
- **Solid**: static unless transformed/destroyed by a reaction.
- **Energy**: custom propagation per element (fire spread, spark along conductors, etc.).

Reactions are data-driven contact rules `(A, B) -> (A', B')` with a probability,
optionally temperature-gated, in a reactions module driven by the table.

---

## SIMULATION ALGORITHM (per tick)

1. Reset moved flags cheaply (generation counter, not a full clear).
2. For each awake chunk, in the correct scan order, iterate its dirty region: skip
   empty/already-moved cells; run movement; run contact reactions; mark moved cells and
   dirty/wake source + destination + boundary-neighbor chunks.
3. Temperature diffusion + phase transitions (one pass or two — measure which is faster).
4. Decrement `life`; convert/remove transients on expiry.
5. Recompute the awake set; chunks with no activity sleep.

Fixed-timestep sim decoupled from render framerate. Support pause and single-step.

---

## TESTING DOCTRINE (non-GUI, automated)

All tests run headless via `cargo test` against `pwdr-core`. No window, no rendering in
any test. Write tests with each feature.

- **Behavioral**: a powder cell falls one row per tick in empty space; piled powder forms
  a symmetric pyramid (fixed seed); a basin fills level and stops; denser sinks through
  lighter; gas rises; transients vanish after their life; each designed reaction yields
  its expected products.
- **Determinism / golden**: seed a fixed scenario, run N ticks, hash the full grid, assert
  it equals a stored golden hash. Provide a documented way to regenerate goldens.
- **Property (proptest)**: across random seedings/tick counts — no out-of-range material
  id; particles never silently duplicate or vanish except where a reaction/life expiry is
  meant to consume them; no out-of-bounds writes; never panics.
- **Performance (criterion)**: ticks/sec on a fully-active grid and a sparse grid; record
  baselines in the README; regressions are bugs.

If a feature can't be tested headlessly, the design is wrong. Fix the design.

---

## PERFORMANCE DOCTRINE

- Establish the criterion baseline after chunking (M2); never regress without justification.
- Targets to measure against (floors, report real numbers): 256x256 fully active at 60fps
  single-threaded; 512x512 sparse at 60fps single-threaded; after threading, 1024x1024
  fully active at 60fps (stretch).
- Render: one RGBA `[u8]` framebuffer per frame, single texture blit, no per-particle draw
  calls. Sub-update dirty texture regions only if measured faster.

---

## MILESTONES

- **M0 — Workspace & harness.** Two-crate workspace; `pwdr-core` compiles graphics-free;
  `pwdr-app` opens a macroquad window showing an empty grid blitted as a texture; seeded
  RNG; `cargo test` runs.
- **M1 — Grid + first powder.** Grid, Cell, Material table, powder movement, brush, erase.
  Tests: falling, piling, rest.
- **M2 — Chunks & dirty rects.** Activity tracking + scan-order alternation; criterion
  baseline. Tests: sleeping chunks skip, wake-on-boundary, behavior unchanged.
- **M3 — Liquids.** Spread/level-seeking + density displacement. Tests: basin fills level,
  denser sinks, no infinite oscillation.
- **M4 — Gases.** Generalized cross-phase density displacement; rising/dispersing gas with
  optional life; a lighter liquid that floats. Tests: gas rises, float ordering holds.
- **M5 — Temperature & transitions.** Heat field + diffusion; freeze/boil/melt/condense/
  solidify. A test per transition.
- **M6 — Reactions & energy.** Data-driven reactions; fire, spark/conduction, a corrosive,
  and your other energy elements. A test per reaction.
- **M7 — Full roster.** Implement the complete self-designed roster with behaviors and
  reactions. Tests cover each element's defining behavior.
- **M8 — App polish.** Categorized + searchable palette, brush size, pause/step, coordinate
  + grid info readout, FPS + tick-time display, save/load grid to file.
- **M9 — Threading.** rayon over chunks with correct boundary scheduling. All earlier tests
  stay green and deterministic. Report new perf numbers.
