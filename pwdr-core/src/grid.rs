//! The simulation grid: a flat `Vec<Cell>` indexed `y * width + x`, the per-tick
//! update, and chunked activity tracking so sleeping regions are skipped.

use crate::material::{self, MaterialId, Phase, EMPTY};
use crate::rng::Rng;

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
        let tint = self.rng.next_u32() as u8;
        self.cells[i] = Cell {
            material: mat,
            gen: 0,
            life: 0,
            tint,
        };
        self.touch(x, y);
    }

    /// Paint a filled disc of `mat`. `radius` 0 = 1 cell. Erase by painting `EMPTY`.
    pub fn paint(&mut self, cx: usize, cy: usize, radius: usize, mat: MaterialId) {
        let r = radius as isize;
        let (cx, cy) = (cx as isize, cy as isize);
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy > r * r {
                    continue;
                }
                let (x, y) = (cx + dx, cy + dy);
                if self.in_bounds(x, y) {
                    self.set(x as usize, y as usize, mat);
                }
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
        let on_left = x % CHUNK == 0 && cx > 0;
        let on_right = x % CHUNK == CHUNK - 1 && cx + 1 < self.chunks_x;
        let on_top = y % CHUNK == 0 && cy > 0;
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

    /// Advance one fixed tick.
    pub fn step(&mut self) {
        self.begin_tick();
        self.movement_pass();
        self.frame_parity = !self.frame_parity;
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
            // Other phases land in later milestones.
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

    /// If `mat` at (x,y) can displace whatever is at (tx,ty), swap them, tag
    /// both moved, and wake the affected chunks. Returns whether a move happened.
    #[inline]
    fn try_move(&mut self, x: usize, y: usize, tx: isize, ty: isize, mat: MaterialId) -> bool {
        if !self.in_bounds(tx, ty) {
            return false;
        }
        let (tx, ty) = (tx as usize, ty as usize);
        let target = self.material_at(tx, ty);
        if !displaces(mat, target) {
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
            for c in 0..3 {
                px[c] = (p.color[c] as i32 + off).clamp(0, 255) as u8;
            }
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

/// Generalized displacement rule: heavier movable cell swaps through a lighter
/// fluid/gas; anything moves into empty. Solids and same-or-denser block.
#[inline]
pub fn displaces(mover: MaterialId, target: MaterialId) -> bool {
    if target == EMPTY {
        return true;
    }
    let tp = material::props(target);
    match tp.phase {
        Phase::Liquid | Phase::Gas => material::props(mover).density > tp.density,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::{SAND, STONE};

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
        let left: usize = (0..w / 2).filter(|&x| g.material_at(x, bottom) == SAND).count();
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
        assert_eq!(g.hash(), GOLDEN_M2, "golden mismatch — see regeneration note");
    }

    // Regenerate: temporarily `assert_eq!(g.hash(), 0)` and read the panic msg.
    const GOLDEN_M2: u64 = 11145926252808830224;
}
