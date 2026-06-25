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
    /// Self-propelled critters (fish/worm/ant). Move under their own rules in the
    /// movement pass rather than by gravity/buoyancy.
    Life,
}

/// Palette/grouping category. Orthogonal to [`Phase`] (which drives physics):
/// this is the human-facing bucket an element shows under. One function, so the
/// big material table stays unchanged when a row is added.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    Earth,
    Liquid,
    Gas,
    Fire,
    Electronic,
    Explosive,
    Life,
    Tool,
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
pub const PLASMA: MaterialId = 21;
pub const FROST: MaterialId = 22;
pub const CLONE: MaterialId = 23;
pub const VOID: MaterialId = 24;
pub const SALT: MaterialId = 25;
pub const PLANT: MaterialId = 26;
pub const THERMITE: MaterialId = 27;
pub const SALTWATER: MaterialId = 28;
pub const BATTERY: MaterialId = 29;
pub const LAMP: MaterialId = 30;
pub const LITLAMP: MaterialId = 31;
pub const FUSE: MaterialId = 32;
pub const HYDROGEN: MaterialId = 33;
pub const NITRO: MaterialId = 34;
pub const TNT: MaterialId = 35;
pub const WAX: MaterialId = 36;
pub const MELTWAX: MaterialId = 37;
pub const COAL: MaterialId = 38;
pub const OBSIDIAN: MaterialId = 39;
pub const HEATER: MaterialId = 40;
pub const COOLER: MaterialId = 41;
pub const BURNFUSE: MaterialId = 42;
pub const SNOW: MaterialId = 43;
pub const ASH: MaterialId = 44;
pub const OXYGEN: MaterialId = 45;
pub const SOIL: MaterialId = 46;
pub const EMBER: MaterialId = 47;
pub const DIAMOND: MaterialId = 48;
pub const FISH: MaterialId = 49;
pub const WORM: MaterialId = 50;
pub const ANT: MaterialId = 51;
pub const DRAIN: MaterialId = 52;

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
        conductivity: 0.006, // air: near-insulating, so conductors keep + carry heat far
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
        high_temp: 1100.0, // melts to lava (e.g. thermite/lava burning through it)
        high_to: LAVA,
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
        dispersion: 20, // sees distant descents -> a wide pool levels flat
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
        dispersion: 14, // a touch more viscous than water
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
        // Holds heat (low conductivity) so it rises a long way; condenses when it
        // finally cools, with a long life as a fallback so it never accumulates.
        life: 220,
        decay_to: WATER,
        default_temp: 120.0,
        conductivity: 0.02,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: 45.0,
        low_to: WATER, // condense
    },
    MaterialProps {
        name: "Lava",
        phase: Phase::Liquid,
        density: 2500, // heavier than water -> sinks
        color: [220, 90, 30],
        color_jitter: 22,
        dispersion: 6, // viscous: levels slowly, keeps a moundy flow
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
        name: "Conductor",
        phase: Phase::Solid,
        density: 9000,
        color: [200, 120, 70],
        color_jitter: 10,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.50,
        high_temp: NEVER_HOT, // heat-proof: never melts/destroyed by temperature
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
        conductivity: 0.50,
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
        dispersion: 14,
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
        conductivity: 0.50,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Plasma",
        phase: Phase::Gas, // a hot "flame": rises, flickers, sheds heat, then vanishes
        density: -120,
        color: [235, 120, 255],
        color_jitter: 30,
        dispersion: 3,
        life: 25,
        decay_to: EMPTY, // leaves no trace
        default_temp: 4000.0,
        conductivity: 0.50, // sheds its heat fast
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Frost",
        phase: Phase::Gas, // a cold "flame": rises, flickers, drains heat, then vanishes
        density: -110,
        color: [190, 240, 255],
        color_jitter: 20,
        dispersion: 3,
        life: 25,
        decay_to: EMPTY, // leaves no trace
        default_temp: -1000.0,
        conductivity: 0.50,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Clone",
        phase: Phase::Solid, // emits copies of an adjacent material into empty space
        density: 9000,
        color: [90, 160, 120],
        color_jitter: 6,
        dispersion: 0,
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
        name: "Void",
        phase: Phase::Solid, // deletes whatever touches it
        density: 9000,
        color: [40, 20, 50],
        color_jitter: 6,
        dispersion: 0,
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
        name: "Salt",
        phase: Phase::Powder, // dissolves in water, melts ice
        density: 1500,
        color: [230, 230, 235],
        color_jitter: 12,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.07,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Plant",
        phase: Phase::Solid, // grows along water; flammable
        density: 9000,
        color: [60, 160, 60],
        color_jitter: 22,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.05,
        high_temp: 250.0, // dries out and ignites when hot
        high_to: FIRE,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Thermite",
        phase: Phase::Powder, // burns into molten slag (lava) hot enough to melt metal
        density: 1800,
        color: [140, 70, 50],
        color_jitter: 16,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.08,
        high_temp: 300.0,
        high_to: LAVA, // ignites to molten
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Saltwater",
        phase: Phase::Liquid, // brine: denser than water, does NOT freeze
        density: 1025,
        color: [60, 120, 150],
        color_jitter: 14,
        dispersion: 20,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.12,
        high_temp: 100.0,
        high_to: STEAM,       // boils (salt left behind is abstracted away)
        low_temp: NEVER_COLD, // freezing-point depression: stays liquid
        low_to: EMPTY,
    },
    // --- Electronics ---
    MaterialProps {
        name: "Battery",
        phase: Phase::Solid, // pulses charge into adjacent copper
        density: 9000,
        color: [70, 200, 90],
        color_jitter: 8,
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
        name: "Lamp",
        phase: Phase::Solid, // lights when charge passes nearby
        density: 9000,
        color: [90, 85, 45],
        color_jitter: 6,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.20,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "LitLamp",
        phase: Phase::Solid, // glowing lamp; fades back to Lamp
        density: 9000,
        color: [255, 240, 130],
        color_jitter: 4,
        dispersion: 0,
        life: 8,
        decay_to: LAMP,
        default_temp: 20.0,
        conductivity: 0.20,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Fuse",
        phase: Phase::Solid, // slow-burning cord
        density: 9000,
        color: [120, 95, 60],
        color_jitter: 12,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.05,
        high_temp: 200.0,
        high_to: BURNFUSE, // heat lights it into a steady travelling burn
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    // --- Explosives ---
    MaterialProps {
        name: "Hydrogen",
        phase: Phase::Gas, // very light, flash-explodes when ignited
        density: -90,
        color: [180, 220, 235],
        color_jitter: 14,
        dispersion: 6,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.04,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Nitro",
        phase: Phase::Liquid, // liquid high explosive, big blast
        density: 1200,
        color: [205, 50, 70],
        color_jitter: 12,
        dispersion: 12,
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
        name: "TNT",
        phase: Phase::Solid, // placeable explosive block
        density: 9000,
        color: [160, 50, 45],
        color_jitter: 10,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.06,
        high_temp: 280.0,
        high_to: FIRE, // autoignites (then detonates via blast hook)
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    // --- Materials & states ---
    MaterialProps {
        name: "Wax",
        phase: Phase::Solid, // melts when warm
        density: 9000,
        color: [235, 225, 190],
        color_jitter: 10,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.06,
        high_temp: 70.0,
        high_to: MELTWAX,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Molten Wax",
        phase: Phase::Liquid, // resolidifies when it cools
        density: 950,
        color: [240, 220, 165],
        color_jitter: 10,
        dispersion: 6,
        life: 0,
        decay_to: EMPTY,
        default_temp: 80.0,
        conductivity: 0.06,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: 60.0,
        low_to: WAX,
    },
    MaterialProps {
        name: "Coal",
        phase: Phase::Solid, // long-burning fuel
        density: 9000,
        color: [42, 42, 48],
        color_jitter: 10,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.05,
        high_temp: 450.0,
        high_to: FIRE,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Obsidian",
        phase: Phase::Solid, // hard glassy rock from quenched lava
        density: 9000,
        color: [28, 24, 40],
        color_jitter: 8,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.08,
        high_temp: 1200.0,
        high_to: LAVA, // remelts
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    // --- Persistent heat sources ---
    MaterialProps {
        name: "Heater",
        phase: Phase::Solid, // holds a fixed hot temperature
        density: 9000,
        color: [200, 70, 40],
        color_jitter: 8,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 600.0,
        conductivity: 0.20,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Cooler",
        phase: Phase::Solid, // holds a fixed cold temperature
        density: 9000,
        color: [60, 120, 205],
        color_jitter: 8,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: -60.0,
        conductivity: 0.20,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Burning Fuse",
        phase: Phase::Energy, // travelling burn that stays on the cord
        density: 0,
        color: [255, 180, 50],
        color_jitter: 30,
        dispersion: 0,
        life: 8,
        decay_to: ASH, // leaves a burnt-ash trail
        default_temp: 500.0,
        conductivity: 0.10,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Snow",
        phase: Phase::Powder, // light, cold; melts to water when it warms up
        density: 400,
        color: [236, 240, 250],
        color_jitter: 8,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: -5.0,
        conductivity: 0.10,
        high_temp: 2.0,
        high_to: WATER, // melts
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Ash",
        phase: Phase::Powder, // inert, light byproduct of burning
        density: 700,
        color: [92, 90, 96],
        color_jitter: 16,
        dispersion: 0,
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
        name: "Oxygen",
        phase: Phase::Gas, // combustion accelerant; makes fire roar
        density: -30,
        color: [150, 200, 255],
        color_jitter: 10,
        dispersion: 6,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.04,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Soil",
        phase: Phase::Powder, // plant grows through it
        density: 1500,
        color: [110, 76, 46],
        color_jitter: 16,
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
        name: "Ember",
        phase: Phase::Powder, // glowing hot coal; ignites things, fades to ash
        density: 900,
        color: [232, 96, 32],
        color_jitter: 30,
        dispersion: 0,
        life: 120,
        decay_to: ASH,
        default_temp: 600.0,
        conductivity: 0.10,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
    MaterialProps {
        name: "Diamond",
        phase: Phase::Solid, // indestructible: fireproof, acid-proof, blast-proof
        density: 9000,
        color: [180, 235, 242],
        color_jitter: 6,
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
    // --- Critters (Phase::Life: self-propelled agents) ---
    MaterialProps {
        name: "Fish",
        phase: Phase::Life, // swims through water; sinks helplessly in air
        density: 1100,
        color: [240, 140, 60],
        color_jitter: 26,
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
        name: "Worm",
        phase: Phase::Life, // burrows down through powders, falls in air
        density: 1300,
        color: [205, 120, 130],
        color_jitter: 22,
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
        name: "Ant",
        phase: Phase::Life, // walks on surfaces, falls in air, eats plant
        density: 600,
        color: [60, 42, 34],
        color_jitter: 18,
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
        name: "Drain",
        phase: Phase::Solid, // a sink that swallows only liquids (keeps solids)
        density: 9000,
        color: [55, 75, 95],
        color_jitter: 8,
        dispersion: 0,
        life: 0,
        decay_to: EMPTY,
        default_temp: 20.0,
        conductivity: 0.08,
        high_temp: NEVER_HOT,
        high_to: EMPTY,
        low_temp: NEVER_COLD,
        low_to: EMPTY,
    },
];

/// Fixed temperature a persistent source holds, if any (Heater/Cooler).
#[inline]
pub fn pinned_temp(id: MaterialId) -> Option<f32> {
    match id {
        HEATER => Some(600.0),
        COOLER => Some(-60.0),
        _ => None,
    }
}

/// Internal materials the user shouldn't paint (transient conduction states).
#[inline]
pub fn user_paintable(id: MaterialId) -> bool {
    !matches!(id, EMPTY | CHARGED | COOLED | LITLAMP | BURNFUSE)
}

/// Human-facing palette bucket for an element. Cross-cuts [`Phase`]: e.g. Lava
/// is a liquid but lives under Fire & Heat, Gunpowder is a powder but under
/// Explosives. Kept as a function so the material table needs no extra column.
#[inline]
pub fn category(id: MaterialId) -> Category {
    match id {
        WATER | SALTWATER | OIL | ACID | MELTWAX => Category::Liquid,
        SMOKE | STEAM | FUME | HYDROGEN | OXYGEN => Category::Gas,
        FIRE | LAVA | PLASMA | FROST | EMBER | COAL | HEATER | COOLER | CRYO | BURNFUSE => {
            Category::Fire
        }
        COPPER | SPARK | CHARGED | BATTERY | LAMP | LITLAMP | FUSE | COOLED => Category::Electronic,
        GUNPOWDER | TNT | NITRO | THERMITE => Category::Explosive,
        PLANT | FISH | WORM | ANT => Category::Life,
        CLONE | VOID | DRAIN => Category::Tool,
        // Everything else (stone, sand, salt, soil, snow, ash, ice, glass,
        // basalt, obsidian, diamond, wax, …) is plain earth/material.
        _ => Category::Earth,
    }
}

/// Catches fire and spreads/sustains flame (but is not itself a detonating
/// explosive — those are flagged by [`explosive_radius`]). Drives the palette's
/// hazard marker. Kept as a small list rather than a table column.
#[inline]
pub fn is_flammable(id: MaterialId) -> bool {
    matches!(
        id,
        OIL | WOOD | FUME | PLANT | COAL | FUSE | THERMITE | OXYGEN | EMBER
    )
}

/// Detonates with a blast (radius > 0). Convenience over [`explosive_radius`].
#[inline]
pub fn is_explosive(id: MaterialId) -> bool {
    explosive_radius(id) > 0
}

/// Blast radius for explosive materials; 0 = not explosive. A function rather
/// than a table column so adding one explosive doesn't touch every row.
#[inline]
pub fn explosive_radius(id: MaterialId) -> u8 {
    match id {
        HYDROGEN => 2,
        GUNPOWDER => 4,
        TNT => 6,
        NITRO => 7,
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
    Reaction {
        a: FIRE,
        b: OIL,
        a_to: FIRE,
        b_to: FIRE,
        prob: 0.5,
        min_temp: NEVER_COLD,
    },
    // Fire is quenched by water (and flashes the water to steam).
    Reaction {
        a: FIRE,
        b: WATER,
        a_to: SMOKE,
        b_to: STEAM,
        prob: 0.4,
        min_temp: NEVER_COLD,
    },
    // Conduction: a spark energizes adjacent copper; charged copper propagates
    // the charge along the wire (the Charged->Cooled->Copper trail prevents the
    // wave from bouncing backward).
    Reaction {
        a: SPARK,
        b: COPPER,
        a_to: SPARK,
        b_to: CHARGED,
        prob: 1.0,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: CHARGED,
        b: COPPER,
        a_to: CHARGED,
        b_to: CHARGED,
        prob: 1.0,
        min_temp: NEVER_COLD,
    },
    // Sparks and live wires ignite adjacent fuel.
    Reaction {
        a: SPARK,
        b: OIL,
        a_to: SPARK,
        b_to: FIRE,
        prob: 1.0,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: CHARGED,
        b: OIL,
        a_to: CHARGED,
        b_to: FIRE,
        prob: 1.0,
        min_temp: NEVER_COLD,
    },
    // Corrosion: acid dissolves materials and is consumed in the process.
    Reaction {
        a: ACID,
        b: WATER,
        a_to: EMPTY,
        b_to: EMPTY,
        prob: 0.15,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: ACID,
        b: SAND,
        a_to: EMPTY,
        b_to: EMPTY,
        prob: 0.20,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: ACID,
        b: STONE,
        a_to: EMPTY,
        b_to: EMPTY,
        prob: 0.10,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: ACID,
        b: COPPER,
        a_to: EMPTY,
        b_to: EMPTY,
        prob: 0.12,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: ACID,
        b: BASALT,
        a_to: EMPTY,
        b_to: EMPTY,
        prob: 0.08,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: ACID,
        b: WOOD,
        a_to: EMPTY,
        b_to: EMPTY,
        prob: 0.15,
        min_temp: NEVER_COLD,
    },
    // Flammable gas: fire and sparks propagate through fume.
    Reaction {
        a: FIRE,
        b: FUME,
        a_to: FIRE,
        b_to: FIRE,
        prob: 0.7,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: SPARK,
        b: FUME,
        a_to: SPARK,
        b_to: FIRE,
        prob: 1.0,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: CHARGED,
        b: FUME,
        a_to: CHARGED,
        b_to: FIRE,
        prob: 1.0,
        min_temp: NEVER_COLD,
    },
    // Flammable solid: fire creeps along wood (every direction, not just up).
    Reaction {
        a: FIRE,
        b: WOOD,
        a_to: FIRE,
        b_to: FIRE,
        prob: 0.20,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: SPARK,
        b: WOOD,
        a_to: SPARK,
        b_to: FIRE,
        prob: 0.5,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: CHARGED,
        b: WOOD,
        a_to: CHARGED,
        b_to: FIRE,
        prob: 0.5,
        min_temp: NEVER_COLD,
    },
    // Molten rock slowly melts through solid stone it touches — so lava (and
    // thermite, which flashes to lava) can burn a hole through a stone slab.
    // Low probability so it eats through gradually rather than all at once.
    Reaction {
        a: LAVA,
        b: STONE,
        a_to: LAVA,
        b_to: LAVA,
        prob: 0.04,
        min_temp: NEVER_COLD,
    },
    // Lava ignites everything flammable it touches (it's 1200 degrees).
    Reaction {
        a: LAVA,
        b: OIL,
        a_to: LAVA,
        b_to: FIRE,
        prob: 0.6,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: LAVA,
        b: WOOD,
        a_to: LAVA,
        b_to: FIRE,
        prob: 0.25,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: LAVA,
        b: FUME,
        a_to: LAVA,
        b_to: FIRE,
        prob: 0.6,
        min_temp: NEVER_COLD,
    },
    // Cold source: cryo freezes adjacent water regardless of its own temperature.
    Reaction {
        a: CRYO,
        b: WATER,
        a_to: CRYO,
        b_to: ICE,
        prob: 0.30,
        min_temp: NEVER_COLD,
    },
    // Salt dissolves water into brine (consumed) and melts ice into brine, which
    // does not refreeze — so salt genuinely thaws ice.
    Reaction {
        a: SALT,
        b: WATER,
        a_to: EMPTY,
        b_to: SALTWATER,
        prob: 0.25,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: SALT,
        b: ICE,
        a_to: SALTWATER,
        b_to: SALTWATER,
        prob: 0.50,
        min_temp: NEVER_COLD,
    },
    // Plant creeps along water and burns readily.
    Reaction {
        a: PLANT,
        b: WATER,
        a_to: PLANT,
        b_to: PLANT,
        prob: 0.06,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: FIRE,
        b: PLANT,
        a_to: FIRE,
        b_to: FIRE,
        prob: 0.35,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: LAVA,
        b: PLANT,
        a_to: LAVA,
        b_to: FIRE,
        prob: 0.7,
        min_temp: NEVER_COLD,
    },
    // Thermite flashes to molten slag on contact with fire/spark/lava.
    Reaction {
        a: FIRE,
        b: THERMITE,
        a_to: FIRE,
        b_to: LAVA,
        prob: 0.6,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: SPARK,
        b: THERMITE,
        a_to: CHARGED,
        b_to: LAVA,
        prob: 1.0,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: LAVA,
        b: THERMITE,
        a_to: LAVA,
        b_to: LAVA,
        prob: 0.5,
        min_temp: NEVER_COLD,
    },
    // Electronics: a live wire (charge/spark) lights an adjacent lamp, and a lit
    // lamp lights its neighbours — so a lamp array all glows together.
    Reaction {
        a: CHARGED,
        b: LAMP,
        a_to: CHARGED,
        b_to: LITLAMP,
        prob: 1.0,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: SPARK,
        b: LAMP,
        a_to: SPARK,
        b_to: LITLAMP,
        prob: 1.0,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: LITLAMP,
        b: LAMP,
        a_to: LITLAMP,
        b_to: LITLAMP,
        prob: 1.0,
        min_temp: NEVER_COLD,
    },
    // Fuse: a steady self-propagating burn travels the cord reliably.
    Reaction {
        a: FIRE,
        b: FUSE,
        a_to: FIRE,
        b_to: BURNFUSE,
        prob: 1.0,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: BURNFUSE,
        b: FUSE,
        a_to: BURNFUSE,
        b_to: BURNFUSE,
        prob: 0.5,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: SPARK,
        b: FUSE,
        a_to: SPARK,
        b_to: BURNFUSE,
        prob: 1.0,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: CHARGED,
        b: FUSE,
        a_to: CHARGED,
        b_to: BURNFUSE,
        prob: 1.0,
        min_temp: NEVER_COLD,
    },
    // Hydrogen: ignites readily (and detonates via the blast hook).
    Reaction {
        a: FIRE,
        b: HYDROGEN,
        a_to: FIRE,
        b_to: FIRE,
        prob: 0.9,
        min_temp: NEVER_COLD,
    },
    // Oxygen: a combustion accelerant — fire tears straight through it.
    Reaction {
        a: FIRE,
        b: OXYGEN,
        a_to: FIRE,
        b_to: FIRE,
        prob: 0.95,
        min_temp: NEVER_COLD,
    },
    // Coal: smoulders — fire creeps slowly, so a coal bed burns for a long time.
    Reaction {
        a: FIRE,
        b: COAL,
        a_to: FIRE,
        b_to: FIRE,
        prob: 0.03,
        min_temp: NEVER_COLD,
    },
    // Quench: lava meeting water sometimes flash-freezes to obsidian (+ steam).
    Reaction {
        a: LAVA,
        b: WATER,
        a_to: OBSIDIAN,
        b_to: STEAM,
        prob: 0.25,
        min_temp: NEVER_COLD,
    },
    // Plant roots spread through soil.
    Reaction {
        a: PLANT,
        b: SOIL,
        a_to: PLANT,
        b_to: PLANT,
        prob: 0.05,
        min_temp: NEVER_COLD,
    },
    // Embers set fire to what they touch.
    Reaction {
        a: EMBER,
        b: OIL,
        a_to: EMBER,
        b_to: FIRE,
        prob: 0.5,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: EMBER,
        b: WOOD,
        a_to: EMBER,
        b_to: FIRE,
        prob: 0.2,
        min_temp: NEVER_COLD,
    },
    Reaction {
        a: EMBER,
        b: FUME,
        a_to: EMBER,
        b_to: FIRE,
        prob: 0.6,
        min_temp: NEVER_COLD,
    },
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

/// One-line, human-facing description of what an element does and its notable
/// interactions. Surfaced by the app as a palette tooltip so the reaction web is
/// discoverable. A plain `match` rather than a table column: it keeps the big
/// [`MATERIALS`] table free of prose. Every [`user_paintable`] id has text;
/// hidden internal states fall through to "".
pub fn blurb(id: MaterialId) -> &'static str {
    match id {
        STONE => "Inert wall. Blast-proof, but lava and thermite melt through it.",
        SAND => "Heavy inert powder. Melts to glass when very hot.",
        WATER => "Seeks its level. Freezes to ice, boils to steam.",
        OIL => "Floats on water. Flammable; self-ignites when hot.",
        SMOKE => "Light gas. Rises and fades away.",
        ICE => "Melts to water. Salt turns it to non-freezing brine.",
        STEAM => "Hot gas. Rises, then condenses back to water.",
        LAVA => "Molten rock. Cools to basalt/obsidian on water; melts stone.",
        BASALT => "Cooled lava. Remelts at extreme heat.",
        COPPER => "Conductor. Carries a spark's charge and conducts heat far.",
        SPARK => "Igniter. Lights wires, oil and fuses, then vanishes.",
        FIRE => "Spreads to fuel, burns out, leaves smoke.",
        ACID => "Corrosive liquid. Dissolves most matter and is used up doing so.",
        FUME => "Flammable gas. Carries flame upward.",
        GUNPOWDER => "Explosive powder. Chain-detonates.",
        CRYO => "Cold source. Freezes nearby water to ice.",
        WOOD => "Flammable solid. Fire creeps along it.",
        GLASS => "Inert. What sand melts into.",
        PLASMA => "Searing, no-trace heat 'flame'.",
        FROST => "Freezing, no-trace cold 'flame'.",
        CLONE => "Endlessly copies whatever element touches it.",
        VOID => "Infinite sink. Swallows anything it touches.",
        SALT => "Melts ice into non-freezing brine.",
        PLANT => "Grows over soil. Ants graze it; fire burns it.",
        THERMITE => "Flashes to molten slag. Melts through stone.",
        SALTWATER => "Brine. Salty water that will not freeze.",
        BATTERY => "Endless charge source for wires.",
        LAMP => "Lights when charged. Lit lamps chain to neighbours.",
        FUSE => "Burns along its length to detonate bombs.",
        HYDROGEN => "Light gas. Violently explosive.",
        NITRO => "Extremely volatile. Large blast.",
        TNT => "Big blast. Chain-detonates with other explosives.",
        WAX => "Solid. Melts to molten wax when heated.",
        MELTWAX => "Molten wax. Resolidifies when it cools.",
        COAL => "Slow-burning solid fuel.",
        OBSIDIAN => "Glassy rock from fast-cooled lava.",
        HEATER => "Persistent heat source.",
        COOLER => "Persistent cold source.",
        SNOW => "Light powder. Melts to water.",
        ASH => "Light powder. Burn residue.",
        OXYGEN => "Feeds fire. Flammable gas.",
        SOIL => "Earthy powder. Plant grows on it.",
        EMBER => "Glowing hot powder. Ignites fuel it touches.",
        DIAMOND => "The only fireproof, acid-proof, blast-proof solid.",
        FISH => "Swims through water. Flops when stranded.",
        WORM => "Burrows down through powders.",
        ANT => "Walks on surfaces and grazes on plant.",
        DRAIN => "Liquid-only sink. Empties a tank without eating the walls.",
        _ => "",
    }
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
    fn every_paintable_element_has_a_blurb() {
        for id in 1..MATERIALS.len() as MaterialId {
            if user_paintable(id) {
                assert!(
                    !blurb(id).is_empty(),
                    "{} (id {id}) is paintable but has no blurb",
                    props(id).name
                );
            }
        }
    }

    #[test]
    fn conductivities_in_range() {
        // The diffusion pass clamps the per-cell total edge weight to <= 1, so any
        // conductivity in [0, 1] is stable. Conductors use up to ~0.5 (fast
        // conduction through thin structures); insulators stay low.
        assert!(MATERIALS
            .iter()
            .all(|m| m.conductivity >= 0.0 && m.conductivity <= 1.0));
    }
}
