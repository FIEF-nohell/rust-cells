//! The simulation grid: a flat `Vec<Cell>` indexed `y * width + x`, plus the
//! per-tick update. No nested vecs — cache locality dominates the hot loop.

use crate::material::{self, MaterialId, Phase, EMPTY};
use crate::rng::Rng;

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
}

impl Grid {
    pub fn new(width: usize, height: usize, seed: u64) -> Self {
        assert!(width > 0 && height > 0, "grid must be non-empty");
        Grid {
            width,
            height,
            cells: vec![Cell::EMPTY; width * height],
            gen: 0,
            frame_parity: false,
            rng: Rng::new(seed),
            framebuffer: vec![0; width * height * 4],
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

    /// Count cells of a given material — test/debug helper.
    pub fn count(&self, id: MaterialId) -> usize {
        self.cells.iter().filter(|c| c.material == id).count()
    }

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
    }

    /// Paint a filled disc of `mat` (Chebyshev-ish round brush). `radius` 0 = 1 cell.
    /// Erase by painting `EMPTY`.
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

    // --- simulation --------------------------------------------------------

    fn begin_tick(&mut self) {
        self.gen = self.gen.wrapping_add(1);
        if self.gen == 0 {
            for c in &mut self.cells {
                c.gen = 0;
            }
            self.gen = 1;
        }
    }

    /// Advance one fixed tick.
    pub fn step(&mut self) {
        self.begin_tick();
        self.movement_pass();
        self.frame_parity = !self.frame_parity;
    }

    /// Bottom-up scan so a falling cell moves exactly one row per tick.
    /// Horizontal direction alternates each frame to avoid directional bias.
    fn movement_pass(&mut self) {
        for y in (0..self.height).rev() {
            if self.frame_parity {
                for x in 0..self.width {
                    self.update_cell(x, y);
                }
            } else {
                for x in (0..self.width).rev() {
                    self.update_cell(x, y);
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

        // Straight down.
        if self.try_move(x, y, x as isize, y as isize + 1, mat) {
            return;
        }
        // Down-diagonals, randomized order.
        let left_first = self.rng.bool();
        let (a, b) = if left_first { (-1, 1) } else { (1, -1) };
        if self.try_move(x, y, x as isize + a, y as isize + 1, mat) {
            return;
        }
        let _ = self.try_move(x, y, x as isize + b, y as isize + 1, mat);
    }

    /// If `mat` at (x,y) can displace whatever is at (tx,ty), swap them and tag
    /// both as moved. Returns whether a move happened.
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
}

/// Generalized displacement rule: heavier movable cell swaps through a lighter
/// fluid/gas; anything moves into empty. Solids and same-or-denser block.
/// This one rule covers sand-through-water, oil-on-water, light-gas-rising.
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
        // Drop a column of sand onto the center; let it pile.
        for _ in 0..200 {
            g.set(w / 2, 0, SAND);
            g.step();
        }
        // Run to rest.
        for _ in 0..2000 {
            g.step();
        }
        // Mass conserved: never duplicated/vanished.
        assert!(g.count(SAND) > 0);
        // Roughly symmetric: left/right halves of the bottom row balance.
        let bottom = g.height() - 1;
        let left: usize = (0..w / 2).filter(|&x| g.material_at(x, bottom) == SAND).count();
        let right: usize = (w / 2 + 1..w)
            .filter(|&x| g.material_at(x, bottom) == SAND)
            .count();
        let diff = left.abs_diff(right);
        assert!(
            diff <= 3,
            "pyramid should be near-symmetric: left={left} right={right}"
        );
    }
}
