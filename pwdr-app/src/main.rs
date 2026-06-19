//! `pwdr-app` — macroquad frontend. Owns window, input, render, UI.
//! All simulation logic lives in `pwdr-core`; this file only draws it and feeds
//! it user edits.

use macroquad::prelude::*;
use pwdr_core::material::{self, MaterialId, Phase, EMPTY};
use pwdr_core::Grid;
use std::time::Instant;

const GRID_W: usize = 256;
const GRID_H: usize = 256;
const SCALE: f32 = 3.0;
const PANEL_W: f32 = 240.0;
const SEED: u64 = 0xC0FFEE;
const SAVE_PATH: &str = "pwdr.save";

/// Fixed simulation timestep (60 Hz), decoupled from render framerate.
const TICK_DT: f64 = 1.0 / 60.0;

fn window_conf() -> Conf {
    Conf {
        window_title: "pwdr".to_owned(),
        window_width: (GRID_W as f32 * SCALE + PANEL_W) as i32,
        window_height: (GRID_H as f32 * SCALE) as i32,
        ..Default::default()
    }
}

const PHASE_ORDER: [(Phase, &str); 5] = [
    (Phase::Powder, "POWDERS"),
    (Phase::Liquid, "LIQUIDS"),
    (Phase::Gas, "GASES"),
    (Phase::Solid, "SOLIDS"),
    (Phase::Energy, "ENERGY"),
];

fn swatch(id: MaterialId) -> Color {
    let c = material::props(id).color;
    Color::from_rgba(c[0], c[1], c[2], 255)
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut grid = Grid::new(GRID_W, GRID_H, SEED);

    let mut image = Image::gen_image_color(GRID_W as u16, GRID_H as u16, BLACK);
    let texture = Texture2D::from_image(&image);
    texture.set_filter(FilterMode::Nearest);

    let sim_w = GRID_W as f32 * SCALE;
    let sim_h = GRID_H as f32 * SCALE;

    let mut acc = 0.0f64;
    let mut brush = 4usize;
    let mut selected: MaterialId = material::SAND;
    let mut paused = false;
    let mut search = String::new();
    let mut last_tick_ms = 0.0f32;
    let mut status = String::new();

    loop {
        // --- text input drives the palette search box ---
        while let Some(c) = get_char_pressed() {
            let lc = c.to_ascii_lowercase();
            if lc.is_ascii_alphabetic() {
                search.push(lc);
            }
        }
        if is_key_pressed(KeyCode::Backspace) {
            search.pop();
        }
        if is_key_pressed(KeyCode::Escape) {
            search.clear();
        }

        // --- controls (non-text keys, so they never collide with search) ---
        if is_key_pressed(KeyCode::Space) {
            paused = !paused;
        }
        let mut do_single_step = false;
        if is_key_pressed(KeyCode::Right) {
            do_single_step = true;
        }
        if is_key_pressed(KeyCode::LeftBracket) && brush > 0 {
            brush -= 1;
        }
        if is_key_pressed(KeyCode::RightBracket) {
            brush += 1;
        }
        if is_key_pressed(KeyCode::Delete) {
            grid = Grid::new(GRID_W, GRID_H, SEED);
            status = "cleared".into();
        }
        if is_key_pressed(KeyCode::F5) {
            match std::fs::write(SAVE_PATH, grid.serialize()) {
                Ok(_) => status = format!("saved {SAVE_PATH}"),
                Err(e) => status = format!("save failed: {e}"),
            }
        }
        if is_key_pressed(KeyCode::F9) {
            match std::fs::read(SAVE_PATH).ok().and_then(|b| Grid::deserialize(&b)) {
                Some(g) if g.width() == GRID_W && g.height() == GRID_H => {
                    grid = g;
                    status = format!("loaded {SAVE_PATH}");
                }
                _ => status = "load failed".into(),
            }
        }

        // --- build the (filtered) palette layout, used for both draw + click ---
        let palette = build_palette(&search, sim_w);

        // --- mouse: palette click vs. brush paint ---
        let (mx, my) = mouse_position();
        let in_panel = mx >= sim_w;
        if is_mouse_button_pressed(MouseButton::Left) && in_panel {
            for entry in &palette {
                if let PaletteItem::Mat(id, rect) = entry {
                    if rect.contains(vec2(mx, my)) {
                        selected = *id;
                    }
                }
            }
        }
        if !in_panel
            && (is_mouse_button_down(MouseButton::Left) || is_mouse_button_down(MouseButton::Right))
        {
            let gx = (mx / SCALE) as i32;
            let gy = (my / SCALE) as i32;
            if gx >= 0 && gy >= 0 && (gx as usize) < GRID_W && (gy as usize) < GRID_H {
                let mat = if is_mouse_button_down(MouseButton::Right) {
                    EMPTY
                } else {
                    selected
                };
                grid.paint(gx as usize, gy as usize, brush, mat);
            }
        }

        // --- fixed-timestep simulation, timed ---
        if paused {
            acc = 0.0;
            if do_single_step {
                let t = Instant::now();
                grid.step();
                last_tick_ms = t.elapsed().as_secs_f32() * 1000.0;
            }
        } else {
            acc += get_frame_time() as f64;
            let mut stepped = false;
            let t = Instant::now();
            let mut n = 0;
            while acc >= TICK_DT && n < 8 {
                grid.step();
                acc -= TICK_DT;
                stepped = true;
                n += 1;
            }
            if stepped {
                last_tick_ms = t.elapsed().as_secs_f32() * 1000.0 / n as f32;
            }
        }

        // --- render ---
        let fb = grid.render_rgba();
        image.get_image_data_mut().copy_from_slice(rgba_chunks(fb));
        texture.update(&image);

        clear_background(Color::from_rgba(18, 18, 22, 255));
        draw_texture_ex(
            &texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(sim_w, sim_h)),
                ..Default::default()
            },
        );

        // brush cursor outline
        if !in_panel {
            let r = brush as f32 * SCALE + SCALE * 0.5;
            draw_circle_lines(mx, my, r, 1.0, Color::from_rgba(255, 255, 255, 120));
        }

        draw_palette(&palette, selected, &search, sim_w);
        draw_hud(&grid, selected, brush, paused, last_tick_ms, &status, mx, my, sim_w, sim_h);

        next_frame().await;
    }
}

/// A laid-out palette item: either a category header or a clickable material.
enum PaletteItem {
    Header(String, f32),
    Mat(MaterialId, Rect),
}

/// Compute palette layout (positions) filtered by `search`. Shared by draw+click
/// so they can never disagree.
fn build_palette(search: &str, x0: f32) -> Vec<PaletteItem> {
    let mut items = Vec::new();
    let pad = 8.0;
    let row_h = 22.0;
    let mut y = 56.0; // below the search box
    for (phase, label) in PHASE_ORDER {
        let mats: Vec<MaterialId> = (1..material::MATERIALS.len() as MaterialId)
            .filter(|&id| material::props(id).phase == phase)
            .filter(|&id| {
                search.is_empty()
                    || material::props(id).name.to_ascii_lowercase().contains(search)
            })
            .collect();
        if mats.is_empty() {
            continue;
        }
        items.push(PaletteItem::Header(label.to_string(), y));
        y += row_h;
        for id in mats {
            let rect = Rect::new(x0 + pad, y, PANEL_W - pad * 2.0, row_h - 3.0);
            items.push(PaletteItem::Mat(id, rect));
            y += row_h;
        }
        y += 4.0;
    }
    items
}

fn draw_palette(palette: &[PaletteItem], selected: MaterialId, search: &str, x0: f32) {
    draw_rectangle(x0, 0.0, PANEL_W, screen_height(), Color::from_rgba(28, 28, 34, 255));
    draw_line(x0, 0.0, x0, screen_height(), 1.0, Color::from_rgba(60, 60, 70, 255));

    // search box
    draw_text("search:", x0 + 8.0, 22.0, 18.0, GRAY);
    draw_rectangle(x0 + 8.0, 28.0, PANEL_W - 16.0, 22.0, Color::from_rgba(12, 12, 16, 255));
    let shown = if search.is_empty() { "_" } else { search };
    draw_text(shown, x0 + 12.0, 44.0, 18.0, WHITE);

    for item in palette {
        match item {
            PaletteItem::Header(label, y) => {
                draw_text(label, x0 + 8.0, y + 15.0, 16.0, Color::from_rgba(150, 150, 165, 255));
            }
            PaletteItem::Mat(id, rect) => {
                if *id == selected {
                    draw_rectangle(rect.x - 2.0, rect.y - 1.0, rect.w + 4.0, rect.h + 2.0, Color::from_rgba(80, 80, 100, 255));
                }
                draw_rectangle(rect.x, rect.y, 16.0, rect.h, swatch(*id));
                draw_rectangle_lines(rect.x, rect.y, 16.0, rect.h, 1.0, Color::from_rgba(0, 0, 0, 180));
                draw_text(material::props(*id).name, rect.x + 22.0, rect.y + 14.0, 18.0, WHITE);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_hud(
    grid: &Grid,
    selected: MaterialId,
    brush: usize,
    paused: bool,
    tick_ms: f32,
    status: &str,
    mx: f32,
    my: f32,
    sim_w: f32,
    sim_h: f32,
) {
    let line = |i: f32, s: &str| draw_text(s, 8.0, 18.0 + i * 18.0, 18.0, WHITE);

    // shaded strip for legibility
    draw_rectangle(0.0, 0.0, 320.0, 96.0, Color::from_rgba(0, 0, 0, 110));
    line(0.0, &format!("pwdr  {}x{}  {:.0} fps", GRID_W, GRID_H, get_fps()));
    line(1.0, &format!("tick {:.3} ms  {}", tick_ms, if paused { "PAUSED" } else { "running" }));
    line(2.0, &format!("brush [{}]  sel: {}", brush, material::props(selected).name));

    // cell readout under cursor
    if mx >= 0.0 && mx < sim_w && my >= 0.0 && my < sim_h {
        let gx = (mx / SCALE) as usize;
        let gy = (my / SCALE) as usize;
        if gx < grid.width() && gy < grid.height() {
            let m = grid.material_at(gx, gy);
            line(
                3.0,
                &format!(
                    "({},{}) {}  {:.0}C",
                    gx,
                    gy,
                    material::props(m).name,
                    grid.temperature_at(gx, gy)
                ),
            );
        }
    }

    // controls hint + status at the bottom of the sim area
    draw_text(
        "L paint  R erase  Space pause  -> step  [ ] brush  F5 save  F9 load  Del clear",
        8.0,
        sim_h - 24.0,
        16.0,
        Color::from_rgba(200, 200, 210, 220),
    );
    if !status.is_empty() {
        draw_text(status, 8.0, sim_h - 8.0, 16.0, Color::from_rgba(140, 220, 140, 230));
    }
}

/// Reinterpret a flat RGBA `[u8]` as `[[u8;4]]` for macroquad's `Image`.
fn rgba_chunks(fb: &[u8]) -> &[[u8; 4]] {
    // SAFETY: fb.len() is a multiple of 4 (RGBA) and [u8;4] has alignment 1.
    unsafe { std::slice::from_raw_parts(fb.as_ptr() as *const [u8; 4], fb.len() / 4) }
}
