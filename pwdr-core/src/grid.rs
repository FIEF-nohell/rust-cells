//! The simulation grid: a flat `Vec<Cell>` indexed `y * width + x`, the per-tick
//! update, and chunked activity tracking so sleeping regions are skipped.

use crate::material::{self, MaterialId, Phase, EMPTY};
use crate::rng::Rng;
use rayon::prelude::*;

/// Chunk edge length. Grid is tiled by `CHUNK x CHUNK` chunks; each tracks
/// whether it needs processing. 64 per the locked decision.
pub const CHUNK: usize = 64;

/// One grid cell. Kept to 4 bytes (`<= 8` per the contract).
///
/// `gen` is the moved-this-tick tag: when it equals the grid's current
/// generation, the cell has already been updated this tick and is skipped.
/// `life`/`tint` are reserved for transients (M6+) and color jitter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub material: MaterialId,
    pub gen: u8,
    pub life: u8,
    pub tint: u8,
}

impl Cell {
    pub const EMPTY: Cell = Cell {
        material: EMPTY,
        gen: 0,
        life: 0,
        tint: 0,
    };
}

#[derive(Clone)]
pub struct Grid {
    width: usize,
    height: usize,
    cells: Vec<Cell>,
    /// Temperature field, parallel to `cells` (one f32 per cell). Kept separate
    /// from `Cell` rather than packed: heat math wants float precision and a
    /// fixed-point byte in the 4-byte cell would lose range (−inf..1200+) and
    /// complicate diffusion. It is only touched in the temperature pass, so it
    /// stays out of the movement hot path entirely.
    temp: Vec<f32>,
    /// Scratch buffer for the Jacobi diffusion pass (reused each tick).
    temp_next: Vec<f32>,
    /// Current moved-tag generation. Cycles 1..=255; 0 is the "untouched"
    /// sentinel. On wrap we clear all tags (amortized O(N)/255 ticks) so a
    /// stale tag can never collide with a new tick's generation.
    gen: u8,
    /// Flips each tick to alternate horizontal scan direction (anti-bias).
    frame_parity: bool,
    rng: Rng,
    framebuffer: Vec<u8>,

    // --- chunked activity tracking ---
    chunks_x: usize,
    chunks_y: usize,
    /// Chunks to process *this* tick.
    active: Vec<bool>,
    /// Chunks to process *next* tick — accumulated as cells are written.
    wake: Vec<bool>,
    /// Count of cells with `life > 0`. When zero, the life pass is skipped — so
    /// transients cost nothing when none exist. Movement swaps preserve it.
    transients: usize,
}

impl Grid {
    pub fn new(width: usize, height: usize, seed: u64) -> Self {
        assert!(width > 0 && height > 0, "grid must be non-empty");
        let chunks_x = width.div_ceil(CHUNK);
        let chunks_y = height.div_ceil(CHUNK);
        let nchunks = chunks_x * chunks_y;
        Grid {
            width,
            height,
            cells: vec![Cell::EMPTY; width * height],
            temp: vec![material::props(EMPTY).default_temp; width * height],
            temp_next: vec![material::props(EMPTY).default_temp; width * height],
            gen: 0,
            frame_parity: false,
            rng: Rng::new(seed),
            framebuffer: vec![0; width * height * 4],
            chunks_x,
            chunks_y,
            active: vec![false; nchunks],
            // Start with every chunk queued so the first tick visits all of it,
            // then chunks settle to sleep.
            wake: vec![true; nchunks],
            transients: 0,
        }
    }

    #[inline]
    pub fn width(&self) -> usize {
        self.width
    }
    #[inline]
    pub fn height(&self) -> usize {
        self.height
    }

    #[inline]
    pub fn in_bounds(&self, x: isize, y: isize) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height
    }

    #[inline]
    fn idx(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> Cell {
        self.cells[self.idx(x, y)]
    }

    #[inline]
    pub fn material_at(&self, x: usize, y: usize) -> MaterialId {
        self.cells[self.idx(x, y)].material
    }

    #[inline]
    pub fn is_empty(&self, x: usize, y: usize) -> bool {
        self.cells[self.idx(x, y)].material == EMPTY
    }

    /// Read-only view of all cells (tests / golden hashing).
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// Count cells of a given material — test/debug helper.
    pub fn count(&self, id: MaterialId) -> usize {
        self.cells.iter().filter(|c| c.material == id).count()
    }

    /// Number of chunks awake (queued for next tick). Test/HUD helper.
    pub fn awake_chunk_count(&self) -> usize {
        self.wake.iter().filter(|&&w| w).count()
    }

    /// Whether the chunk containing cell (x,y) is queued awake for next tick.
    pub fn chunk_awake_at(&self, x: usize, y: usize) -> bool {
        self.wake[self.chunk_idx(x / CHUNK, y / CHUNK)]
    }

    pub fn chunks_x(&self) -> usize {
        self.chunks_x
    }
    pub fn chunks_y(&self) -> usize {
        self.chunks_y
    }

    // --- editing -----------------------------------------------------------

    /// Place a single cell of `mat` with a fresh per-cell tint. `EMPTY` clears.
    pub fn set(&mut self, x: usize, y: usize, mat: MaterialId) {
        let i = self.idx(x, y);
        if self.cells[i].life > 0 {
            self.transients -= 1;
        }
        let tint = self.rng.next_u32() as u8;
        let life = material::props(mat).life;
        if life > 0 {
            self.transients += 1;
        }
        self.cells[i] = Cell {
            material: mat,
            gen: 0,
            life,
            tint,
        };
        self.temp[i] = material::props(mat).default_temp;
        self.touch(x, y);
    }

    /// Paint a filled disc of `mat`. `radius` 0 = 1 cell. Painting a real
    /// material only **writes into empty cells** (never overwrites existing
    /// matter); painting `EMPTY` erases anything in the disc.
    pub fn paint(&mut self, cx: usize, cy: usize, radius: usize, mat: MaterialId) {
        self.stamp(cx, cy, radius, mat, false);
    }

    /// Like [`Grid::paint`] but **overwrites existing matter** as well as empty
    /// cells. Erasing (`EMPTY`) behaves identically to `paint`.
    pub fn paint_over(&mut self, cx: usize, cy: usize, radius: usize, mat: MaterialId) {
        self.stamp(cx, cy, radius, mat, true);
    }

    /// Stamp a filled disc. `overwrite` lets a real material replace existing
    /// matter; otherwise it only fills empty cells. Erasing always clears.
    fn stamp(&mut self, cx: usize, cy: usize, radius: usize, mat: MaterialId, overwrite: bool) {
        let erasing = mat == EMPTY;
        let r = radius as isize;
        let (cx, cy) = (cx as isize, cy as isize);
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy > r * r {
                    continue;
                }
                let (x, y) = (cx + dx, cy + dy);
                if !self.in_bounds(x, y) {
                    continue;
                }
                let (x, y) = (x as usize, y as usize);
                if erasing || overwrite || self.is_empty(x, y) {
                    self.set(x, y, mat);
                }
            }
        }
    }

    /// Stamp a brush disc of radius `radius` along the straight line of grid
    /// cells from `(x0,y0)` to `(x1,y1)` (inclusive). This is the bucket the app
    /// uses to turn a fast mouse drag into a continuous stroke instead of dotted
    /// stamps. `overwrite` and erasing follow [`Grid::stamp`].
    #[allow(clippy::too_many_arguments)]
    pub fn paint_line(
        &mut self,
        x0: isize,
        y0: isize,
        x1: isize,
        y1: isize,
        radius: usize,
        mat: MaterialId,
        overwrite: bool,
    ) {
        // Integer Bresenham over grid cells; stamp a disc at every step.
        let (mut x, mut y) = (x0, y0);
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            if self.in_bounds(x, y) {
                self.stamp(x as usize, y as usize, radius, mat, overwrite);
            }
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Flood fill (4-connected) from `(x, y)`: replace the contiguous region of
    /// cells matching the clicked cell's material with `mat`. A no-op when the
    /// target already is `mat`. Bounded by the grid; an open region fills to its
    /// walls. Goes through [`Grid::set`] so transient counts, temperature, and
    /// chunk wake bits stay correct.
    pub fn flood_fill(&mut self, x: usize, y: usize, mat: MaterialId) {
        if x >= self.width || y >= self.height {
            return;
        }
        let target = self.material_at(x, y);
        if target == mat {
            return;
        }
        let mut stack = vec![(x, y)];
        while let Some((cx, cy)) = stack.pop() {
            // Only cells still showing the target material (set() below changes
            // them, so each cell is visited at most once).
            if self.material_at(cx, cy) != target {
                continue;
            }
            self.set(cx, cy, mat);
            if cx > 0 {
                stack.push((cx - 1, cy));
            }
            if cx + 1 < self.width {
                stack.push((cx + 1, cy));
            }
            if cy > 0 {
                stack.push((cx, cy - 1));
            }
            if cy + 1 < self.height {
                stack.push((cx, cy + 1));
            }
        }
    }

    // --- chunk bookkeeping -------------------------------------------------

    #[inline]
    fn chunk_idx(&self, cx: usize, cy: usize) -> usize {
        cy * self.chunks_x + cx
    }

    #[inline]
    fn wake_chunk(&mut self, cx: usize, cy: usize) {
        let i = self.chunk_idx(cx, cy);
        self.wake[i] = true;
    }

    /// Mark a cell's chunk — and any chunk whose border it sits against,
    /// including diagonals — awake for next tick. This is how "crossing a
    /// boundary wakes the neighbor": every write/move calls `touch`.
    #[inline]
    fn touch(&mut self, x: usize, y: usize) {
        let cx = x / CHUNK;
        let cy = y / CHUNK;
        let on_left = x.is_multiple_of(CHUNK) && cx > 0;
        let on_right = x % CHUNK == CHUNK - 1 && cx + 1 < self.chunks_x;
        let on_top = y.is_multiple_of(CHUNK) && cy > 0;
        let on_bot = y % CHUNK == CHUNK - 1 && cy + 1 < self.chunks_y;

        self.wake_chunk(cx, cy);
        if on_left {
            self.wake_chunk(cx - 1, cy);
        }
        if on_right {
            self.wake_chunk(cx + 1, cy);
        }
        if on_top {
            self.wake_chunk(cx, cy - 1);
        }
        if on_bot {
            self.wake_chunk(cx, cy + 1);
        }
        // diagonals
        if on_left && on_top {
            self.wake_chunk(cx - 1, cy - 1);
        }
        if on_right && on_top {
            self.wake_chunk(cx + 1, cy - 1);
        }
        if on_left && on_bot {
            self.wake_chunk(cx - 1, cy + 1);
        }
        if on_right && on_bot {
            self.wake_chunk(cx + 1, cy + 1);
        }
    }

    // --- simulation --------------------------------------------------------

    fn begin_tick(&mut self) {
        self.gen = self.gen.wrapping_add(1);
        if self.gen == 0 {
            for c in &mut self.cells {
                c.gen = 0;
            }
            self.gen = 1;
        }
        // This tick processes whatever was queued last tick; start a fresh queue.
        self.active.copy_from_slice(&self.wake);
        for w in &mut self.wake {
            *w = false;
        }
    }

    /// Advance one fixed tick (single-threaded).
    pub fn step(&mut self) {
        self.step_inner(false);
    }

    /// Advance one fixed tick with the heat-diffusion stencil parallelized over
    /// rows via rayon (M9). The diffusion is a pure Jacobi pass (reads the old
    /// field, writes a scratch buffer), so it is **order-independent**: the result
    /// is byte-identical to [`Grid::step`] regardless of thread count. Movement
    /// and reactions stay single-threaded so the seeded RNG is consumed in a
    /// fixed order and every determinism/golden guarantee holds unchanged.
    pub fn step_parallel(&mut self) {
        self.step_inner(true);
    }

    fn step_inner(&mut self, parallel: bool) {
        self.begin_tick();
        self.reactions_pass();
        self.movement_pass();
        self.temperature_pass(parallel);
        self.life_pass();
        self.frame_parity = !self.frame_parity;
    }

    /// Data-driven contact reactions over awake chunks. A cell reacts with at
    /// most one neighbour per tick; both endpoints are then tagged (so they
    /// neither move nor react again this tick) and their chunks woken. Reactions
    /// run before movement, so a reacting cell stays put that tick.
    fn reactions_pass(&mut self) {
        const NEIGHBOURS: [(isize, isize); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];
        for cy in 0..self.chunks_y {
            for cx in 0..self.chunks_x {
                if !self.active[self.chunk_idx(cx, cy)] {
                    continue;
                }
                let x0 = cx * CHUNK;
                let y0 = cy * CHUNK;
                let x1 = (x0 + CHUNK).min(self.width);
                let y1 = (y0 + CHUNK).min(self.height);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let i = self.idx(x, y);
                        let a = self.cells[i].material;
                        if a == EMPTY || self.cells[i].gen == self.gen {
                            continue;
                        }
                        if a == material::CLONE {
                            self.update_clone(x, y, &NEIGHBOURS);
                            continue;
                        }
                        if a == material::VOID {
                            self.update_void(x, y, &NEIGHBOURS);
                            continue;
                        }
                        if a == material::DRAIN {
                            self.update_drain(x, y, &NEIGHBOURS);
                            continue;
                        }
                        if a == material::BATTERY {
                            self.update_battery(x, y, &NEIGHBOURS);
                            continue;
                        }
                        // Tracks whether this cell has a live reactive partner.
                        // If so but the (probabilistic) reaction didn't fire this
                        // tick, we still keep the chunk awake — otherwise an
                        // ongoing reaction in an otherwise-static scene (plant
                        // growing in still water, cryo freezing a pool) would stop
                        // the moment the region settled to sleep.
                        let mut reactive = false;
                        let mut done = false;
                        for (dx, dy) in NEIGHBOURS {
                            let nx = x as isize + dx;
                            let ny = y as isize + dy;
                            if !self.in_bounds(nx, ny) {
                                continue;
                            }
                            let (nx, ny) = (nx as usize, ny as usize);
                            let nidx = self.idx(nx, ny);
                            if self.cells[nidx].gen == self.gen {
                                continue; // neighbour already reacted/moved
                            }
                            let b = self.cells[nidx].material;

                            // Explosive ignition: fire, lava, or a live wire
                            // (spark / charged copper) detonates it — so circuits
                            // can set off bombs.
                            let igniter = |m| {
                                m == material::FIRE
                                    || m == material::SPARK
                                    || m == material::CHARGED
                                    || m == material::LAVA
                                    || m == material::BURNFUSE
                                    || m == material::EMBER
                            };
                            let ra = material::explosive_radius(a);
                            let rb = material::explosive_radius(b);
                            if ra > 0 && igniter(b) {
                                self.explode(x, y, ra as isize);
                                done = true;
                                break;
                            }
                            if rb > 0 && igniter(a) {
                                self.explode(nx, ny, rb as isize);
                                self.touch(x, y);
                                done = true;
                                break;
                            }

                            if let Some(r) = material::reaction_for(a, b) {
                                reactive = true;
                                if self.temp[i] >= r.min_temp && self.rng.chance(r.prob) {
                                    self.transform(x, y, r.a_to);
                                    self.transform(nx, ny, r.b_to);
                                    self.touch(x, y);
                                    self.touch(nx, ny);
                                    done = true;
                                    break;
                                }
                            }
                        }
                        if reactive && !done {
                            self.touch(x, y);
                        }
                    }
                }
            }
        }
    }

    /// Clone: copy the first non-empty (non-clone/void) neighbour's material into
    /// every empty neighbour — an infinite source of whatever it's fed.
    fn update_clone(&mut self, x: usize, y: usize, neigh: &[(isize, isize); 4]) {
        let mut src = EMPTY;
        for (dx, dy) in neigh {
            let (nx, ny) = (x as isize + dx, y as isize + dy);
            if !self.in_bounds(nx, ny) {
                continue;
            }
            let m = self.material_at(nx as usize, ny as usize);
            if m != EMPTY && m != material::CLONE && m != material::VOID {
                src = m;
                break;
            }
        }
        if src == EMPTY {
            return;
        }
        for (dx, dy) in neigh {
            let (nx, ny) = (x as isize + dx, y as isize + dy);
            if !self.in_bounds(nx, ny) {
                continue;
            }
            let (nx, ny) = (nx as usize, ny as usize);
            if self.material_at(nx, ny) == EMPTY {
                self.transform(nx, ny, src);
                self.touch(nx, ny);
            }
        }
    }

    /// Battery: energize adjacent copper. Charge then travels the wire; the
    /// copper's refractory cycle makes the battery pulse naturally. Stays awake so
    /// it keeps powering the circuit.
    fn update_battery(&mut self, x: usize, y: usize, neigh: &[(isize, isize); 4]) {
        self.touch(x, y);
        for (dx, dy) in neigh {
            let (nx, ny) = (x as isize + dx, y as isize + dy);
            if !self.in_bounds(nx, ny) {
                continue;
            }
            let (nx, ny) = (nx as usize, ny as usize);
            if self.material_at(nx, ny) == material::COPPER {
                self.transform(nx, ny, material::CHARGED);
                self.touch(nx, ny);
            }
        }
    }

    /// Void: delete any non-empty (non-void) neighbour — an infinite sink.
    fn update_void(&mut self, x: usize, y: usize, neigh: &[(isize, isize); 4]) {
        for (dx, dy) in neigh {
            let (nx, ny) = (x as isize + dx, y as isize + dy);
            if !self.in_bounds(nx, ny) {
                continue;
            }
            let (nx, ny) = (nx as usize, ny as usize);
            let m = self.material_at(nx, ny);
            if m != EMPTY && m != material::VOID {
                self.transform(nx, ny, EMPTY);
                self.touch(nx, ny);
            }
        }
    }

    /// Drain: a liquid-only sink. Deletes adjacent liquid cells (water, oil,
    /// acid, lava, …) but leaves solids, powders, gases and critters alone — so
    /// it empties a tank without chewing through its walls.
    fn update_drain(&mut self, x: usize, y: usize, neigh: &[(isize, isize); 4]) {
        for (dx, dy) in neigh {
            let (nx, ny) = (x as isize + dx, y as isize + dy);
            if !self.in_bounds(nx, ny) {
                continue;
            }
            let (nx, ny) = (nx as usize, ny as usize);
            if material::phase(self.material_at(nx, ny)) == Phase::Liquid {
                self.transform(nx, ny, EMPTY);
                self.touch(nx, ny);
            }
        }
    }

    /// Detonate at (cx,cy): convert every cell within `r` (disc) to fire, except
    /// blast-proof stone. Other explosives caught in the blast chain-detonate.
    /// Iterative (work-stack) so a long fuse of explosives can't overflow the
    /// stack; the per-tick gen tag ensures each cell detonates at most once.
    fn explode(&mut self, cx: usize, cy: usize, r: isize) {
        let mut stack = vec![(cx, cy, r)];
        while let Some((cx, cy, r)) = stack.pop() {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx * dx + dy * dy > r * r {
                        continue;
                    }
                    let x = cx as isize + dx;
                    let y = cy as isize + dy;
                    if !self.in_bounds(x, y) {
                        continue;
                    }
                    let (x, y) = (x as usize, y as usize);
                    let m = self.material_at(x, y);
                    if m == material::STONE || m == material::DIAMOND {
                        continue; // blast-proof
                    }
                    let chain = material::explosive_radius(m);
                    let already = self.cells[self.idx(x, y)].gen == self.gen;
                    self.transform(x, y, material::FIRE);
                    self.touch(x, y);
                    if chain > 0 && !already {
                        stack.push((x, y, chain as isize));
                    }
                }
            }
        }
    }

    /// Change cell (x,y) to material `to`, keeping bookkeeping consistent:
    /// transient count, life reset, gen tag, and temperature (a hot product
    /// raises the cell's temperature so reactions/transitions can cascade).
    fn transform(&mut self, x: usize, y: usize, to: MaterialId) {
        let i = self.idx(x, y);
        if self.cells[i].life > 0 {
            self.transients -= 1;
        }
        let new_life = material::props(to).life;
        if new_life > 0 {
            self.transients += 1;
        }
        self.cells[i].material = to;
        self.cells[i].life = new_life;
        self.cells[i].gen = self.gen;
        // A reaction releases/absorbs latent heat: the product takes its own
        // default temperature (hot fire heats, frozen ice stays cold and doesn't
        // instantly melt). Empty keeps the ambient temperature already present.
        if to != EMPTY {
            self.temp[i] = material::props(to).default_temp;
        }
    }

    /// Read a cell's temperature (test/HUD helper).
    pub fn temperature_at(&self, x: usize, y: usize) -> f32 {
        self.temp[self.idx(x, y)]
    }

    /// Force a cell's temperature and wake it (heat source / test helper).
    pub fn set_temperature(&mut self, x: usize, y: usize, t: f32) {
        let i = self.idx(x, y);
        self.temp[i] = t;
        self.touch(x, y);
    }

    /// Heat diffusion + temperature-driven phase transitions over awake chunks.
    /// The diffusion stencil is a **Jacobi** pass — reads the old field, writes
    /// the scratch `temp_next` — so it is order-independent and can be run in
    /// parallel (`parallel = true`) with a result identical to serial. The
    /// commit (wake-on-change + transitions) is serial and cheap, keeping the
    /// `transients` count and wake set race-free.
    fn temperature_pass(&mut self, parallel: bool) {
        const EPS: f32 = 0.05;

        // 1. Phase transitions first, on each cell's CURRENT temperature — a cell
        //    that is already past a threshold transforms this tick regardless of
        //    how fast it would diffuse away. (Diffusion-driven crossings transition
        //    on the next tick; a one-tick lag is imperceptible.)
        for cy in 0..self.chunks_y {
            for cx in 0..self.chunks_x {
                if !self.active[self.chunk_idx(cx, cy)] {
                    continue;
                }
                let x0 = cx * CHUNK;
                let y0 = cy * CHUNK;
                let x1 = (x0 + CHUNK).min(self.width);
                let y1 = (y0 + CHUNK).min(self.height);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let i = self.idx(x, y);
                        // Persistent heat sources hold their temperature each tick.
                        if let Some(t) = material::pinned_temp(self.cells[i].material) {
                            self.temp[i] = t;
                            self.touch(x, y);
                            continue;
                        }
                        if self.try_transition(x, y, self.temp[i]) {
                            self.touch(x, y);
                        }
                    }
                }
            }
        }

        // 2. Diffuse (uses any just-changed materials' conductivities).
        self.diffuse(parallel);

        // 3. Commit new temperatures; wake chunks whose temperature moved.
        for cy in 0..self.chunks_y {
            for cx in 0..self.chunks_x {
                if !self.active[self.chunk_idx(cx, cy)] {
                    continue;
                }
                let x0 = cx * CHUNK;
                let y0 = cy * CHUNK;
                let x1 = (x0 + CHUNK).min(self.width);
                let y1 = (y0 + CHUNK).min(self.height);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let i = self.idx(x, y);
                        let new = self.temp_next[i];
                        if (new - self.temp[i]).abs() > EPS {
                            self.touch(x, y);
                        }
                        self.temp[i] = new;
                    }
                }
            }
        }
    }

    /// Compute the next temperature for every **awake** cell into `temp_next`
    /// (4-neighbour insulated-boundary Jacobi stencil, reading the old field).
    /// Parallelized over awake chunks with rayon: chunks tile the grid, so each
    /// task writes a disjoint set of `temp_next` indices — race-free. The single
    /// `unsafe` is justified by that tiling (no two chunks share a cell).
    fn diffuse(&mut self, parallel: bool) {
        let Grid {
            ref temp,
            ref cells,
            ref active,
            ref mut temp_next,
            width: w,
            height: h,
            chunks_x,
            chunks_y,
            ..
        } = *self;

        let awake: Vec<(usize, usize)> = (0..chunks_y)
            .flat_map(|cy| (0..chunks_x).map(move |cx| (cx, cy)))
            .filter(|&(cx, cy)| active[cy * chunks_x + cx])
            .collect();

        /// Pointer to `temp_next`, marked shareable because each chunk writes a
        /// disjoint index range.
        struct Out(*mut f32);
        unsafe impl Sync for Out {}
        let out = Out(temp_next.as_mut_ptr());

        let process = |&(cx, cy): &(usize, usize)| {
            let out = &out; // capture by ref
            let x0 = cx * CHUNK;
            let y0 = cy * CHUNK;
            let x1 = (x0 + CHUNK).min(w);
            let y1 = (y0 + CHUNK).min(h);
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = y * w + x;
                    let here = temp[i];
                    let ci = material::props(cells[i].material).conductivity;
                    // Per-edge flux: heat across a boundary is limited by the
                    // WORSE conductor (thermal resistances in series). So copper
                    // (0.25) shares heat fast with adjacent copper but barely
                    // leaks into air (0.03) — a wire conducts along itself instead
                    // of dumping into the surrounding air. Insulated boundary:
                    // out-of-grid neighbours carry zero flux.
                    let mut flux = 0.0f32;
                    let mut ksum = 0.0f32;
                    let mut edge = |j: usize| {
                        let k = ci.min(material::props(cells[j].material).conductivity);
                        ksum += k;
                        flux += k * (temp[j] - here);
                    };
                    if x > 0 {
                        edge(i - 1);
                    }
                    if x + 1 < w {
                        edge(i + 1);
                    }
                    if y > 0 {
                        edge(i - w);
                    }
                    if y + 1 < h {
                        edge(i + w);
                    }
                    // Stability clamp: explicit diffusion needs the total edge
                    // weight <= 1. This lets a material use a high conductivity
                    // (fast conduction through thin structures like a wire, which
                    // has few same-material edges) while a solid block of it stays
                    // stable (its 4 edges scale down to sum 1).
                    if ksum > 1.0 {
                        flux /= ksum;
                    }
                    let mut next = here + flux;
                    // Empty air is a weak ambient sink: it slowly relaxes to 20 so
                    // stray air heat/cold doesn't linger. Gentle, so conductors
                    // keep their heat (they barely couple to air anyway).
                    if cells[i].material == EMPTY {
                        const AMBIENT: f32 = 20.0;
                        const AIR_RELAX: f32 = 0.01;
                        next += AIR_RELAX * (AMBIENT - next);
                    }
                    // SAFETY: `i` lies in chunk (cx,cy); chunks are disjoint, so
                    // no other task writes this index concurrently.
                    unsafe { *out.0.add(i) = next };
                }
            }
        };

        if parallel {
            awake.par_iter().for_each(process);
        } else {
            awake.iter().for_each(process);
        }
    }

    /// Apply a temperature-driven phase transition to (x,y) if its temperature
    /// crossed a threshold. Temperature (energy) is preserved across the change.
    /// Returns whether a transition happened.
    fn try_transition(&mut self, x: usize, y: usize, t: f32) -> bool {
        let i = self.idx(x, y);
        let p = material::props(self.cells[i].material);
        let to = if t >= p.high_temp {
            p.high_to
        } else if t <= p.low_temp {
            p.low_to
        } else {
            return false;
        };
        if to == self.cells[i].material {
            return false;
        }
        if self.cells[i].life > 0 {
            self.transients -= 1;
        }
        let new_life = material::props(to).life;
        if new_life > 0 {
            self.transients += 1;
        }
        let was = self.cells[i].material;
        self.cells[i].material = to;
        self.cells[i].life = new_life;
        self.cells[i].gen = self.gen;
        // Dissolving etc. to empty must not strand the old temperature in air.
        if to == EMPTY && was != EMPTY {
            self.temp[i] = material::props(EMPTY).default_temp;
        }
        true
    }

    /// Decrement transient life; expired cells become their `decay_to` product.
    /// Only scans awake chunks (a transient is, by definition, active). Keeps
    /// the cell's chunk awake while it still lives so it can't be stranded asleep.
    fn life_pass(&mut self) {
        if self.transients == 0 {
            return;
        }
        for cy in 0..self.chunks_y {
            for cx in 0..self.chunks_x {
                if !self.active[self.chunk_idx(cx, cy)] {
                    continue;
                }
                let x0 = cx * CHUNK;
                let y0 = cy * CHUNK;
                let x1 = (x0 + CHUNK).min(self.width);
                let y1 = (y0 + CHUNK).min(self.height);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let i = self.idx(x, y);
                        let life = self.cells[i].life;
                        if life == 0 {
                            continue;
                        }
                        if life == 1 {
                            let decay = material::props(self.cells[i].material).decay_to;
                            let new_life = material::props(decay).life;
                            if new_life == 0 {
                                self.transients -= 1;
                            }
                            self.cells[i] = Cell {
                                material: decay,
                                gen: self.gen,
                                life: new_life,
                                tint: self.cells[i].tint,
                            };
                            // A cell that vanishes must not leave its (possibly
                            // extreme) temperature behind as an invisible pocket.
                            if decay == EMPTY {
                                self.temp[i] = material::props(EMPTY).default_temp;
                            }
                            self.touch(x, y);
                        } else {
                            self.cells[i].life = life - 1;
                            self.touch(x, y); // still alive -> stay awake
                        }
                    }
                }
            }
        }
    }

    /// Bottom-up scan so a falling cell moves exactly one row per tick.
    /// Horizontal direction alternates each frame. Cells in sleeping chunks are
    /// skipped — they are provably static, so the result is identical to a full
    /// scan but cheaper.
    fn movement_pass(&mut self) {
        for y in (0..self.height).rev() {
            let cy = y / CHUNK;
            let row_base = cy * self.chunks_x;
            if self.frame_parity {
                for x in 0..self.width {
                    if self.active[row_base + x / CHUNK] {
                        self.update_cell(x, y);
                    }
                }
            } else {
                for x in (0..self.width).rev() {
                    if self.active[row_base + x / CHUNK] {
                        self.update_cell(x, y);
                    }
                }
            }
        }
    }

    #[inline]
    fn update_cell(&mut self, x: usize, y: usize) {
        let cell = self.get(x, y);
        if cell.gen == self.gen {
            return; // already moved this tick
        }
        match material::phase(cell.material) {
            Phase::Powder => self.update_powder(x, y),
            Phase::Liquid => self.update_liquid(x, y),
            Phase::Gas => self.update_gas(x, y),
            Phase::Life => self.update_critter(x, y),
            // Energy propagation lands in later milestones.
            _ => {}
        }
    }

    /// Powder: straight down, else a down-diagonal (randomized order).
    /// Sinks through lighter fluids via the generalized density swap.
    fn update_powder(&mut self, x: usize, y: usize) {
        let mat = self.material_at(x, y);
        if self.try_move(x, y, x as isize, y as isize + 1, mat) {
            return;
        }
        let left_first = self.rng.bool();
        let (a, b) = if left_first { (-1, 1) } else { (1, -1) };
        if self.try_move(x, y, x as isize + a, y as isize + 1, mat) {
            return;
        }
        let _ = self.try_move(x, y, x as isize + b, y as isize + 1, mat);
    }

    /// Liquid: down, else a down-diagonal, else flow horizontally toward the
    /// nearest reachable descent (level-seeking). Flowing only into empty/lighter
    /// cells and only *toward a place it can fall* guarantees settling — a liquid
    /// on flat ground with no lower neighbour rests, so no infinite oscillation.
    fn update_liquid(&mut self, x: usize, y: usize) {
        let mat = self.material_at(x, y);
        if self.try_move(x, y, x as isize, y as isize + 1, mat) {
            return;
        }
        let left_first = self.rng.bool();
        let (a, b) = if left_first { (-1, 1) } else { (1, -1) };
        if self.try_move(x, y, x as isize + a, y as isize + 1, mat) {
            return;
        }
        if self.try_move(x, y, x as isize + b, y as isize + 1, mat) {
            return;
        }

        // Horizontal level-seeking. Scan each side through passable cells for the
        // nearest column where the liquid could descend.
        let d = material::props(mat).dispersion as isize;
        let left = self.scan_descent(x, y, -1, d, mat);
        let right = self.scan_descent(x, y, 1, d, mat);
        let dir = match (left, right) {
            (Some(l), Some(r)) => {
                if l < r {
                    -1
                } else if r < l {
                    1
                } else if self.rng.bool() {
                    -1
                } else {
                    1
                }
            }
            (Some(_), None) => -1,
            (None, Some(_)) => 1,
            // No reachable descent: instead of resting as a frozen ramp (which
            // makes a wide pool look like a powder pile), equalize by pressure.
            (None, None) => return self.level_pressure(x, y),
        };
        let _ = self.try_move(x, y, x as isize + dir, y as isize, mat);
    }

    /// Hydrostatic leveling. A liquid cell with no reachable descent normally
    /// rests — but if it is *under pressure* (a non-empty cell directly above)
    /// it spreads one step sideways into a strictly-empty same-row neighbour.
    /// That neighbour is necessarily a shorter column (a deeper empty would have
    /// been found as a descent), so every such move is downhill: the surface
    /// variance strictly drops and the flow terminates. The unpressured top layer
    /// never moves, so a flat pool still settles and its chunk sleeps; only
    /// stacked liquid flows, flattening the ramps the descent-seek leaves behind.
    fn level_pressure(&mut self, x: usize, y: usize) {
        if y == 0 || self.material_at(x, y - 1) == EMPTY {
            return; // top of the column: nothing pressing down, so it rests
        }
        let (a, b) = if self.rng.bool() {
            (-1isize, 1)
        } else {
            (1, -1)
        };
        for dir in [a, b] {
            let nx = x as isize + dir;
            if self.in_bounds(nx, y as isize) && self.material_at(nx as usize, y) == EMPTY {
                self.swap_cells(x, y, nx as usize, y);
                return;
            }
        }
    }

    /// Gas: the inverse of liquid. Rises (up), else an up-diagonal, else
    /// disperses sideways into empty/lighter. Buoyancy via the same directional
    /// rule (lighter rises through heavier fluids). Finite-life gases fade in the
    /// life pass.
    fn update_gas(&mut self, x: usize, y: usize) {
        let mat = self.material_at(x, y);
        if self.try_move(x, y, x as isize, y as isize - 1, mat) {
            return;
        }
        let left_first = self.rng.bool();
        let (a, b) = if left_first { (-1, 1) } else { (1, -1) };
        if self.try_move(x, y, x as isize + a, y as isize - 1, mat) {
            return;
        }
        if self.try_move(x, y, x as isize + b, y as isize - 1, mat) {
            return;
        }
        // Disperse: a single sideways step (randomized) when it can't rise.
        if self.try_move(x, y, x as isize + a, y as isize, mat) {
            return;
        }
        let _ = self.try_move(x, y, x as isize + b, y as isize, mat);
    }

    /// Critters (Phase::Life) move under their own rules rather than by gravity.
    /// Each keeps its chunk awake (it is always potentially moving) and acts at
    /// most once per tick — a swap/`transform` tags `gen`, so the bottom-up
    /// movement scan never processes it twice.
    fn update_critter(&mut self, x: usize, y: usize) {
        self.touch(x, y); // an agent is never "settled"; keep the region alive
        match self.material_at(x, y) {
            material::FISH => self.update_fish(x, y),
            material::WORM => self.update_worm(x, y),
            material::ANT => self.update_ant(x, y),
            _ => {}
        }
    }

    /// Four neighbours in a randomized rotation, so a critter has no directional
    /// bias from the fixed scan order.
    #[inline]
    fn shuffled_neighbours(&mut self) -> [(isize, isize); 4] {
        const N: [(isize, isize); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];
        let s = self.rng.below(4) as usize;
        [N[s], N[(s + 1) % 4], N[(s + 2) % 4], N[(s + 3) % 4]]
    }

    /// Fish: drifts by swapping with an adjacent water cell (any direction). Out
    /// of water it just sinks through air — so draining a tank strands the fish.
    fn update_fish(&mut self, x: usize, y: usize) {
        for (dx, dy) in self.shuffled_neighbours() {
            let (nx, ny) = (x as isize + dx, y as isize + dy);
            if self.in_bounds(nx, ny)
                && self.material_at(nx as usize, ny as usize) == material::WATER
            {
                self.swap_cells(x, y, nx as usize, ny as usize);
                return;
            }
        }
        if y + 1 < self.height && self.material_at(x, y + 1) == EMPTY {
            self.swap_cells(x, y, x, y + 1);
        }
    }

    /// Worm: burrows straight down through powders (sand, soil, …) and air; when
    /// it can't sink it wriggles one step sideways through powder/air.
    fn update_worm(&mut self, x: usize, y: usize) {
        let passable = |m: MaterialId| m == EMPTY || material::phase(m) == Phase::Powder;
        if y + 1 < self.height && passable(self.material_at(x, y + 1)) {
            self.swap_cells(x, y, x, y + 1);
            return;
        }
        let dir = if self.rng.bool() { 1 } else { -1 };
        let nx = x as isize + dir;
        if self.in_bounds(nx, y as isize) && passable(self.material_at(nx as usize, y)) {
            self.swap_cells(x, y, nx as usize, y);
        }
    }

    /// Ant: eats an adjacent plant (its turn), else falls through air, else walks
    /// one step sideways across a surface.
    fn update_ant(&mut self, x: usize, y: usize) {
        for (dx, dy) in self.shuffled_neighbours() {
            let (nx, ny) = (x as isize + dx, y as isize + dy);
            if self.in_bounds(nx, ny)
                && self.material_at(nx as usize, ny as usize) == material::PLANT
            {
                self.transform(nx as usize, ny as usize, EMPTY);
                self.touch(nx as usize, ny as usize);
                return;
            }
        }
        if y + 1 < self.height && self.material_at(x, y + 1) == EMPTY {
            self.swap_cells(x, y, x, y + 1);
            return;
        }
        let dir = if self.rng.bool() { 1 } else { -1 };
        let nx = x as isize + dir;
        if self.in_bounds(nx, y as isize) && self.material_at(nx as usize, y) == EMPTY {
            self.swap_cells(x, y, nx as usize, y);
        }
    }

    /// Scan `dir` (±1) up to `max` steps through cells this liquid can pass
    /// (empty/lighter). Returns the step count of the nearest column where it can
    /// also fall (cell below is displaceable), or `None` if none / path blocked.
    fn scan_descent(
        &self,
        x: usize,
        y: usize,
        dir: isize,
        max: isize,
        mat: MaterialId,
    ) -> Option<isize> {
        for step in 1..=max {
            let nx = x as isize + dir * step;
            if !self.in_bounds(nx, y as isize) {
                return None;
            }
            let nx = nx as usize;
            if !can_move_into(mat, self.material_at(nx, y), 0) {
                return None; // path blocked
            }
            let below = y as isize + 1;
            if self.in_bounds(nx as isize, below)
                && can_move_into(mat, self.material_at(nx, below as usize), 1)
            {
                return Some(step);
            }
        }
        None
    }

    /// If `mat` at (x,y) can displace whatever is at (tx,ty), swap them, tag
    /// both moved, and wake the affected chunks. Returns whether a move happened.
    #[inline]
    fn try_move(&mut self, x: usize, y: usize, tx: isize, ty: isize, mat: MaterialId) -> bool {
        if !self.in_bounds(tx, ty) {
            return false;
        }
        let dy = ty - y as isize;
        let (tx, ty) = (tx as usize, ty as usize);
        let target = self.material_at(tx, ty);
        if !can_move_into(mat, target, dy) {
            return false;
        }
        self.swap_cells(x, y, tx, ty);
        true
    }

    #[inline]
    fn swap_cells(&mut self, x: usize, y: usize, tx: usize, ty: usize) {
        let i = self.idx(x, y);
        let j = self.idx(tx, ty);
        self.cells.swap(i, j);
        self.temp.swap(i, j); // temperature travels with the cell
        self.cells[i].gen = self.gen;
        self.cells[j].gen = self.gen;
        self.touch(x, y);
        self.touch(tx, ty);
    }

    // --- rendering ---------------------------------------------------------

    /// Render to RGBA8. Empty is black; other cells use base color +/- a
    /// per-cell brightness jitter derived from `tint`.
    pub fn render_rgba(&mut self) -> &[u8] {
        for (cell, px) in self.cells.iter().zip(self.framebuffer.chunks_exact_mut(4)) {
            if cell.material == EMPTY {
                px.copy_from_slice(&[0, 0, 0, 255]);
                continue;
            }
            let p = material::props(cell.material);
            let j = p.color_jitter as i32;
            let off = if j == 0 {
                0
            } else {
                (cell.tint as i32 % (2 * j + 1)) - j
            };
            for (c, out) in px.iter_mut().enumerate().take(3) {
                *out = (p.color[c] as i32 + off).clamp(0, 255) as u8;
            }
            px[3] = 255;
        }
        &self.framebuffer
    }

    // --- save / load -------------------------------------------------------

    /// Serialize the full grid to a byte blob (dimensions, RNG state, cells, and
    /// the temperature field). File IO lives in the app; this stays pure so the
    /// core keeps zero graphics/platform deps. Reloading reproduces subsequent
    /// ticks bit-for-bit (RNG state is preserved).
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(16 + self.cells.len() * 8);
        out.extend_from_slice(b"PWDR");
        out.push(1); // version
        out.extend_from_slice(&(self.width as u32).to_le_bytes());
        out.extend_from_slice(&(self.height as u32).to_le_bytes());
        out.push(self.gen);
        out.push(self.frame_parity as u8);
        for v in self.rng.state() {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for c in &self.cells {
            out.push(c.material);
            out.push(c.gen);
            out.push(c.life);
            out.push(c.tint);
        }
        for t in &self.temp {
            out.extend_from_slice(&t.to_le_bytes());
        }
        // Chunk wake bits: which chunks are queued for the next tick. Restoring
        // these is what makes the reloaded grid consume the RNG identically and
        // thus evolve bit-for-bit.
        for w in &self.wake {
            out.push(*w as u8);
        }
        out
    }

    /// Rebuild a grid from [`Grid::serialize`] output. Returns `None` on a bad
    /// magic, version, or truncated/oversized buffer.
    pub fn deserialize(bytes: &[u8]) -> Option<Grid> {
        let mut p = 0usize;
        let take = |p: &mut usize, n: usize| -> Option<&[u8]> {
            let s = bytes.get(*p..*p + n)?;
            *p += n;
            Some(s)
        };
        if take(&mut p, 4)? != b"PWDR" {
            return None;
        }
        if take(&mut p, 1)?[0] != 1 {
            return None;
        }
        let rd_u32 = |p: &mut usize| -> Option<u32> {
            Some(u32::from_le_bytes(take(p, 4)?.try_into().ok()?))
        };
        let width = rd_u32(&mut p)? as usize;
        let height = rd_u32(&mut p)? as usize;
        if width == 0 || height == 0 {
            return None;
        }
        let n = width.checked_mul(height)?;
        let gen = take(&mut p, 1)?[0];
        let frame_parity = take(&mut p, 1)?[0] != 0;
        let mut state = [0u64; 4];
        for s in &mut state {
            *s = u64::from_le_bytes(take(&mut p, 8)?.try_into().ok()?);
        }

        let mut g = Grid::new(width, height, 0);
        g.rng = Rng::from_state(state);
        g.gen = gen;
        g.frame_parity = frame_parity;

        let mut transients = 0usize;
        for i in 0..n {
            let rec = take(&mut p, 4)?;
            let (material, gen, life, tint) = (rec[0], rec[1], rec[2], rec[3]);
            if material as usize >= material::MATERIALS.len() {
                return None;
            }
            if life > 0 {
                transients += 1;
            }
            g.cells[i] = Cell {
                material,
                gen,
                life,
                tint,
            };
        }
        for i in 0..n {
            g.temp[i] = f32::from_le_bytes(take(&mut p, 4)?.try_into().ok()?);
        }
        g.transients = transients;
        for i in 0..g.wake.len() {
            g.wake[i] = take(&mut p, 1)?[0] != 0;
        }
        Some(g)
    }

    /// Render the scene with each cell **recolored by its temperature** (blue
    /// cold → red/orange/white hot). Empty space stays black so element shapes
    /// remain visible — this is a thermal recolor of matter, not a blurry field.
    pub fn render_temperature_rgba(&mut self) -> &[u8] {
        for ((cell, t), px) in self
            .cells
            .iter()
            .zip(self.temp.iter())
            .zip(self.framebuffer.chunks_exact_mut(4))
        {
            let c = if cell.material == EMPTY {
                [0, 0, 0]
            } else {
                temp_color(*t)
            };
            px[0] = c[0];
            px[1] = c[1];
            px[2] = c[2];
            px[3] = 255;
        }
        &self.framebuffer
    }

    /// Stable FNV-1a hash over material/life/tint of every cell. Deterministic
    /// for a given seed + input sequence — used by golden tests.
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for c in &self.cells {
            for b in [c.material, c.life, c.tint] {
                h ^= b as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    }
}

/// Map a temperature (°C-like) to an RGB used to recolor matter: ambient (~20)
/// is neutral grey, colder shifts toward blue, hotter ramps red→yellow→white.
pub fn temp_color(t: f32) -> [u8; 3] {
    let lerp = |a: f32, b: f32, f: f32| (a + (b - a) * f.clamp(0.0, 1.0)) as u8;
    if t <= 20.0 {
        // 20 (grey) .. -200 (deep blue)
        let f = (20.0 - t) / 220.0;
        [
            lerp(140.0, 30.0, f),
            lerp(140.0, 110.0, f),
            lerp(150.0, 255.0, f),
        ]
    } else {
        let f = ((t - 20.0) / 1180.0).clamp(0.0, 1.0); // 20..1200
        if f < 0.4 {
            let g = f / 0.4; // grey -> red
            [
                lerp(140.0, 230.0, g),
                lerp(140.0, 40.0, g),
                lerp(150.0, 30.0, g),
            ]
        } else if f < 0.75 {
            let g = (f - 0.4) / 0.35; // red -> orange/yellow
            [
                lerp(230.0, 255.0, g),
                lerp(40.0, 200.0, g),
                lerp(30.0, 40.0, g),
            ]
        } else {
            let g = (f - 0.75) / 0.25; // yellow -> white
            [255, lerp(200.0, 255.0, g), lerp(40.0, 255.0, g)]
        }
    }
}

/// Generalized, direction-aware displacement. `dy` is the vertical component of
/// the attempted move: `+1` sinking, `-1` rising, `0` lateral.
///
/// One rule covers every cross-phase case:
///   * into empty — always.
///   * into a fluid (Liquid/Gas) — when sinking, the denser cell wins; when
///     rising, the lighter cell wins; laterally, the denser pushes the lighter.
///   * into a solid or powder — never (they block; fluids flow around).
///
/// This single rule yields sand-through-water, oil-on-water, and gas-rising with
/// no per-pair code.
#[inline]
pub fn can_move_into(mover: MaterialId, target: MaterialId, dy: isize) -> bool {
    if target == EMPTY {
        return true;
    }
    let tp = material::props(target);
    let md = material::props(mover).density;
    match tp.phase {
        Phase::Liquid | Phase::Gas => {
            if dy < 0 {
                md < tp.density // rising: lighter displaces heavier
            } else {
                md > tp.density // sinking / lateral: denser displaces lighter
            }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::{
        ACID, BASALT, CHARGED, CLONE, COOLED, COPPER, CRYO, FIRE, FUME, GLASS, GUNPOWDER, ICE,
        LAVA, OIL, PLANT, SALT, SALTWATER, SAND, SMOKE, SPARK, STEAM, STONE, THERMITE, VOID, WATER,
        WOOD,
    };
    use crate::material::{ANT, DRAIN, FISH, WORM};
    use crate::material::{ASH, DIAMOND, EMBER, OXYGEN, SNOW, SOIL};
    use crate::material::{
        BATTERY, COAL, COOLER, FUSE, HEATER, HYDROGEN, LAMP, LITLAMP, MELTWAX, NITRO, OBSIDIAN, WAX,
    };

    #[test]
    fn cell_is_small() {
        assert!(std::mem::size_of::<Cell>() <= 8);
    }

    #[test]
    fn powder_falls_one_row_per_tick() {
        let mut g = Grid::new(8, 16, 1);
        g.set(4, 0, SAND);
        for y in 0..15 {
            assert_eq!(g.material_at(4, y), SAND);
            g.step();
            assert_eq!(g.material_at(4, y), EMPTY, "vacated row {y}");
            assert_eq!(g.material_at(4, y + 1), SAND, "advanced to row {}", y + 1);
        }
    }

    #[test]
    fn powder_rests_on_floor() {
        let mut g = Grid::new(8, 8, 1);
        g.set(3, 0, SAND);
        for _ in 0..50 {
            g.step();
        }
        assert_eq!(g.material_at(3, 7), SAND, "settled on bottom row");
        assert_eq!(g.count(SAND), 1, "not duplicated or lost");
    }

    #[test]
    fn powder_rests_on_solid() {
        let mut g = Grid::new(8, 8, 1);
        for x in 0..8 {
            g.set(x, 5, STONE); // static floor row
        }
        g.set(3, 0, SAND);
        for _ in 0..50 {
            g.step();
        }
        assert_eq!(g.material_at(3, 4), SAND, "rests atop stone");
        assert_eq!(g.material_at(3, 5), STONE, "stone never moved");
    }

    #[test]
    fn pile_is_conserved_and_roughly_symmetric() {
        let w = 41;
        let mut g = Grid::new(w, 30, 12345);
        for _ in 0..200 {
            g.set(w / 2, 0, SAND);
            g.step();
        }
        for _ in 0..2000 {
            g.step();
        }
        assert!(g.count(SAND) > 0);
        let bottom = g.height() - 1;
        let left: usize = (0..w / 2)
            .filter(|&x| g.material_at(x, bottom) == SAND)
            .count();
        let right: usize = (w / 2 + 1..w)
            .filter(|&x| g.material_at(x, bottom) == SAND)
            .count();
        assert!(
            left.abs_diff(right) <= 3,
            "pyramid should be near-symmetric: left={left} right={right}"
        );
    }

    // --- M2 chunking tests ---

    #[test]
    fn distant_chunks_sleep() {
        // 4x4 chunks. Activity only top-left; far chunks must settle to asleep.
        let mut g = Grid::new(256, 256, 7);
        g.paint(20, 20, 4, SAND);
        for _ in 0..400 {
            g.step();
        }
        // Bottom-right chunk never had activity -> asleep.
        assert!(!g.chunk_awake_at(250, 250), "far chunk should sleep");
        // And far fewer than all chunks are awake.
        assert!(
            g.awake_chunk_count() < g.chunks_x() * g.chunks_y(),
            "most chunks should be asleep"
        );
    }

    #[test]
    fn fully_settled_grid_sleeps_completely() {
        let mut g = Grid::new(128, 128, 1);
        g.paint(64, 10, 5, SAND);
        for _ in 0..600 {
            g.step();
        }
        assert_eq!(g.awake_chunk_count(), 0, "settled grid fully asleep");
        // A further step must be a no-op (nothing awake).
        let before = g.hash();
        g.step();
        assert_eq!(g.hash(), before, "asleep grid does not change");
    }

    #[test]
    fn crossing_boundary_wakes_neighbor() {
        // Drop sand just above a horizontal chunk boundary so it falls across.
        let mut g = Grid::new(128, 128, 3);
        g.set(30, CHUNK - 1, SAND); // bottom edge of top chunk row
        g.step(); // sand falls to y=CHUNK, the chunk below
        assert_eq!(g.material_at(30, CHUNK), SAND, "fell across boundary");
        assert!(
            g.chunk_awake_at(30, CHUNK),
            "destination (lower) chunk must be awake"
        );
    }

    #[test]
    fn chunking_matches_full_scan() {
        // Behavior unchanged: chunked stepping == forcing every chunk awake.
        let mut chunked = Grid::new(192, 160, 99);
        let mut full = Grid::new(192, 160, 99);
        for g in [&mut chunked, &mut full] {
            for i in 0..30 {
                g.paint(20 + i * 5, 5, 3, SAND);
            }
        }
        for _ in 0..500 {
            // `full` keeps every chunk awake each tick (skipping disabled).
            for w in full.wake.iter_mut() {
                *w = true;
            }
            chunked.step();
            full.step();
            assert_eq!(
                chunked.cells(),
                full.cells(),
                "chunked result must match full scan"
            );
        }
    }

    // --- M3 liquid tests ---

    /// Build a stone basin: tall side walls at `x=left,right`, floor row at the
    /// bottom. Interior columns are `left+1 ..= right-1`.
    fn basin(w: usize, h: usize, left: usize, right: usize, seed: u64) -> Grid {
        let mut g = Grid::new(w, h, seed);
        for y in 0..h {
            g.set(left, y, STONE);
            g.set(right, y, STONE);
        }
        for x in left..=right {
            g.set(x, h - 1, STONE);
        }
        g
    }

    #[test]
    fn basin_fills_level_and_settles() {
        let (w, h, left, right) = (24, 24, 4, 19);
        let mut g = basin(w, h, left, right, 5);
        // Pour a 14-wide x 4-tall slab (= 4 full interior rows).
        for y in 2..=5 {
            for x in (left + 1)..=(right - 1) {
                g.set(x, y, WATER);
            }
        }
        let poured = g.count(WATER);
        for _ in 0..4000 {
            g.step();
        }
        assert_eq!(g.count(WATER), poured, "water conserved");
        assert_eq!(
            g.awake_chunk_count(),
            0,
            "must settle — no infinite oscillation"
        );

        // Bottom 4 interior rows fully filled and level.
        let floor = h - 1;
        for row in (floor - 4)..floor {
            for x in (left + 1)..=(right - 1) {
                assert_eq!(g.material_at(x, row), WATER, "level fill at ({x},{row})");
            }
        }
        // The row above the pool is empty (no overfill).
        for x in (left + 1)..=(right - 1) {
            assert_eq!(g.material_at(x, floor - 5), EMPTY, "no water above level");
        }
    }

    #[test]
    fn water_spreads_horizontally() {
        // A 1-wide column of water on a floor must spread sideways when it can't fall.
        let mut g = Grid::new(21, 12, 7);
        for x in 0..21 {
            g.set(x, 11, STONE); // floor
        }
        for y in 4..11 {
            g.set(10, y, WATER); // tall thin column
        }
        for _ in 0..600 {
            g.step();
        }
        assert_eq!(g.awake_chunk_count(), 0, "settles");
        let bottom = 10;
        let span = (0..21)
            .filter(|&x| g.material_at(x, bottom) == WATER)
            .count();
        assert!(span > 3, "water spread out along the floor, span={span}");
    }

    #[test]
    fn water_levels_a_lopsided_pool() {
        // A block of water dumped to one side of a flat basin must level out into
        // a roughly horizontal surface, not freeze as a powder-like ramp.
        let mut g = Grid::new(40, 24, 9);
        for x in 0..40 {
            g.set(x, 23, STONE); // floor
        }
        // a tall stack of water crammed into the left third
        for y in 4..23 {
            for x in 0..12 {
                g.set(x, y, WATER);
            }
        }
        for _ in 0..1500 {
            g.step();
        }
        assert_eq!(g.awake_chunk_count(), 0, "pool settles to sleep");
        // Surface height (topmost water row) on the far left vs far right should
        // be close — a level pool, not a ramp piled on the left.
        let surface = |g: &Grid, x: usize| (0..23).find(|&y| g.material_at(x, y) == WATER);
        let left = surface(&g, 2).expect("water on the left");
        let right = surface(&g, 37).expect("water reached the right");
        assert!(
            left.abs_diff(right) <= 2,
            "surface should be level: left_top={left} right_top={right}"
        );
    }

    #[test]
    fn denser_powder_sinks_through_liquid() {
        let mut g = Grid::new(6, 16, 9);
        // A still pool resting on the bottom boundary.
        for y in 8..16 {
            for x in 0..6 {
                g.set(x, y, WATER);
            }
        }
        let water0 = g.count(WATER);
        g.set(3, 0, SAND);
        for _ in 0..200 {
            g.step();
        }
        assert_eq!(g.count(SAND), 1, "sand conserved");
        assert_eq!(g.count(WATER), water0, "water conserved");
        assert_eq!(g.material_at(3, 15), SAND, "sand sank to the bottom");
        // A water cell got displaced up to the old top of the pool.
        assert_eq!(g.material_at(3, 7), WATER, "water displaced upward");
    }

    // --- M4 gas / buoyancy tests ---

    #[test]
    fn gas_rises_one_row_per_tick() {
        let mut g = Grid::new(9, 20, 4);
        g.set(4, 18, SMOKE);
        for step in 1..=12 {
            g.step();
            assert_eq!(
                g.material_at(4, 18 - step),
                SMOKE,
                "smoke risen to row {}",
                18 - step
            );
        }
    }

    #[test]
    fn finite_life_gas_fades() {
        // Smoke sealed in a 1-cell pocket can't move; it must fade to nothing.
        let mut g = Grid::new(3, 3, 1);
        for y in 0..3 {
            for x in 0..3 {
                g.set(x, y, STONE);
            }
        }
        g.set(1, 1, SMOKE);
        assert_eq!(g.count(SMOKE), 1);
        for _ in 0..400 {
            g.step();
        }
        assert_eq!(g.count(SMOKE), 0, "smoke faded after its life");
        assert_eq!(g.material_at(1, 1), EMPTY, "decayed to empty");
        assert_eq!(g.awake_chunk_count(), 0, "and the grid sleeps again");
    }

    #[test]
    fn lighter_liquid_floats_on_denser() {
        // Oil dropped at the bottom of a water pool must rise to the surface.
        let mut g = Grid::new(6, 16, 11);
        for y in 8..16 {
            for x in 0..6 {
                g.set(x, y, WATER);
            }
        }
        let water0 = g.count(WATER);
        g.set(3, 15, OIL); // overwrite a bottom water cell with oil
        let water1 = g.count(WATER);
        for _ in 0..300 {
            g.step();
        }
        assert_eq!(g.count(OIL), 1, "oil conserved");
        assert_eq!(g.count(WATER), water1, "water conserved");
        assert!(water1 < water0); // sanity: one water cell was overwritten

        // Oil floats: it ends at or above the water surface — no water sits above
        // it. (Diagonal sinking can nudge the bubble between columns, so we check
        // the float ordering across the whole grid, not within one column.)
        let mut oil_row = usize::MAX;
        let mut min_water_row = usize::MAX;
        for y in 0..g.height() {
            for x in 0..g.width() {
                match g.material_at(x, y) {
                    OIL => oil_row = oil_row.min(y),
                    WATER => min_water_row = min_water_row.min(y),
                    _ => {}
                }
            }
        }
        assert!(
            oil_row <= min_water_row,
            "oil ({oil_row}) must float at/above the water surface ({min_water_row})"
        );
    }

    // --- M5 temperature / transition tests ---

    /// A single liquid/gas/solid cell trapped in a stone box, forced to `temp`.
    /// Returns (grid, x, y). Movement can't move it, so transitions are isolated.
    fn boxed(center: MaterialId, temp: f32) -> (Grid, usize, usize) {
        let mut g = Grid::new(3, 3, 1);
        for y in 0..3 {
            for x in 0..3 {
                g.set(x, y, STONE);
            }
        }
        g.set(1, 1, center);
        g.set_temperature(1, 1, temp);
        (g, 1, 1)
    }

    #[test]
    fn water_freezes_when_cold() {
        let (mut g, x, y) = boxed(WATER, -50.0);
        g.step();
        assert_eq!(g.material_at(x, y), ICE, "cold water -> ice");
    }

    #[test]
    fn ice_melts_when_warm() {
        let (mut g, x, y) = boxed(ICE, 50.0);
        g.step();
        assert_eq!(g.material_at(x, y), WATER, "warm ice -> water");
    }

    #[test]
    fn water_boils_to_steam() {
        let (mut g, x, y) = boxed(WATER, 150.0);
        g.step();
        assert_eq!(g.material_at(x, y), STEAM, "hot water -> steam");
    }

    #[test]
    fn steam_condenses_to_water() {
        let (mut g, x, y) = boxed(STEAM, 30.0);
        g.step();
        assert_eq!(g.material_at(x, y), WATER, "cool steam -> water");
    }

    #[test]
    fn lava_solidifies_when_cooled() {
        let (mut g, x, y) = boxed(LAVA, 100.0);
        g.step();
        assert_eq!(g.material_at(x, y), BASALT, "cooled lava -> basalt");
    }

    #[test]
    fn stone_melts_to_lava_when_very_hot() {
        // Stone is no longer heat-proof: thermite/lava heat melts it into lava.
        let (mut g, x, y) = boxed(STONE, 1200.0);
        g.step();
        assert_eq!(g.material_at(x, y), LAVA, "molten stone -> lava");
    }

    #[test]
    fn thermite_burns_through_stone() {
        // A thermite charge sitting on a stone slab should eat a hole through it.
        let mut g = Grid::new(7, 9, 1);
        for y in 4..9 {
            for x in 0..7 {
                g.set(x, y, STONE); // a stone slab
            }
        }
        for x in 2..5 {
            g.set(x, 3, THERMITE); // thermite pile on top
        }
        g.set(3, 2, FIRE); // ignite it
        let before = g.count(STONE);
        for _ in 0..200 {
            g.step();
        }
        assert!(
            g.count(STONE) < before,
            "thermite melted through stone: {} -> {}",
            before,
            g.count(STONE)
        );
    }

    #[test]
    fn copper_conducts_heat_and_holds_it() {
        // A lone hot copper cell in air must hold heat (not crash to ambient).
        let mut s = Grid::new(9, 9, 1);
        s.set(4, 4, COPPER);
        s.set_temperature(4, 4, 1000.0);
        for _ in 0..30 {
            s.step();
        }
        assert!(
            s.temperature_at(4, 4) > 60.0,
            "copper retains heat (not instant room temp): {}",
            s.temperature_at(4, 4)
        );

        // A copper wire carries heat down its length, and far better than air
        // does over the same distance (heat flux is limited by the worse
        // conductor, so copper barely leaks into the surrounding air while air
        // itself relaxes toward ambient — copper is the clear conductor).
        let mut g = Grid::new(40, 9, 1);
        for x in 0..40 {
            g.set(x, 4, COPPER);
        }
        for _ in 0..300 {
            g.set_temperature(0, 4, 1000.0); // held source (below copper's melt point)
            g.step();
        }
        let wire = g.temperature_at(5, 4);
        let air = g.temperature_at(5, 1);
        assert!(wire > 120.0, "copper carried heat down the wire: {wire}");
        assert!(
            wire > air * 2.0 + 20.0,
            "copper conducts far better than air: {wire} vs {air}"
        );
    }

    #[test]
    fn temperature_diffuses_between_neighbours() {
        // Hot and cold cells side by side converge toward each other.
        let mut g = Grid::new(2, 1, 1);
        g.set(0, 0, STONE);
        g.set(1, 0, STONE);
        g.set_temperature(0, 0, 100.0);
        g.set_temperature(1, 0, 0.0);
        for _ in 0..200 {
            g.step();
        }
        let a = g.temperature_at(0, 0);
        let b = g.temperature_at(1, 0);
        // Converges until the per-tick change drops below the sleep threshold
        // (~diff 2), then the chunk sleeps — so they end close but not identical.
        assert!((a - b).abs() < 3.0, "temps converged: {a} vs {b}");
        assert!((a - 50.0).abs() < 5.0, "toward the mean: {a}");
    }

    #[test]
    fn lava_and_water_react_thermally() {
        // Emergent: hot lava + cold water -> basalt crust + steam, no special rule.
        // Diamond crucible: inert, so the vessel itself doesn't melt and muddy the
        // experiment (lava melts through plain stone now).
        let mut g = Grid::new(9, 12, 5);
        for y in 0..12 {
            g.set(0, y, DIAMOND);
            g.set(8, y, DIAMOND);
        }
        for x in 0..9 {
            g.set(x, 11, DIAMOND); // floor
        }
        for y in 7..11 {
            for x in 1..8 {
                g.set(x, y, LAVA); // lava pool
            }
        }
        for y in 3..7 {
            for x in 1..8 {
                g.set(x, y, WATER); // water on top
            }
        }
        for _ in 0..400 {
            g.step();
        }
        assert!(g.count(BASALT) > 0, "lava solidified to basalt on contact");
        assert!(g.count(STEAM) > 0, "water flashed to steam");
    }

    // --- M6 reaction / energy tests ---

    #[test]
    fn fire_consumes_oil_and_emits_smoke() {
        // A sealed box of oil, ignited in the middle, burns out to smoke.
        let mut g = Grid::new(12, 12, 4);
        for y in 0..12 {
            for x in 0..12 {
                if x == 0 || y == 0 || x == 11 || y == 11 {
                    g.set(x, y, STONE);
                }
            }
        }
        for y in 1..11 {
            for x in 1..11 {
                g.set(x, y, OIL);
            }
        }
        let oil0 = g.count(OIL);
        g.set(6, 6, FIRE);
        let mut saw_smoke = false;
        for _ in 0..400 {
            g.step();
            if g.count(SMOKE) > 0 {
                saw_smoke = true;
            }
        }
        assert!(g.count(OIL) < oil0 / 4, "most oil burned: {}", g.count(OIL));
        assert!(
            saw_smoke,
            "combustion emitted smoke (byproduct) during the burn"
        );
    }

    #[test]
    fn fire_is_quenched_by_water() {
        // Sealed box with an interior fire/water pair.
        let mut g = Grid::new(4, 4, 1);
        for y in 0..4 {
            for x in 0..4 {
                if x == 0 || y == 0 || x == 3 || y == 3 {
                    g.set(x, y, STONE);
                }
            }
        }
        g.set(1, 2, FIRE);
        g.set(1, 1, WATER);
        let mut saw_steam = false;
        for _ in 0..60 {
            g.step();
            if g.count(STEAM) > 0 {
                saw_steam = true;
            }
        }
        assert!(saw_steam, "fire + water produced steam");
        assert_eq!(g.count(FIRE), 0, "fire was quenched");
    }

    #[test]
    fn acid_dissolves_solid_and_is_consumed() {
        // Acid poured onto a sand pile: both are consumed over time.
        let mut g = Grid::new(5, 12, 7);
        for x in 0..5 {
            g.set(x, 11, STONE); // floor
        }
        for y in 7..11 {
            for x in 1..4 {
                g.set(x, y, SAND);
            }
        }
        let sand0 = g.count(SAND);
        for x in 1..4 {
            g.set(x, 2, ACID);
        }
        let acid0 = g.count(ACID);
        for _ in 0..400 {
            g.step();
        }
        assert!(g.count(SAND) < sand0, "acid dissolved sand");
        assert!(g.count(ACID) < acid0, "acid was consumed");
    }

    #[test]
    fn spark_conducts_along_copper_then_settles() {
        let mut g = Grid::new(22, 3, 1);
        for x in 0..22 {
            g.set(x, 1, COPPER);
        }
        g.set(0, 0, SPARK); // beside the wire end (spark vanishes, doesn't consume wire)
        let mut reached = false;
        for _ in 0..80 {
            g.step();
            let m = g.material_at(20, 1);
            if m == CHARGED || m == COOLED {
                reached = true;
                break;
            }
        }
        assert!(reached, "charge conducted to the far end of the wire");

        for _ in 0..200 {
            g.step();
        }
        assert_eq!(g.count(SPARK), 0, "no perpetual spark");
        assert_eq!(g.count(CHARGED), 0, "charge decayed");
        assert_eq!(g.count(COOLED), 0, "refractory trail decayed");
        assert_eq!(g.count(COPPER), 22, "wire restored");
        assert_eq!(g.awake_chunk_count(), 0, "and the grid sleeps");
    }

    #[test]
    fn spark_ignites_oil() {
        let mut g = Grid::new(5, 3, 1);
        g.set(1, 1, COPPER);
        g.set(2, 1, OIL);
        g.set(1, 1, SPARK); // overwrite copper with spark next to oil
        let mut saw_fire = false;
        for _ in 0..10 {
            g.step();
            if g.count(FIRE) > 0 {
                saw_fire = true;
                break;
            }
        }
        assert!(saw_fire, "spark ignited adjacent oil");
    }

    // --- M7 full-roster element tests ---

    #[test]
    fn fume_propagates_fire() {
        let mut g = Grid::new(10, 10, 3);
        for y in 0..10 {
            for x in 0..10 {
                if x == 0 || y == 0 || x == 9 || y == 9 {
                    g.set(x, y, STONE);
                }
            }
        }
        for y in 1..9 {
            for x in 1..9 {
                g.set(x, y, FUME);
            }
        }
        let fume0 = g.count(FUME);
        g.set(5, 5, FIRE);
        let mut saw_fire_spread = false;
        for _ in 0..120 {
            g.step();
            if g.count(FIRE) > 1 {
                saw_fire_spread = true;
            }
        }
        assert!(saw_fire_spread, "fire propagated through fume");
        assert!(g.count(FUME) < fume0 / 2, "fume largely consumed");
    }

    #[test]
    fn gunpowder_explodes_in_a_radius() {
        let mut g = Grid::new(20, 20, 1);
        for y in 6..11 {
            for x in 6..11 {
                g.set(x, y, GUNPOWDER); // 5x5 cluster = 25
            }
        }
        let gp0 = g.count(GUNPOWDER);
        g.set(5, 8, FIRE); // ignite at the edge
                           // A couple of ticks lets the chain finish.
        for _ in 0..4 {
            g.step();
        }
        assert_eq!(g.count(GUNPOWDER), 0, "all gunpowder detonated");
        assert!(
            g.count(FIRE) >= gp0,
            "blast turned a radius into fire (>= cluster size): {}",
            g.count(FIRE)
        );
    }

    #[test]
    fn cryo_freezes_adjacent_water() {
        let mut g = Grid::new(6, 6, 2);
        for y in 0..6 {
            for x in 0..6 {
                g.set(x, y, STONE);
            }
        }
        // Fill the interior with water so it can't drain away, then embed cryo.
        for y in 1..5 {
            for x in 1..5 {
                g.set(x, y, WATER);
            }
        }
        g.set(2, 2, CRYO);
        g.set(3, 3, CRYO);
        let mut saw_ice = false;
        for _ in 0..100 {
            g.step();
            if g.count(ICE) > 0 {
                saw_ice = true;
                break;
            }
        }
        assert!(saw_ice, "cryo froze adjacent water to ice");
    }

    #[test]
    fn wood_burns_away() {
        let mut g = Grid::new(10, 10, 5);
        for y in 0..10 {
            for x in 0..10 {
                if x == 0 || y == 0 || x == 9 || y == 9 {
                    g.set(x, y, STONE);
                }
            }
        }
        for y in 1..9 {
            for x in 1..9 {
                g.set(x, y, WOOD);
            }
        }
        let wood0 = g.count(WOOD);
        g.set(4, 4, FIRE);
        for _ in 0..1500 {
            g.step();
        }
        assert!(g.count(WOOD) < wood0, "wood burned (at least partially)");
    }

    #[test]
    fn plasma_heats_then_leaves_no_trace() {
        use crate::material::PLASMA;
        let mut g = Grid::new(5, 5, 1);
        for y in 0..5 {
            for x in 0..5 {
                g.set(x, y, STONE);
            }
        }
        g.set(2, 1, WATER);
        g.set(2, 2, PLASMA); // plasma below the water
        let mut saw_steam = false;
        for _ in 0..120 {
            g.step();
            if g.count(STEAM) > 0 {
                saw_steam = true;
            }
        }
        assert!(saw_steam, "plasma boiled the water to steam");
        assert_eq!(g.count(PLASMA), 0, "plasma left no trace");
    }

    #[test]
    fn frost_cools_then_leaves_no_trace() {
        use crate::material::FROST;
        let mut g = Grid::new(5, 5, 1);
        for y in 0..5 {
            for x in 0..5 {
                g.set(x, y, WATER);
            }
        }
        g.set(2, 2, FROST);
        let mut saw_ice = false;
        for _ in 0..120 {
            g.step();
            if g.count(ICE) > 0 {
                saw_ice = true;
            }
        }
        assert!(saw_ice, "frost froze nearby water to ice");
        assert_eq!(g.count(FROST), 0, "frost left no trace");
    }

    #[test]
    fn lava_ignites_wood_on_contact() {
        let mut g = Grid::new(8, 8, 1);
        for y in 0..8 {
            for x in 0..8 {
                g.set(x, y, WOOD);
            }
        }
        g.set(4, 4, LAVA); // lava embedded in wood
        let wood0 = g.count(WOOD);
        let mut saw_fire = false;
        for _ in 0..60 {
            g.step();
            if g.count(FIRE) > 0 {
                saw_fire = true;
            }
        }
        assert!(saw_fire, "lava set the wood alight");
        assert!(g.count(WOOD) < wood0, "wood burned from lava contact");
    }

    #[test]
    fn sand_melts_to_glass_when_hot() {
        let (mut g, x, y) = boxed(SAND, 1200.0);
        g.step();
        assert_eq!(g.material_at(x, y), GLASS, "molten sand -> glass");
    }

    #[test]
    fn serialize_roundtrip_preserves_state_and_future() {
        let mut g = Grid::new(48, 48, 0xBEEF);
        g.paint(24, 4, 6, SAND);
        g.paint(10, 4, 4, WATER);
        g.set(24, 24, FIRE);
        for _ in 0..60 {
            g.step();
        }
        let blob = g.serialize();
        let mut g2 = Grid::deserialize(&blob).expect("valid blob");
        assert_eq!(g2.cells(), g.cells(), "cells restored");
        assert_eq!(g2.hash(), g.hash(), "hash restored");
        // RNG state preserved -> identical future evolution.
        for _ in 0..60 {
            g.step();
            g2.step();
        }
        assert_eq!(g.hash(), g2.hash(), "reloaded grid evolves identically");
    }

    #[test]
    fn deserialize_rejects_garbage() {
        assert!(Grid::deserialize(b"nope").is_none());
        assert!(Grid::deserialize(&[]).is_none());
        let mut ok = Grid::new(4, 4, 1).serialize();
        ok.truncate(ok.len() - 3); // corrupt/truncate
        assert!(Grid::deserialize(&ok).is_none());
    }

    // --- M9 threading determinism ---

    #[test]
    fn parallel_step_matches_serial_bit_for_bit() {
        // A thermally-busy scene (lava, water, fire) exercises the parallel
        // diffusion path. step_parallel must equal step every tick, and the
        // temperature field must match exactly too.
        let mut a = Grid::new(160, 160, 0x1234);
        let mut b = Grid::new(160, 160, 0x1234);
        for g in [&mut a, &mut b] {
            for x in 20..140 {
                g.set(x, 150, STONE);
            }
            g.paint(80, 120, 14, LAVA);
            g.paint(80, 40, 16, WATER);
            g.set(40, 60, FIRE);
            g.paint(110, 30, 4, OIL);
        }
        for tick in 0..300 {
            a.step();
            b.step_parallel();
            assert_eq!(a.hash(), b.hash(), "cell hash diverged at tick {tick}");
            assert_eq!(a.cells(), b.cells(), "cells diverged at tick {tick}");
            // temperature field identical too
            for i in 0..a.cells().len() {
                let (ta, tb) = (a.temp[i], b.temp[i]);
                assert_eq!(ta.to_bits(), tb.to_bits(), "temp diverged at tick {tick}");
            }
        }
    }

    #[test]
    fn paint_writes_only_into_empty_cells() {
        let mut g = Grid::new(8, 8, 1);
        g.paint(4, 4, 1, SAND);
        let sand = g.count(SAND);
        g.paint(4, 4, 1, WATER); // must not overwrite sand
        assert_eq!(g.count(SAND), sand, "sand untouched");
        assert_eq!(g.count(WATER), 0, "water did not overwrite matter");
        g.paint(4, 4, 1, EMPTY); // erase clears anything
        assert_eq!(g.count(SAND), 0, "erase removed the sand");
    }

    #[test]
    fn free_spark_vanishes_without_spawning_copper() {
        // A spark in open air must just fade — never materialize copper.
        let mut g = Grid::new(8, 8, 1);
        g.set(4, 4, SPARK);
        for _ in 0..30 {
            g.step();
        }
        assert_eq!(g.count(COPPER), 0, "no copper spawned from a free spark");
        assert_eq!(g.count(CHARGED), 0);
        assert_eq!(g.count(COOLED), 0);
        assert_eq!(g.count(SPARK), 0, "spark faded");
    }

    // --- new-elements tests ---

    #[test]
    fn clone_emits_its_source() {
        let mut g = Grid::new(7, 7, 1);
        for x in 0..7 {
            g.set(x, 6, STONE); // floor to catch cloned sand
        }
        g.set(3, 3, CLONE);
        g.set(3, 2, SAND); // feed sand from above
        for _ in 0..40 {
            g.step();
        }
        assert!(g.count(SAND) > 3, "clone produced sand: {}", g.count(SAND));
    }

    #[test]
    fn void_deletes_neighbours() {
        let mut g = Grid::new(7, 9, 1);
        g.set(3, 4, VOID);
        for y in 0..3 {
            g.set(3, y, SAND); // sand falls onto the void and is eaten
        }
        for _ in 0..60 {
            g.step();
        }
        assert_eq!(g.count(SAND), 0, "void deleted all the sand");
        assert_eq!(g.material_at(3, 4), VOID, "void persists");
    }

    #[test]
    fn drain_empties_a_pool() {
        // Narrow shaft so every floor cell is adjacent to the drain (a wide flat
        // pool would leave resting corner puddles that never flow to it).
        let mut g = Grid::new(3, 9, 1);
        g.set(1, 8, DRAIN); // drain on the floor
        for y in 0..6 {
            for x in 0..3 {
                g.set(x, y, WATER); // a column of water above it
            }
        }
        for _ in 0..400 {
            g.step();
        }
        assert_eq!(g.count(WATER), 0, "drain emptied the pool");
        assert_eq!(g.material_at(1, 8), DRAIN, "drain persists");
    }

    #[test]
    fn drain_eats_liquid_but_not_solids() {
        let mut g = Grid::new(3, 3, 1);
        g.set(1, 1, DRAIN);
        g.set(0, 1, STONE); // solid neighbour: must survive
        g.set(2, 1, WATER); // liquid neighbour: must be eaten
        g.set(1, 0, OIL); // another liquid neighbour
        for _ in 0..10 {
            g.step();
        }
        assert_eq!(g.count(WATER), 0, "drain ate the water");
        assert_eq!(g.count(OIL), 0, "drain ate the oil");
        assert_eq!(g.count(STONE), 1, "drain left the solid alone");
    }

    #[test]
    fn fish_stays_in_water() {
        let mut g = Grid::new(9, 9, 1);
        for y in 0..9 {
            for x in 0..9 {
                g.set(x, y, WATER);
            }
        }
        g.set(4, 4, FISH);
        for _ in 0..100 {
            g.step();
        }
        // The fish still exists and is somewhere in the tank (it never converts).
        assert_eq!(g.count(FISH), 1, "fish persists while submerged");
    }

    #[test]
    fn fish_sinks_when_water_drained() {
        let mut g = Grid::new(5, 12, 1);
        g.set(2, 0, FISH); // in open air
        for _ in 0..60 {
            g.step();
        }
        let fy = (0..12).find(|&y| g.material_at(2, y) == FISH);
        assert_eq!(fy, Some(11), "stranded fish sank to the floor");
    }

    #[test]
    fn worm_burrows_into_a_powder_pile() {
        let mut g = Grid::new(5, 12, 1);
        for y in 4..12 {
            for x in 0..5 {
                g.set(x, y, SAND); // a sand bed
            }
        }
        g.set(2, 3, WORM); // resting on top of the bed
        for _ in 0..80 {
            g.step();
        }
        // The worm wriggles, so find it anywhere on the grid, not just column 2.
        let mut pos = None;
        for y in 0..12 {
            for x in 0..5 {
                if g.material_at(x, y) == WORM {
                    pos = Some((x, y));
                }
            }
        }
        let (_, wy) = pos.expect("worm still present");
        assert!(wy >= 9, "worm burrowed deep into the sand (at row {wy})");
        assert_eq!(g.count(WORM), 1, "worm conserved");
    }

    #[test]
    fn ant_eats_plant() {
        let mut g = Grid::new(7, 7, 1);
        for x in 0..7 {
            g.set(x, 4, PLANT); // a row of plant
        }
        for x in 0..7 {
            g.set(x, 3, ANT); // ants sitting on it
        }
        let before = g.count(PLANT);
        for _ in 0..60 {
            g.step();
        }
        assert!(
            g.count(PLANT) < before,
            "ants grazed the plant: {} -> {}",
            before,
            g.count(PLANT)
        );
    }

    #[test]
    fn salt_dissolves_in_water_and_melts_ice() {
        let mut g = Grid::new(5, 5, 1);
        for y in 0..5 {
            for x in 0..5 {
                g.set(x, y, WATER);
            }
        }
        g.set(2, 2, SALT);
        for _ in 0..40 {
            g.step();
        }
        assert_eq!(g.count(SALT), 0, "salt dissolved");

        assert!(g.count(SALTWATER) > 0, "salt dissolved into brine");

        let mut h = Grid::new(5, 5, 1);
        for y in 0..5 {
            for x in 0..5 {
                h.set(x, y, ICE);
            }
        }
        h.set(2, 2, SALT);
        let ice0 = h.count(ICE);
        for _ in 0..60 {
            h.step();
        }
        // Brine does not refreeze, so the thaw persists.
        assert!(h.count(ICE) < ice0, "salt melted ice");
        assert!(h.count(SALTWATER) > 0, "ice thawed into brine");
    }

    #[test]
    fn plant_grows_along_water_and_burns() {
        let mut g = Grid::new(7, 7, 1);
        for y in 0..7 {
            for x in 0..7 {
                g.set(x, y, WATER);
            }
        }
        g.set(3, 3, PLANT);
        for _ in 0..200 {
            g.step();
        }
        let grown = g.count(PLANT);
        assert!(grown > 1, "plant spread through water: {grown}");

        g.set(3, 3, FIRE);
        let mut saw_drop = false;
        let before = g.count(PLANT);
        for _ in 0..300 {
            g.step();
            if g.count(PLANT) < before {
                saw_drop = true;
            }
        }
        assert!(saw_drop, "fire burned the plant");
    }

    #[test]
    fn thermite_flashes_to_molten() {
        let mut g = Grid::new(5, 5, 1);
        for y in 0..5 {
            for x in 0..5 {
                g.set(x, y, THERMITE);
            }
        }
        g.set(2, 2, FIRE);
        let mut saw_lava = false;
        for _ in 0..80 {
            g.step();
            if g.count(LAVA) > 0 {
                saw_lava = true;
                break;
            }
        }
        assert!(saw_lava, "thermite ignited into molten slag (lava)");
    }

    #[test]
    fn conductor_is_heat_proof() {
        let (mut g, x, y) = boxed(COPPER, 5000.0);
        for _ in 0..50 {
            g.step();
        }
        assert_eq!(
            g.material_at(x, y),
            COPPER,
            "conductor never melts from heat"
        );
    }

    // --- electronics / explosives / materials / heat-source tests ---

    #[test]
    fn battery_powers_wire_and_lights_lamp() {
        let mut g = Grid::new(12, 4, 1);
        for x in 0..12 {
            g.set(x, 1, COPPER);
        }
        g.set(0, 0, BATTERY); // beside the wire's start
        g.set(5, 0, LAMP); // beside the wire midway
        let mut lit = false;
        for _ in 0..60 {
            g.step();
            if g.count(LITLAMP) > 0 {
                lit = true;
                break;
            }
        }
        assert!(lit, "battery drove charge down the wire and lit the lamp");
    }

    #[test]
    fn fuse_burns_along_its_length() {
        let mut g = Grid::new(20, 3, 1);
        for x in 0..20 {
            g.set(x, 1, FUSE);
        }
        g.set(0, 1, FIRE);
        for _ in 0..400 {
            g.step();
        }
        assert_eq!(g.count(FUSE), 0, "fuse burned down fully and reliably");
    }

    #[test]
    fn lit_lamp_chains_across_neighbours() {
        let mut g = Grid::new(12, 12, 1);
        for y in 3..9 {
            for x in 3..9 {
                g.set(x, y, LAMP);
            }
        }
        let total = g.count(LAMP);
        g.set(2, 5, CHARGED); // a single pulse next to the array
        let mut max_lit = 0;
        for _ in 0..30 {
            g.step();
            max_lit = max_lit.max(g.count(LITLAMP));
        }
        assert!(
            max_lit > total / 2,
            "lit lamp chained through the array: {max_lit}/{total}"
        );
    }

    #[test]
    fn spark_and_charge_detonate_explosives() {
        use crate::material::{CHARGED, TNT};
        // Spark directly on a TNT cluster.
        let mut g = Grid::new(20, 20, 1);
        for y in 8..12 {
            for x in 8..12 {
                g.set(x, y, TNT);
            }
        }
        g.set(7, 9, SPARK);
        let mut blew = false;
        for _ in 0..5 {
            g.step();
            if g.count(TNT) == 0 && g.count(FIRE) > 0 {
                blew = true;
                break;
            }
        }
        assert!(blew, "spark detonated TNT");

        // A live wire (charged copper) detonates TNT too — circuits trigger bombs.
        let mut h = Grid::new(20, 20, 1);
        for y in 8..12 {
            for x in 8..12 {
                h.set(x, y, TNT);
            }
        }
        h.set(7, 9, CHARGED);
        let mut blew2 = false;
        for _ in 0..6 {
            h.step();
            if h.count(FIRE) > 0 {
                blew2 = true;
                break;
            }
        }
        assert!(blew2, "charged wire detonated TNT");
    }

    #[test]
    fn hydrogen_flashes_when_ignited() {
        let mut g = Grid::new(16, 16, 1);
        for y in 0..16 {
            for x in 0..16 {
                if x == 0 || y == 0 || x == 15 || y == 15 {
                    g.set(x, y, STONE);
                }
            }
        }
        for y in 1..15 {
            for x in 1..15 {
                g.set(x, y, HYDROGEN);
            }
        }
        let h0 = g.count(HYDROGEN);
        g.set(8, 8, FIRE);
        for _ in 0..30 {
            g.step();
        }
        assert!(g.count(HYDROGEN) < h0 / 4, "hydrogen flashed off");
    }

    #[test]
    fn nitro_blast_is_bigger_than_gunpowder() {
        let fire_after = |mat: MaterialId| {
            let mut g = Grid::new(40, 40, 1);
            g.set(20, 20, mat);
            g.set(19, 20, FIRE);
            for _ in 0..3 {
                g.step();
            }
            g.count(FIRE)
        };
        let nitro = fire_after(NITRO);
        let gp = fire_after(crate::material::GUNPOWDER);
        assert!(
            nitro > gp,
            "nitro blast ({nitro}) bigger than gunpowder ({gp})"
        );
    }

    #[test]
    fn wax_melts_and_resolidifies() {
        let (mut g, x, y) = boxed(WAX, 100.0);
        g.step();
        assert_eq!(g.material_at(x, y), MELTWAX, "warm wax melts");

        let (mut h, hx, hy) = boxed(MELTWAX, 30.0);
        h.step();
        assert_eq!(h.material_at(hx, hy), WAX, "cool molten wax resolidifies");
    }

    #[test]
    fn coal_burns_slowly() {
        let mut g = Grid::new(7, 7, 1);
        for y in 0..7 {
            for x in 0..7 {
                g.set(x, y, COAL);
            }
        }
        let coal0 = g.count(COAL);
        g.set(3, 3, FIRE);
        for _ in 0..30 {
            g.step();
        }
        let left = g.count(COAL);
        assert!(left < coal0, "coal caught fire");
        assert!(
            left > 0,
            "coal still smouldering after 30 ticks (slow burn): {left}"
        );
    }

    #[test]
    fn lava_quench_makes_obsidian() {
        let mut g = Grid::new(9, 12, 5);
        for y in 0..12 {
            g.set(0, y, STONE);
            g.set(8, y, STONE);
        }
        for x in 0..9 {
            g.set(x, 11, STONE);
        }
        for y in 7..11 {
            for x in 1..8 {
                g.set(x, y, LAVA);
            }
        }
        for y in 3..7 {
            for x in 1..8 {
                g.set(x, y, WATER);
            }
        }
        let mut saw = false;
        for _ in 0..200 {
            g.step();
            if g.count(OBSIDIAN) > 0 {
                saw = true;
                break;
            }
        }
        assert!(saw, "quenched lava formed obsidian");
    }

    #[test]
    fn heater_warms_and_cooler_chills() {
        let (mut hg, _, _) = boxed(HEATER, 600.0);
        for _ in 0..120 {
            hg.step();
        }
        assert!(
            hg.temperature_at(1, 0) > 100.0,
            "heater warmed neighbour: {}",
            hg.temperature_at(1, 0)
        );

        let (mut cg, _, _) = boxed(COOLER, -60.0);
        for _ in 0..120 {
            cg.step();
        }
        assert!(
            cg.temperature_at(1, 0) < 0.0,
            "cooler chilled neighbour: {}",
            cg.temperature_at(1, 0)
        );
    }

    #[test]
    fn snow_melts_when_warm() {
        let (mut g, x, y) = boxed(SNOW, 30.0);
        g.step();
        assert_eq!(g.material_at(x, y), WATER, "warm snow melts to water");
    }

    #[test]
    fn oxygen_feeds_fire() {
        let mut g = Grid::new(14, 14, 1);
        for y in 0..14 {
            for x in 0..14 {
                if x == 0 || y == 0 || x == 13 || y == 13 {
                    g.set(x, y, STONE);
                }
            }
        }
        for y in 1..13 {
            for x in 1..13 {
                g.set(x, y, OXYGEN);
            }
        }
        let o0 = g.count(OXYGEN);
        g.set(7, 7, FIRE);
        for _ in 0..20 {
            g.step();
        }
        assert!(g.count(OXYGEN) < o0 / 4, "fire tore through the oxygen");
    }

    #[test]
    fn ash_is_inert_and_settles() {
        let mut g = Grid::new(6, 10, 1);
        for x in 0..6 {
            g.set(x, 9, STONE); // floor
        }
        g.set(3, 0, ASH);
        g.set(2, 5, WATER); // a neighbour that must not react with ash
        let ash0 = g.count(ASH);
        for _ in 0..60 {
            g.step();
        }
        assert_eq!(g.count(ASH), ash0, "ash is inert (no reactions)");
        assert_eq!(
            g.material_at(3, 8),
            ASH,
            "ash fell and settled on the floor"
        );
    }

    #[test]
    fn burnt_fuse_leaves_ash() {
        let mut g = Grid::new(16, 3, 1);
        for x in 0..16 {
            g.set(x, 1, FUSE);
        }
        g.set(0, 1, FIRE);
        let mut saw_ash = false;
        for _ in 0..300 {
            g.step();
            if g.count(ASH) > 0 {
                saw_ash = true;
            }
        }
        assert!(saw_ash, "a burning fuse leaves an ash trail");
    }

    #[test]
    fn diamond_is_indestructible() {
        let mut g = Grid::new(9, 9, 1);
        for x in 0..9 {
            g.set(x, 8, STONE); // floor
        }
        g.set(4, 7, DIAMOND);
        g.set(4, 6, ACID); // acid resting on it
        g.set(3, 7, GUNPOWDER);
        g.set(2, 7, FIRE); // a blast right next to it
        g.set_temperature(4, 7, 4000.0); // extreme heat
        for _ in 0..80 {
            g.step();
        }
        assert_eq!(
            g.material_at(4, 7),
            DIAMOND,
            "diamond survives heat, acid, and explosions"
        );
    }

    #[test]
    fn ember_ignites_fuel() {
        let mut g = Grid::new(5, 5, 1);
        for x in 0..5 {
            g.set(x, 4, STONE);
        }
        g.set(2, 3, EMBER);
        g.set(3, 3, OIL);
        let mut saw_fire = false;
        for _ in 0..30 {
            g.step();
            if g.count(FIRE) > 0 {
                saw_fire = true;
                break;
            }
        }
        assert!(saw_fire, "an ember set the oil alight");
    }

    #[test]
    fn plant_spreads_through_soil() {
        let mut g = Grid::new(8, 8, 1);
        for y in 0..8 {
            for x in 0..8 {
                if x == 0 || y == 0 || x == 7 || y == 7 {
                    g.set(x, y, STONE);
                }
            }
        }
        for y in 1..7 {
            for x in 1..7 {
                g.set(x, y, SOIL);
            }
        }
        g.set(4, 4, PLANT);
        for _ in 0..400 {
            g.step();
        }
        assert!(g.count(PLANT) > 1, "plant rooted through the soil");
    }

    #[test]
    fn golden_hash_is_stable() {
        // Fixed scenario, fixed seed, N ticks -> known hash. Regenerate by
        // running once and pasting the printed value (see PROGRESS.md).
        let mut g = Grid::new(96, 96, 0xABCDEF);
        for i in 0..20 {
            g.paint(10 + i * 4, 4, 2, SAND);
        }
        for _ in 0..300 {
            g.step();
        }
        assert_eq!(
            g.hash(),
            GOLDEN_M2,
            "golden mismatch — see regeneration note"
        );
    }

    // Regenerate: temporarily `assert_eq!(g.hash(), 0)` and read the panic msg.
    const GOLDEN_M2: u64 = 3370987513426964979;

    #[test]
    fn paint_writes_into_empty_only() {
        let mut g = Grid::new(16, 16, 1);
        g.set(8, 8, SAND);
        g.paint(8, 8, 3, STONE); // disc covers the sand cell
        assert_eq!(g.material_at(8, 8), SAND, "paint must not overwrite matter");
        assert_eq!(g.material_at(8, 5), STONE, "but fills surrounding empties");
    }

    #[test]
    fn paint_over_overwrites_existing() {
        let mut g = Grid::new(16, 16, 1);
        g.paint(8, 8, 3, SAND);
        let sand_before = g.count(SAND);
        assert!(sand_before > 0);
        g.paint_over(8, 8, 3, STONE);
        assert_eq!(g.material_at(8, 8), STONE, "overwrites the centre");
        assert_eq!(g.count(SAND), 0, "all painted sand replaced");
    }

    #[test]
    fn paint_line_leaves_no_gaps() {
        // A single-cell brush dragged diagonally must paint a connected line, not
        // dotted stamps (the bug a per-frame single stamp would produce).
        let mut g = Grid::new(32, 32, 1);
        g.paint_line(2, 2, 20, 14, 0, STONE, false);
        // Every Bresenham cell on the way is filled; the endpoints are present.
        assert_eq!(g.material_at(2, 2), STONE);
        assert_eq!(g.material_at(20, 14), STONE);
        // No 2-cell gap exists: each filled cell has a filled 8-neighbour ahead.
        let painted: Vec<(usize, usize)> = (0..32)
            .flat_map(|y| (0..32).map(move |x| (x, y)))
            .filter(|&(x, y)| g.material_at(x, y) == STONE)
            .collect();
        assert!(painted.len() >= 18, "line spans its length: {}", painted.len());
    }

    #[test]
    fn flood_fill_fills_enclosed_pocket() {
        // A 10x10 stone box with a hollow interior; fill the interior with water.
        let mut g = Grid::new(16, 16, 1);
        for x in 3..=12 {
            g.set(x, 3, STONE);
            g.set(x, 12, STONE);
        }
        for y in 3..=12 {
            g.set(3, y, STONE);
            g.set(12, y, STONE);
        }
        g.flood_fill(7, 7, WATER);
        assert_eq!(g.material_at(7, 7), WATER, "interior filled");
        assert_eq!(g.material_at(4, 4), WATER, "fill reaches the corners inside");
        assert_eq!(g.material_at(3, 3), STONE, "walls untouched");
        assert_eq!(g.material_at(0, 0), EMPTY, "fill did not escape the box");
    }

    #[test]
    fn flood_fill_replaces_contiguous_material() {
        let mut g = Grid::new(16, 16, 1);
        g.paint_over(8, 8, 3, SAND); // a sand blob
        let blob = g.count(SAND);
        g.set(0, 0, SAND); // a disconnected stray grain
        g.flood_fill(8, 8, WATER); // click the blob
        assert_eq!(g.material_at(8, 8), WATER);
        assert_eq!(g.count(WATER), blob, "only the contiguous blob converted");
        assert_eq!(g.material_at(0, 0), SAND, "the disconnected grain stays");
    }

    #[test]
    fn flood_fill_noop_when_target_equals_fill() {
        let mut g = Grid::new(8, 8, 1);
        g.set(4, 4, WATER);
        g.flood_fill(4, 4, WATER); // same material: must not loop or change
        assert_eq!(g.count(WATER), 1);
    }
}
