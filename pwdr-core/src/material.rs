//! Data-driven material table. Adding an element = adding a row here, not
//! touching the hot loop. Ids are small (`u8`) and index directly into
//! [`MATERIALS`]. Id `0` is always [`EMPTY`].

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
    /// Sideways spread per tick for fluids/gases. Powders/solids: 0.
    pub dispersion: u8,
    /// Initial life for transient cells; 0 = permanent. Decrements each tick and
    /// the cell becomes [`MaterialProps::decay_to`] at 0.
    pub life: u8,
    /// What a transient becomes when its life expires.
    pub decay_to: MaterialId,

    // --- thermal (M5) ---
    /// Temperature a freshly placed cell starts at (°C-like units).
    pub default_temp: f32,
    /// Diffusion rate toward the neighbour average per tick, 0..=0.25 (stable).
    pub conductivity: f32,
    /// At/above this temperature the cell becomes [`MaterialProps::high_to`].
    pub high_temp: f32,
    pub high_to: MaterialId,
    /// At/below this temperature the cell becomes [`MaterialProps::low_to`].
    pub low_temp: f32,
    pub low_to: MaterialId,
}

// ---- Material ids ---------------------------------------------------------
pub const EMPTY: MaterialId = 0;
pub const STONE: MaterialId = 1;
pub const SAND: MaterialId = 2;
pub const WATER: MaterialId = 3;
pub const OIL: MaterialId = 4;
pub const SMOKE: MaterialId = 5;
pub const ICE: MaterialId = 6;
pub const STEAM: MaterialId = 7;
pub const LAVA: MaterialId = 8;
pub const BASALT: MaterialId = 9;

const NEVER_HOT: f32 = f32::INFINITY;
const NEVER_COLD: f32 = f32::NEG_INFINITY;

/// The material table, indexed by [`MaterialId`]. Order MUST match the ids above.
pub static MATERIALS: &[MaterialProps] = &[
    MaterialProps {
        name: "Empty",
        phase: Phase::Empty,
        density: 0,
        color: [0, 0, 0],
        color_jitter: 0,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.03, // air: slow
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Stone",
        phase: Phase::Solid,
        density: 9000,
        color: [120, 120, 128],
        color_jitter: 12,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.10,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Sand",
        phase: Phase::Powder,
        density: 1600,
        color: [194, 178, 110],
        color_jitter: 18,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.06,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Water",
        phase: Phase::Liquid,
        density: 1000,
        color: [40, 90, 200],
        color_jitter: 14,
        dispersion: 5,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.12,
        high_temp: 100.0,
        high_to: STEAM, // boil
        low_temp: 0.0,
        low_to: ICE, // freeze
    },
    MaterialProps {
        name: "Oil",
        phase: Phase::Liquid,
        density: 800, // lighter than water -> floats. Flammable (M6).
        color: [90, 70, 40],
        color_jitter: 12,
        dispersion: 4,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.08,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Smoke",
        phase: Phase::Gas,
        density: -50,
        color: [60, 60, 64],
        color_jitter: 16,
        dispersion: 6,
        life: 180,
        decay_to: EMPTY,
        default_temp: 60.0,
        conductivity: 0.05,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Ice",
        phase: Phase::Solid,
        density: 900,
        color: [170, 210, 240],
        color_jitter: 10,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: -10.0,
        conductivity: 0.18,
        high_temp: 0.0,
        high_to: WATER, // melt
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Steam",
        phase: Phase::Gas,
        density: -60,
        color: [200, 200, 210],
        color_jitter: 14,
        dispersion: 6,
        life: 0,
        decay_to: EMPTY,
        default_temp: 110.0,
        conductivity: 0.05,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: 99.0,
        low_to: WATER, // condense
    },
    MaterialProps {
        name: "Lava",
        phase: Phase::Liquid,
        density: 2500, // heavier than water -> sinks
        color: [220, 90, 30],
        color_jitter: 22,
        dispersion: 3,
        life: 0,
        decay_to: EMPTY,
        default_temp: 1200.0,
        conductivity: 0.14,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: 500.0,
        low_to: BASALT, // cools / solidifies
    },
    MaterialProps {
        name: "Basalt",
        phase: Phase::Solid,
        density: 9000,
        color: [70, 64, 70],
        color_jitter: 12,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 300.0,
        conductivity: 0.10,
        high_temp: 1000.0,
        high_to: LAVA, // remelt
        low_temp: NEVER_COLD,
        low_to: EMPTY,
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
        assert_eq!(props(WATER).name, "Water");
        assert_eq!(props(LAVA).name, "Lava");
        assert_eq!(props(BASALT).name, "Basalt");
    }

    #[test]
    fn empty_is_phase_empty() {
        assert_eq!(phase(EMPTY), Phase::Empty);
    }

    #[test]
    fn conductivities_are_stable() {
        // Explicit diffusion with 4 neighbours is stable for rate <= 0.25.
        assert!(MATERIALS.iter().all(|m| m.conductivity <= 0.25));
    }
}
