//! Data-driven material table. Adding an element = adding a row here, not
//! touching the hot loop. Ids are small (`u8`) and index directly into
//! [`MATERIALS`]. Id `0` is always [`EMPTY`].
//!
//! Fields beyond what a given milestone uses are present but inert; later
//! milestones (liquids, gases, heat, reactions) read them. This keeps `Cell`
//! and the loop stable as the roster grows.

/// A material id. Indexes [`MATERIALS`]. Kept to `u8` so a `Cell` stays tiny.
pub type MaterialId = u8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Empty,
    Powder,
    Liquid,
    Gas,
    Solid,
    Energy,
}

/// Static properties for one material. Pure data.
#[derive(Clone, Copy, Debug)]
pub struct MaterialProps {
    pub name: &'static str,
    pub phase: Phase,
    /// Heavier sinks through lighter (generalized density swap). Empty is 0.
    /// Gases are negative so they rise through anything with non-negative density.
    pub density: i16,
    /// Base RGB. Empty renders as black via the renderer, not this color.
    pub color: [u8; 3],
    /// Per-cell brightness jitter range (+/-), applied via the cell's tint byte.
    pub color_jitter: u8,
    /// Sideways spread per tick for fluids/gases. Powders/solids: 0. (M3+)
    pub dispersion: u8,
}

// ---- Material ids ---------------------------------------------------------
// Stable numeric ids. The roster grows across milestones; M1 ships Empty plus
// an inert heavy powder (Sand) and a static solid (Stone) to test movement.

pub const EMPTY: MaterialId = 0;
pub const STONE: MaterialId = 1;
pub const SAND: MaterialId = 2;

/// The material table, indexed by [`MaterialId`]. Order MUST match the ids above.
pub static MATERIALS: &[MaterialProps] = &[
    MaterialProps {
        name: "Empty",
        phase: Phase::Empty,
        density: 0,
        color: [0, 0, 0],
        color_jitter: 0,
        dispersion: 0,
    },
    MaterialProps {
        name: "Stone",
        phase: Phase::Solid,
        density: 9000,
        color: [120, 120, 128],
        color_jitter: 12,
        dispersion: 0,
    },
    MaterialProps {
        name: "Sand",
        phase: Phase::Powder,
        density: 1600,
        color: [194, 178, 110],
        color_jitter: 18,
        dispersion: 0,
    },
];

#[inline]
pub fn props(id: MaterialId) -> &'static MaterialProps {
    &MATERIALS[id as usize]
}

#[inline]
pub fn phase(id: MaterialId) -> Phase {
    props(id).phase
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_match_table_positions() {
        assert_eq!(props(EMPTY).name, "Empty");
        assert_eq!(props(STONE).name, "Stone");
        assert_eq!(props(SAND).name, "Sand");
    }

    #[test]
    fn empty_is_phase_empty() {
        assert_eq!(phase(EMPTY), Phase::Empty);
    }
}
