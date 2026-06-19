//! Property-based invariants (proptest). Across random grids, random paint
//! sequences, random seeds, and random tick counts, the core must:
//!   * never panic / never write out of bounds (a panic fails the test),
//!   * never hold an out-of-range material id,
//!   * conserve mass while only movement is in play (no reactions/life yet) —
//!     non-empty cells are neither duplicated nor lost.
//!
//! As reactions (M6) and transients (M5+) arrive, the conservation invariant is
//! refined accordingly in later test additions; movement alone is strictly
//! conservative.

use pwdr_core::material::{self, MaterialId, EMPTY};
use pwdr_core::Grid;
use proptest::prelude::*;

fn nonempty(g: &Grid) -> usize {
    g.cells().iter().filter(|c| c.material != EMPTY).count()
}

fn all_ids_valid(g: &Grid) -> bool {
    let n = material::MATERIALS.len() as MaterialId;
    g.cells().iter().all(|c| c.material < n)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 200, ..ProptestConfig::default() })]

    #[test]
    fn movement_is_conservative_and_safe(
        w in 4usize..40,
        h in 4usize..40,
        seed: u64,
        // up to 12 paint ops: (x_frac, y_frac, radius, material)
        ops in proptest::collection::vec(
            (0u16..1000, 0u16..1000, 0usize..4, 0u8..4),
            0..12,
        ),
        ticks in 0u32..120,
    ) {
        let mut g = Grid::new(w, h, seed);
        for (xf, yf, r, m) in &ops {
            let x = (*xf as usize * w) / 1000;
            let y = (*yf as usize * h) / 1000;
            let x = x.min(w - 1);
            let y = y.min(h - 1);
            g.paint(x, y, *r, *m as MaterialId);
        }

        let before = nonempty(&g);
        prop_assert!(all_ids_valid(&g));

        for _ in 0..ticks {
            g.step();
        }

        prop_assert!(all_ids_valid(&g), "material id went out of range");
        prop_assert_eq!(nonempty(&g), before, "movement must conserve mass");
    }
}
