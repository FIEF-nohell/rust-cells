<div align="center">

<img src="assets/icon.svg" width="120" alt="pwdr icon"/>

# pwdr

**A fast, deterministic falling-sand / powder simulation in Rust.**

Powders pile, liquids seek their level, gases rise and disperse, and reactive elements burn,
conduct, freeze, dissolve, and explode — all on a chunked, multi-threaded grid with a clean
simulation core that knows *nothing* about graphics.

[![CI](https://github.com/FIEF-nohell/rust-cells/actions/workflows/ci.yml/badge.svg)](https://github.com/FIEF-nohell/rust-cells/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

</div>

---

## Highlights

- 🧪 **40+ elements** across five phases with a real **reaction web** — combustion, conduction,
  corrosion, freezing, melting, condensation, quenching, and chain explosions.
- ⚙️ **Pure simulation core** (`pwdr-core`) with **zero graphics dependencies** — fully
  testable headlessly.
- 🎯 **Deterministic**: same seed + same inputs ⇒ byte-identical result, single- *or*
  multi-threaded. Backed by golden-hash and property tests.
- ⚡ **Fast**: flat `Vec<Cell>`, chunked sleeping/dirty regions, a generalized density rule,
  a temperature field with per-edge conductive diffusion, and **rayon**-parallel heat.
- 🖌️ A **macroquad** sandbox: searchable/scrollable palette, brush sizing, zoom + minimap,
  pause/step, a heat overlay, and native save/load to disk.

## Run it

```sh
cargo run -p pwdr-app --release
```

> Prebuilt Windows binaries are attached to each [release](https://github.com/FIEF-nohell/rust-cells/releases).

Then press **F8** to load the bundled showcase diorama (it opens paused — hit **Space** to
bring it to life).

## Controls

| Input | Action |
|-------|--------|
| **Left drag** | paint the selected element (into empty space) |
| **Right drag** | erase |
| **Mouse wheel** | brush size (smallest = a single grid-snapped cell) |
| **`+` / `-` / `0`** | zoom in / out / reset |
| **Middle drag** · **minimap click** | pan the view |
| **Space** · **→** | pause · single-step |
| **F2** | temperature overlay (recolors matter blue→red) |
| **F5** · **F9** · **F8** | save · load · load showcase |
| **Del** | clear |
| *type* · **click swatch** | filter the palette · select an element |

## The element roster

| Phase | Elements |
|-------|----------|
| **Powders** | Sand, Gunpowder, Salt, Thermite |
| **Liquids** | Water, Oil (floats), Saltwater (brine), Lava, Acid, Nitro, Mercury, Molten Wax |
| **Gases** | Smoke, Steam, Fire, Fume, Hydrogen, Plasma, Frost |
| **Solids** | Stone, Ice, Basalt, Obsidian, Glass, Wood, Plant, Conductor, Cryo, Wax, Coal, TNT, Battery, Lamp, Fuse, Heater, Cooler, Clone, Void |

A few of the interactions that *emerge* from the data-driven rules:

- **Lava + Water** → obsidian/basalt crust + a burst of steam.
- **Conductor + Spark** carries a travelling charge; wire it from a **Battery** to a **Lamp**
  array (they chain-light) or to a **Fuse** → **TNT** for a remote detonator.
- **Salt** melts **Ice** into non-freezing **Brine**; **Acid** dissolves most matter and is consumed.
- **Plasma**/**Frost** are no-trace heat/cold "flames"; **Heater**/**Cooler** are persistent sources.
- **Clone** is an infinite source of whatever it touches; **Void** is an infinite sink.
- **Thermite** flashes to molten slag; **Gunpowder/Nitro/TNT/Hydrogen** chain-detonate.

## Architecture

Two-crate workspace (locked by design):

```
pwdr-core/   the simulation — pure logic on a flat grid, ZERO graphics deps
pwdr-app/    the macroquad frontend — window, input, render, UI; holds no sim logic
```

- **Flat `Vec<Cell>`** indexed `y*w + x`; `Cell` is 4 bytes. Cache locality dominates.
- **Data-driven materials**: a `Material` id indexes a properties table (phase, density,
  color, conductivity, transitions, life). Adding an element is adding a row, not touching
  the hot loop. Reactions live in a separate `(A,B) → (A',B')` table.
- **Chunked dirty/sleep tracking** (64²): settled regions are skipped; crossing a boundary
  wakes the neighbour.
- **Generalized density rule** moves heavier movable cells through lighter fluids (sand
  through water, oil on water, gas rising) — one rule, no per-pair hacks.
- **Temperature field** with per-edge conductive diffusion (flux limited by the worse
  conductor, so a wire carries heat instead of bleeding it into air) and threshold-driven
  phase transitions.
- **Seeded, reproducible RNG** (xoshiro256\*\*) — no global nondeterminism.
- **Threading** (M9): the heat-diffusion stencil is parallelized over chunks with rayon as a
  pure Jacobi pass, so a parallel tick is **byte-identical** to a serial one.

## Testing

```sh
cargo test  -p pwdr-core    # behavioral + determinism-golden + property (proptest)
cargo bench -p pwdr-core    # criterion baselines
```

The doctrine: if a feature can't be tested headlessly, the design is wrong. The suite covers
movement/piling/settling, every reaction and phase transition, a stored **golden hash** for a
fixed scenario, **proptest** invariants (no panics, valid ids, mass conserved under movement),
and a check that the **parallel tick equals the serial tick** bit-for-bit.

## Performance

Single tick, `release`/`bench` profile. 60 fps budget = **16.67 ms/tick**. Numbers are
indicative (a shared CI-class box; re-run `cargo bench` for your machine):

| Benchmark | Regime | Time / tick | 60 fps? |
|-----------|--------|-------------|---------|
| `full_active_256` | 256×256, every cell moving | ~1–2 ms | ✓ |
| `full_active_512` | 512×512, every cell moving | ~4–10 ms | ✓ |
| `sparse_512` | 512×512, small active blob | ~1 ms | ✓ |
| `sparse_1024` | 1024×1024, small active blob | ~3.4 ms | ✓ |

## Project layout

```
pwdr-core/         simulation library (+ benches, the showcase example)
pwdr-app/          macroquad desktop app
assets/            icon
.docs/             PROJECT.md (the build contract) and PROGRESS.md (the build log)
.github/workflows/ CI + release automation
```

Regenerate the showcase map any time:

```sh
cargo run -p pwdr-core --example showcase   # writes pwdr-showcase.save to your Desktop
```

## License

MIT — see [LICENSE](LICENSE).
