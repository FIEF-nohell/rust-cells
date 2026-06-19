//! `pwdr-core` — the falling-sand simulation core.
//!
//! Pure logic on a flat grid. **Zero graphics dependencies.** Everything here
//! is testable headlessly via `cargo test`. The frontend (`pwdr-app`) owns the
//! window, input, and GPU blit; it only ever reads the framebuffer this core
//! produces and pushes user edits in.

pub mod grid;
pub mod material;
pub mod rng;

pub use grid::{Cell, Grid};
pub use material::{MaterialId, MaterialProps, Phase};
pub use rng::Rng;
