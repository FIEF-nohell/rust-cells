//! Generate a hand-designed "showcase" diorama featuring (nearly) every element
//! in a natural context — a volcano, an ocean, a snowy mountain, a wired house,
//! a sky, and an underground. Written to the Desktop as `rust-cells-showcase.save`.
//! Load it in the app with F8 — it opens PAUSED, so it reads as a picture; hit
//! Space to bring it to life (lava flows into the sea, the circuit lights the
//! lamp, fuses can be lit, the waterfall runs, ...).
//!
//! Run with: `cargo run -p pwdr-core --example showcase`

use pwdr_core::material::*;
use pwdr_core::Grid;

const W: usize = 529;
const H: usize = 357;

fn main() {
    let mut g = Grid::new(W, H, 0xC0FFEE);

    terrain(&mut g);
    ocean(&mut g);
    volcano(&mut g);
    mountain(&mut g);
    house(&mut g);
    sky(&mut g);
    underground(&mut g);
    decor(&mut g);

    // Coverage report: warn about any paintable element not present.
    let missing: Vec<&str> = (1..MATERIALS.len() as MaterialId)
        .filter(|&id| user_paintable(id) && g.count(id) == 0)
        .map(|id| props(id).name)
        .collect();
    if missing.is_empty() {
        println!("coverage: all paintable elements present");
    } else {
        println!("coverage: MISSING {:?}", missing);
    }

    let blob = g.serialize();
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    let path = std::path::Path::new(&home)
        .join("Desktop")
        .join("rust-cells-showcase.save");
    std::fs::write(&path, blob).expect("write showcase save");
    println!("wrote {} ({W}x{H})", path.display());
}

// --- drawing helpers -------------------------------------------------------

fn put(g: &mut Grid, x: i32, y: i32, m: MaterialId) {
    if x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H {
        g.set(x as usize, y as usize, m);
    }
}

fn rect(g: &mut Grid, x0: i32, y0: i32, x1: i32, y1: i32, m: MaterialId) {
    for y in y0..=y1 {
        for x in x0..=x1 {
            put(g, x, y, m);
        }
    }
}

fn disc(g: &mut Grid, cx: i32, cy: i32, r: i32, m: MaterialId) {
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                put(g, cx + dx, cy + dy, m);
            }
        }
    }
}

fn hline(g: &mut Grid, x0: i32, x1: i32, y: i32, m: MaterialId) {
    for x in x0..=x1 {
        put(g, x, y, m);
    }
}

fn vline(g: &mut Grid, x: i32, y0: i32, y1: i32, m: MaterialId) {
    for y in y0..=y1 {
        put(g, x, y, m);
    }
}

// --- scenes ----------------------------------------------------------------

/// Rolling rock ground across the bottom, with a soil/sand top layer.
fn terrain(g: &mut Grid) {
    let base = 250;
    for x in 0..W as i32 {
        // gentle hills via two sine-ish bumps (integer approximation)
        let h = base
            + ((x - 200) * (x - 200) / 1600) // dip near the sea
            - (x / 30); // slight slope
        let surf = h.clamp(235, 300);
        rect(g, x, surf, x, H as i32 - 1, STONE);
        // sandy top
        rect(g, x, surf, x, surf + 2, SAND);
    }
}

/// A sea basin: water, an oil slick on top, a brine patch, mercury at the bottom,
/// seaweed (plant), and a sandy beach.
fn ocean(g: &mut Grid) {
    let (x0, x1) = (150, 330);
    let top = 232;
    let floor = 300;
    // carve the basin (empty) then fill
    rect(g, x0, top, x1, floor, EMPTY);
    rect(g, x0, top + 4, x1, floor, WATER);
    rect(g, x0 + 90, top + 6, x1, floor - 4, SALTWATER); // brine on the right half
    rect(g, x0, top + 2, x1, top + 3, OIL); // floating oil slick
    rect(g, x0 + 10, floor - 2, x0 + 40, floor, MERCURY); // heavy metal puddle
                                                          // seaweed
    for sx in (x0 + 20..x1 - 10).step_by(22) {
        vline(g, sx, floor - 18, floor - 1, PLANT);
    }
    // beach
    rect(g, x1, 240, x1 + 30, 252, SAND);
    rect(g, x1 + 18, 236, x1 + 26, 252, SALT); // salt flat
}

/// Volcano: a basalt/stone cone with a lava crater, a clone-fed lava fountain,
/// a thermite vein, a magma heater, spilling lava toward the sea (-> obsidian).
fn volcano(g: &mut Grid) {
    let (cx, base, peak) = (110, 250, 150);
    for y in peak..=base {
        let half = (y - peak) / 2 + 4;
        hline(g, cx - half, cx + half, y, BASALT);
        if y > peak + 6 {
            hline(g, cx - half + 2, cx + half - 2, y, STONE);
        }
    }
    // crater + lava
    rect(g, cx - 5, peak, cx + 5, peak + 16, LAVA);
    put(g, cx, peak + 14, CLONE); // lava fountain source (fed by surrounding lava)
    rect(g, cx - 2, peak + 18, cx + 2, peak + 26, HEATER); // magma chamber
    vline(g, cx + 7, peak + 20, base - 10, THERMITE); // thermite vein
                                                      // a lava lip spilling toward the sea on the right
    rect(g, cx + half_at(peak + 30), peak + 28, 150, peak + 31, LAVA);
    // a few obsidian rocks where lava has met water before
    disc(g, 150, 236, 3, OBSIDIAN);
}

fn half_at(_y: i32) -> i32 {
    18
}

/// Snowy mountain: stone massif, an ice/snow cap, a cryo core, a cooler "ice
/// cave", a frost aura, and a clone-fed waterfall running to the sea.
fn mountain(g: &mut Grid) {
    let (cx, base, peak) = (40, 250, 95);
    for y in peak..=base {
        let half = (y - peak) / 2 + 2;
        hline(g, cx - half, cx + half, y, STONE);
    }
    // ice cap, dusted with snow at the very top
    for y in peak..peak + 22 {
        let half = (y - peak) / 2 + 2;
        hline(g, cx - half, cx + half, y, ICE);
    }
    disc(g, cx, peak + 2, 4, SNOW); // snow cap
    disc(g, cx, peak + 30, 6, CRYO); // frozen heart
    rect(g, cx - 3, peak + 44, cx + 3, peak + 50, COOLER); // ice cave
    disc(g, cx, peak + 8, 3, FROST); // frost aura at the summit
                                     // waterfall: a clone fed by a water cap, pouring down the east face into the sea
    put(g, cx + 6, peak + 12, WATER);
    put(g, cx + 7, peak + 13, CLONE);
    rect(g, cx + 8, peak + 14, cx + 9, 232, WATER); // the falling stream (settles on play)
}

/// A little house: wood walls, glass windows, basalt roof, a battery+copper+lamp
/// circuit inside, a wax candle, a smoking chimney, a fridge (cooler) and a
/// furnace (heater).
fn house(g: &mut Grid) {
    let (x0, y1) = (430, 250); // bottom-left of house
    let (w, h) = (70, 46);
    let (x1, y0) = (x0 + w, y1 - h);
    // walls + floor
    rect(g, x0, y0, x1, y1, WOOD);
    rect(g, x0 + 2, y0 + 2, x1 - 2, y1 - 1, EMPTY); // hollow interior
                                                    // glass windows
    rect(g, x0 + 10, y0 + 12, x0 + 20, y0 + 22, GLASS);
    rect(g, x1 - 20, y0 + 12, x1 - 10, y0 + 22, GLASS);
    // basalt roof (triangle)
    for i in 0..(w / 2 + 2) {
        hline(g, x0 + i, x1 - i, y0 - i, BASALT);
    }
    // chimney with smoke
    rect(g, x1 - 14, y0 - 18, x1 - 10, y0 - 2, STONE);
    rect(g, x1 - 13, y0 - 22, x1 - 11, y0 - 19, SMOKE);
    rect(g, x1 - 13, y0 - 27, x1 - 11, y0 - 24, FUME);
    // circuit along the floor: battery -> copper wire -> lamp
    let fy = y1 - 3;
    put(g, x0 + 4, fy, BATTERY);
    hline(g, x0 + 5, x1 - 8, fy, COPPER);
    put(g, x0 + 6, fy - 1, SPARK); // a spark to kick it off on play
    rect(g, x1 - 7, fy - 3, x1 - 4, fy, LAMP); // a lamp cluster
                                               // candle on a wood table
    rect(g, x0 + 30, y1 - 6, x0 + 36, y1 - 5, WOOD);
    vline(g, x0 + 33, y1 - 12, y1 - 7, WAX);
    put(g, x0 + 33, y1 - 13, FIRE);
    put(g, x0 + 33, y1 - 11, MELTWAX);
    rect(g, x0 + 30, y1 - 4, x0 + 37, y1 - 2, ASH); // ash bed under the candle
                                                    // fridge + furnace appliances
    rect(g, x0 + 4, y1 - 14, x0 + 8, y1 - 6, COOLER);
    rect(g, x1 - 8, y1 - 14, x1 - 4, y1 - 6, HEATER);
}

/// Sky: a blazing sun (plasma core, fire corona), drifting clouds (steam/smoke),
/// and a hydrogen balloon on a fuse tether.
fn sky(g: &mut Grid) {
    // sun
    disc(g, 60, 40, 14, FIRE);
    disc(g, 60, 40, 6, PLASMA);
    // clouds
    for &(cx, cy) in &[(180, 40), (250, 55), (330, 38), (400, 60)] {
        disc(g, cx, cy, 9, STEAM);
        disc(g, cx + 8, cy + 2, 6, SMOKE);
    }
    // hydrogen balloon with a basket on a fuse tether
    disc(g, 480, 55, 12, HYDROGEN);
    vline(g, 480, 68, 86, FUSE);
    rect(g, 477, 86, 483, 90, WOOD);
    // a pocket of pure oxygen drifting in the sky (combustion accelerant)
    disc(g, 150, 30, 7, OXYGEN);
}

/// Underground: a coal seam, an acid cavern, a bomb cache wired to a surface
/// fuse, a fume pocket, a salt deposit, and a void drain.
fn underground(g: &mut Grid) {
    let top = 305;
    rect(g, 0, top, W as i32 - 1, H as i32 - 1, STONE);
    // coal seam
    rect(g, 60, 318, 200, 326, COAL);
    // acid cavern
    rect(g, 230, 320, 290, 345, EMPTY);
    rect(g, 230, 332, 290, 345, ACID);
    // salt deposit
    rect(g, 300, 318, 330, 330, SALT);
    // fume pocket
    rect(g, 340, 330, 370, 345, EMPTY);
    rect(g, 340, 338, 370, 345, FUME);
    // bomb cache wired to the surface with a fuse
    rect(g, 400, 322, 440, 345, EMPTY);
    rect(g, 402, 336, 420, 345, GUNPOWDER);
    rect(g, 422, 336, 438, 345, TNT);
    rect(g, 402, 330, 414, 335, NITRO);
    vline(g, 420, 250, 322, FUSE); // light the top end on the surface to detonate
                                   // void drain in the corner
    disc(g, 515, 348, 5, VOID);
}

/// Surface decoration: a palm tree and a couple of torches.
fn decor(g: &mut Grid) {
    // palm tree on the beach
    let (tx, ty) = (350, 240);
    vline(g, tx, ty - 24, ty, WOOD);
    disc(g, tx, ty - 26, 7, PLANT);
    // torches (wood post + flame) flanking the house path
    for &px in &[415, 470] {
        vline(g, px, 240, 250, WOOD);
        put(g, px, 238, FIRE);
    }
    // a bed of soil with the plant rooted in it
    rect(g, tx - 7, ty + 1, tx + 7, ty + 4, SOIL);
    // glowing embers in a little fire pit on the beach
    disc(g, 300, 246, 3, EMBER);
    // diamond gems embedded deep underground
    disc(g, 470, 338, 3, DIAMOND);
    disc(g, 130, 344, 2, DIAMOND);
}
