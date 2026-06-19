//! Generate a clean "showcase" map: a tidy grid of swatches, one per paintable
//! element, grouped by phase. Written to the Desktop as `pwdr-showcase.save`.
//! Load it in the app with F8 (it opens PAUSED). Hover a swatch to read its name.
//!
//! Run with: `cargo run -p pwdr-core --example showcase`

use pwdr_core::material::{self, MaterialId, Phase};
use pwdr_core::Grid;

const COLS: usize = 8;
const SW: usize = 12; // swatch size
const STEP: usize = 18; // grid spacing
const M: usize = 12; // margin

fn main() {
    let order = [
        Phase::Powder,
        Phase::Liquid,
        Phase::Gas,
        Phase::Solid,
        Phase::Energy,
    ];

    // Lay out swatch positions, grouped by phase with a gap row between groups.
    let mut placements: Vec<(MaterialId, usize, usize)> = Vec::new();
    let mut row = 0usize;
    for phase in order {
        let mats: Vec<MaterialId> = (1..material::MATERIALS.len() as MaterialId)
            .filter(|&id| material::user_paintable(id) && material::props(id).phase == phase)
            .collect();
        if mats.is_empty() {
            continue;
        }
        let mut col = 0usize;
        for id in mats {
            placements.push((id, M + col * STEP, M + row * STEP));
            col += 1;
            if col == COLS {
                col = 0;
                row += 1;
            }
        }
        if col != 0 {
            row += 1;
        }
        row += 1; // blank row between phases
    }

    let w = placements.iter().map(|&(_, x, _)| x + SW).max().unwrap() + M;
    let h = placements.iter().map(|&(_, _, y)| y + SW).max().unwrap() + M;

    let mut g = Grid::new(w, h, 0xC0FFEE);
    for (id, px, py) in placements {
        for y in py..py + SW {
            for x in px..px + SW {
                g.set(x, y, id);
            }
        }
    }

    let blob = g.serialize();
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    let path = std::path::Path::new(&home)
        .join("Desktop")
        .join("pwdr-showcase.save");
    std::fs::write(&path, blob).expect("write showcase save");
    println!("wrote {} ({w}x{h})", path.display());
}
