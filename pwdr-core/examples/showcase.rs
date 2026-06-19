//! Generate a "showcase" map containing a stone-walled compartment for every
//! paintable element, and write it to the user's Desktop as `pwdr-showcase.save`.
//! Load it in the app with F8. Hover a compartment to read the element's name.
//!
//! Run with: `cargo run -p pwdr-core --example showcase`

use pwdr_core::material::{self, MaterialId};
use pwdr_core::Grid;

fn main() {
    let paint: Vec<MaterialId> = (1..material::MATERIALS.len() as MaterialId)
        .filter(|&id| material::user_paintable(id))
        .collect();

    let cols = 7usize;
    let (cw, ch) = (18usize, 18usize); // compartment size
    let rows = paint.len().div_ceil(cols);
    let w = cols * cw + 1;
    let h = rows * ch + 1;

    let mut g = Grid::new(w, h, 0xC0FFEE);

    // Compartment grid: stone walls.
    for r in 0..=rows {
        let y = (r * ch).min(h - 1);
        for x in 0..w {
            g.set(x, y, material::STONE);
        }
    }
    for c in 0..=cols {
        let x = (c * cw).min(w - 1);
        for y in 0..h {
            g.set(x, y, material::STONE);
        }
    }

    // Fill each compartment interior with one element.
    for (idx, &id) in paint.iter().enumerate() {
        let c = idx % cols;
        let r = idx / cols;
        let x0 = c * cw + 1;
        let y0 = r * ch + 1;
        let x1 = (x0 + cw - 1).min(w - 1);
        let y1 = (y0 + ch - 1).min(h - 1);
        for y in y0..y1 {
            for x in x0..x1 {
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
    println!(
        "wrote {} ({w}x{h}, {} elements)",
        path.display(),
        paint.len()
    );
}
