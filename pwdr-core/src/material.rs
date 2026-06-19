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
pub const COPPER: MaterialId = 10;
pub const SPARK: MaterialId = 11;
pub const CHARGED: MaterialId = 12;
pub const FIRE: MaterialId = 13;
pub const ACID: MaterialId = 14;
pub const FUME: MaterialId = 15;
pub const GUNPOWDER: MaterialId = 16;
pub const CRYO: MaterialId = 17;
pub const WOOD: MaterialId = 18;
pub const GLASS: MaterialId = 19;
pub const COOLED: MaterialId = 20;

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
        high_temp: 1100.0, // melts to glass (e.g. against lava)
        high_to: GLASS,
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
        density: 800, // lighter than water -> floats.
        color: [90, 70, 40],
        color_jitter: 12,
        dispersion: 4,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.08,
        high_temp: 350.0, // autoignites when hot enough
        high_to: FIRE,
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
    MaterialProps {
        name: "Copper",
        phase: Phase::Solid,
        density: 9000,
        color: [200, 120, 70],
        color_jitter: 10,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.22, // good thermal + electrical conductor
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Spark",
        phase: Phase::Energy,
        density: 0,
        color: [255, 245, 180],
        color_jitter: 0,
        dispersion: 0,
        // A free igniter: lives 2 ticks then vanishes (it does NOT leave copper).
        // It energizes adjacent copper into Charged and ignites fuel.
        life: 2,
        decay_to: EMPTY,
        default_temp: 60.0,
        conductivity: 0.10,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Charged",
        phase: Phase::Solid,
        // Energized copper: propagates the charge to neighbouring plain copper,
        // then cools to a refractory Cooled state (not Copper) so the wave can't
        // bounce back along its own trail.
        density: 9000,
        color: [255, 230, 150],
        color_jitter: 8,
        dispersion: 0,
        life: 2,
        decay_to: COOLED,
        default_temp: 20.0,
        conductivity: 0.22,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Fire",
        phase: Phase::Gas, // flickers upward and disperses
        density: -100,
        color: [255, 140, 30],
        color_jitter: 40,
        dispersion: 3,
        life: 60,
        decay_to: SMOKE, // byproduct
        default_temp: 700.0,
        conductivity: 0.10,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Acid",
        phase: Phase::Liquid,
        density: 1100,
        color: [120, 220, 60],
        color_jitter: 16,
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
        name: "Fume",
        phase: Phase::Gas, // flammable gas, rises and propagates fire
        density: -40,
        color: [150, 170, 90],
        color_jitter: 18,
        dispersion: 6,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.05,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Gunpowder",
        phase: Phase::Powder, // reactive, explosive powder
        density: 1700,
        color: [55, 55, 60],
        color_jitter: 14,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.06,
        high_temp: 300.0, // autoignites (then explodes via the blast hook)
        high_to: FIRE,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Cryo",
        phase: Phase::Solid, // persistent cold source
        density: 9000,
        color: [150, 200, 220],
        color_jitter: 10,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: -50.0,
        conductivity: 0.20,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Wood",
        phase: Phase::Solid, // flammable structural solid
        density: 9000,
        color: [120, 80, 40],
        color_jitter: 16,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.05,
        high_temp: 400.0, // chars/ignites when very hot
        high_to: FIRE,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Glass",
        phase: Phase::Solid, // inert, melt product of sand
        density: 9000,
        color: [180, 210, 215],
        color_jitter: 8,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.09,
        high_temp: 1450.0,
        high_to: LAVA, // remelts at extreme heat
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Cooled",
        phase: Phase::Solid, // refractory copper trail; reverts to copper
        density: 9000,
        color: [150, 95, 60],
        color_jitter: 8,
        dispersion: 0,
        life: 3,
        decay_to: COPPER,
        default_temp: 20.0,
        conductivity: 0.22,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
];

/// Internal materials the user shouldn't paint (transient conduction states).
#[inline]
pub fn user_paintable(id: MaterialId) -> bool {
    !matches!(id, EMPTY | CHARGED | COOLED)
}

/// Blast radius for explosive materials; 0 = not explosive. A function rather
/// than a table column so adding one explosive doesn't touch every row.
#[inline]
pub fn explosive_radius(id: MaterialId) -> u8 {
    match id {
        GUNPOWDER => 4,
        _ => 0,
    }
}

/// A data-driven contact reaction: when a cell of `a` is adjacent to a cell of
/// `b`, with probability `prob` (and only if the `a` cell's temperature is at
/// least `min_temp`), `a` becomes `a_to` and `b` becomes `b_to`. Reactions are
/// directional — the cell being processed is `a`.
#[derive(Clone, Copy, Debug)]
pub struct Reaction {
    pub a: MaterialId,
    pub b: MaterialId,
    pub a_to: MaterialId,
    pub b_to: MaterialId,
    pub prob: f32,
    pub min_temp: f32,
}

/// The reaction web. Emergent interactions, no per-pair code in the hot loop —
/// the engine just walks this table.
pub static REACTIONS: &[Reaction] = &[
    // Combustion: fire spreads into oil; oil becomes more fire.
    Reaction { a: FIRE, b: OIL, a_to: FIRE, b_to: FIRE, prob: 0.5, min_temp: NEVER_COLD },
    // Fire is quenched by water (and flashes the water to steam).
    Reaction { a: FIRE, b: WATER, a_to: SMOKE, b_to: STEAM, prob: 0.4, min_temp: NEVER_COLD },
    // Conduction: a spark energizes adjacent copper; charged copper propagates
    // the charge along the wire (the Charged->Cooled->Copper trail prevents the
    // wave from bouncing backward).
    Reaction { a: SPARK, b: COPPER, a_to: SPARK, b_to: CHARGED, prob: 1.0, min_temp: NEVER_COLD },
    Reaction { a: CHARGED, b: COPPER, a_to: CHARGED, b_to: CHARGED, prob: 1.0, min_temp: NEVER_COLD },
    // Sparks and live wires ignite adjacent fuel.
    Reaction { a: SPARK, b: OIL, a_to: SPARK, b_to: FIRE, prob: 1.0, min_temp: NEVER_COLD },
    Reaction { a: CHARGED, b: OIL, a_to: CHARGED, b_to: FIRE, prob: 1.0, min_temp: NEVER_COLD },
    // Corrosion: acid dissolves materials and is consumed in the process.
    Reaction { a: ACID, b: SAND, a_to: EMPTY, b_to: EMPTY, prob: 0.20, min_temp: NEVER_COLD },
    Reaction { a: ACID, b: STONE, a_to: EMPTY, b_to: EMPTY, prob: 0.10, min_temp: NEVER_COLD },
    Reaction { a: ACID, b: COPPER, a_to: EMPTY, b_to: EMPTY, prob: 0.12, min_temp: NEVER_COLD },
    Reaction { a: ACID, b: BASALT, a_to: EMPTY, b_to: EMPTY, prob: 0.08, min_temp: NEVER_COLD },
    Reaction { a: ACID, b: WOOD, a_to: EMPTY, b_to: EMPTY, prob: 0.15, min_temp: NEVER_COLD },
    // Flammable gas: fire and sparks propagate through fume.
    Reaction { a: FIRE, b: FUME, a_to: FIRE, b_to: FIRE, prob: 0.7, min_temp: NEVER_COLD },
    Reaction { a: SPARK, b: FUME, a_to: SPARK, b_to: FIRE, prob: 1.0, min_temp: NEVER_COLD },
    Reaction { a: CHARGED, b: FUME, a_to: CHARGED, b_to: FIRE, prob: 1.0, min_temp: NEVER_COLD },
    // Flammable solid: fire creeps along wood.
    Reaction { a: FIRE, b: WOOD, a_to: FIRE, b_to: FIRE, prob: 0.05, min_temp: NEVER_COLD },
    // Cold source: cryo freezes adjacent water regardless of its own temperature.
    Reaction { a: CRYO, b: WATER, a_to: CRYO, b_to: ICE, prob: 0.30, min_temp: NEVER_COLD },
];

/// First reaction matching `(a, b)`, if any. Linear scan — the table is small.
#[inline]
pub fn reaction_for(a: MaterialId, b: MaterialId) -> Option<&'static Reaction> {
    REACTIONS.iter().find(|r| r.a == a && r.b == b)
}

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
