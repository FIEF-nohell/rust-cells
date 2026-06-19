//! `pwdr-core` — the falling-sand simulation core.
//!
//! Pure logic on a flat grid. **Zero graphics dependencies.** Everything here
//! is testable headlessly via `cargo test`. The frontend (`pwdr-app`) owns the
//! window, input, and GPU blit; it only ever reads the framebuffer this core
//! produces and pushes user edits in.

pub mod rng;

pub use rng::Rng;

/// A framebuffer-producing, simulating grid. Filled out across milestones.
/// At M0 it is an empty grid that blits to all-black RGBA.
#[derive(Clone)]
pub struct Grid {
    width: usize,
    height: usize,
    /// RGBA8 scratch buffer, `width * height * 4`, regenerated on demand.
    framebuffer: Vec<u8>,
    rng: Rng,
}

impl Grid {
    pub fn new(width: usize, height: usize, seed: u64) -> Self {
        assert!(width > 0 && height > 0, "grid must be non-empty");
        Grid {
            width,
            height,
            framebuffer: vec![0; width * height * 4],
            rng: Rng::new(seed),
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

    /// Advance the simulation one fixed tick. (No-op at M0.)
    pub fn step(&mut self) {
        let _ = &mut self.rng;
    }

    /// Render the current grid into an RGBA8 buffer and return it.
    /// At M0 the grid is empty, so every pixel is opaque black.
    pub fn render_rgba(&mut self) -> &[u8] {
        for px in self.framebuffer.chunks_exact_mut(4) {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
            px[3] = 255;
        }
        &self.framebuffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_dimensions() {
        let g = Grid::new(64, 48, 0);
        assert_eq!(g.width(), 64);
        assert_eq!(g.height(), 48);
    }

    #[test]
    fn framebuffer_is_rgba_sized_and_opaque_black() {
        let mut g = Grid::new(8, 8, 0);
        let fb = g.render_rgba();
        assert_eq!(fb.len(), 8 * 8 * 4);
        assert!(fb.chunks_exact(4).all(|p| p == [0, 0, 0, 255]));
    }

    #[test]
    fn step_is_deterministic_noop() {
        let mut a = Grid::new(16, 16, 123);
        let mut b = Grid::new(16, 16, 123);
        for _ in 0..10 {
            a.step();
            b.step();
        }
        assert_eq!(a.render_rgba(), b.render_rgba());
    }
}
