<div align="center">

<img src="assets/icon.svg" width="120" alt="rust-cells icon"/>

# rust-cells

A fast, deterministic falling-sand / powder simulation in Rust.

Powders pile, liquids seek their level, gases rise and disperse, and reactive elements
burn, conduct, freeze, dissolve, and explode. It runs on a chunked, multi-threaded grid,
and the simulation core knows nothing about graphics.

[![CI](https://github.com/FIEF-nohell/rust-cells/actions/workflows/ci.yml/badge.svg)](https://github.com/FIEF-nohell/rust-cells/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

</div>

## What it does

- 45+ elements across five phases, with a reaction web: combustion, conduction, corrosion,
  freezing, melting, condensation, quenching, and chain explosions.
- A pure simulation core (`pwdr-core`) with zero graphics dependencies, so it can be tested
  headlessly.
- Deterministic. Same seed and same inputs give a byte-identical result, single- or
  multi-threaded. Golden-hash and property tests keep it that way.
- Built for speed: a flat `Vec<Cell>`, chunked sleeping/dirty regions, one generalized
  density rule, a temperature field with per-edge conductive diffusion, and rayon-parallel
  heat.
- A macroquad sandbox with a searchable/scrollable palette, brush sizing, zoom + minimap,
  pause/step, a heat overlay, and save/load to disk.

## Run it

```sh
cargo run -p pwdr-app --release
```

Prebuilt Windows binaries are attached to each
[release](https://github.com/FIEF-nohell/rust-cells/releases).

Press F8 to load the bundled showcase diorama. It opens paused, so hit Space to bring it
to life.

## Controls

| Input | Action |
|-------|--------|
| Left drag | paint the selected element (into empty space) |
| Right drag | erase |
| Mouse wheel | brush size (smallest is a single grid-snapped cell) |
| `+` / `-` / `0` | zoom in / out / reset |
| Middle drag, minimap click | pan the view |
| Space, Right arrow | pause, single-step |
| F2 | temperature overlay (recolors matter from blue to red) |
| F5, F9, F8 | save, load, load showcase |
| Del | clear |
| type, click a swatch | filter the palette, select an element |

## The element roster

Each palette row shows two markers on the right: a **phase** glyph (block = solid, grains =
powder, droplet = liquid, bubbles = gas, diamond = critter, spark = energy) and, when it
applies, a **hazard** glyph (flame = flammable, burst = explosive).

The palette groups elements by category (which cuts across the physical phase — Lava is a
liquid but lives under Fire & Heat, Gunpowder is a powder but under Explosives):

| Category | Elements |
|----------|----------|
| Earth & Solids | Stone, Sand, Salt, Soil, Snow, Ash, Ice, Glass, Basalt, Obsidian, Diamond, Wood, Wax |
| Liquids | Water, Oil (floats), Saltwater (brine), Acid, Molten Wax |
| Gases | Smoke, Steam, Fume, Hydrogen, Oxygen |
| Fire & Heat | Fire, Lava, Plasma, Frost, Ember, Coal, Cryo, Heater, Cooler |
| Electronics | Conductor, Spark, Battery, Lamp, Fuse |
| Explosives | Gunpowder, Thermite, Nitro, TNT |
| Life | Plant, Fish, Worm, Ant |
| Tools | Clone, Void, Drain |

Some interactions that fall out of the data-driven rules:

- Lava meeting water makes an obsidian or basalt crust plus a burst of steam.
- Conductor carries a charge from a spark. Wire a Battery to a Lamp array (they chain-light),
  or to a Fuse and a TNT block for a remote detonator.
- Salt melts Ice into non-freezing Brine. Acid dissolves most matter and is used up doing so.
- Plasma and Frost are no-trace heat and cold "flames". Heater and Cooler are persistent
  temperature sources.
- Clone is an infinite source of whatever it touches. Void is an infinite sink. Drain is a
  liquid-only sink — it empties a tank without eating the walls.
- Thermite flashes to molten slag. Gunpowder, Nitro, TNT, and Hydrogen chain-detonate.
- Lava (and thermite, which flashes to lava) slowly melts through stone, turning it to more
  lava. Diamond is the only fireproof, acid-proof, blast-proof solid.
- The critters move themselves: Fish swim through water (and flop helplessly when drained out),
  Worms burrow down through powders, and Ants walk on surfaces and graze on Plant.

## Architecture

Two crates, by design:

```
pwdr-core/   the simulation: pure logic on a flat grid, zero graphics deps
pwdr-app/    the macroquad frontend: window, input, render, UI; no sim logic
```

- Flat `Vec<Cell>` indexed `y*w + x`. `Cell` is 4 bytes. Cache locality dominates.
- Data-driven materials. A material id indexes a properties table (phase, density, color,
  conductivity, transitions, life), so adding an element means adding a row rather than
  touching the hot loop. Reactions live in a separate `(A,B)` to `(A',B')` table.
- Chunked dirty/sleep tracking (64x64). Settled regions are skipped, and crossing a boundary
  wakes the neighbour.
- One generalized density rule moves heavier cells through lighter fluids (sand through
  water, oil on water, gas rising), with no per-pair special cases.
- A temperature field with per-edge conductive diffusion (flux is limited by the worse
  conductor, so a wire carries heat instead of bleeding it into the air), plus
  threshold-driven phase transitions.
- A seeded, reproducible RNG (xoshiro256\*\*). No global nondeterminism.
- Threading. The heat-diffusion stencil runs over chunks with rayon as a pure Jacobi pass,
  so a parallel tick is byte-identical to a serial one.

## Testing

```sh
cargo test  -p pwdr-core    # behavioral, determinism-golden, and property (proptest)
cargo bench -p pwdr-core    # criterion baselines
```

If a feature can't be tested headlessly, the design is wrong. The suite covers movement,
piling and settling, every reaction and phase transition, a stored golden hash for a fixed
scenario, proptest invariants (no panics, valid ids, mass conserved under movement), and a
check that the parallel tick equals the serial tick bit for bit.

## Performance

Single tick, `release`/`bench` profile. The 60 fps budget is 16.67 ms per tick. Numbers are
indicative (a shared CI-class box); re-run `cargo bench` for your machine.

| Benchmark | Regime | Time / tick | 60 fps? |
|-----------|--------|-------------|---------|
| `full_active_256` | 256x256, every cell moving | ~1-2 ms | yes |
| `full_active_512` | 512x512, every cell moving | ~4-10 ms | yes |
| `sparse_512` | 512x512, small active blob | ~1 ms | yes |
| `sparse_1024` | 1024x1024, small active blob | ~3.4 ms | yes |

## Project layout

```
pwdr-core/         simulation library (plus benches and the showcase example)
pwdr-app/          macroquad desktop app
assets/            icon
.docs/             PROJECT.md (the build contract) and PROGRESS.md (the build log)
.github/workflows/ CI and release automation
```

Regenerate the showcase map any time:

```sh
cargo run -p pwdr-core --example showcase   # writes rust-cells-showcase.save to your Desktop
```

## License

MIT. See [LICENSE](LICENSE).
