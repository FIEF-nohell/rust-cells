//! `pwdr-app` — macroquad frontend. Owns window, input, render, UI.
//! All simulation logic lives in `pwdr-core`; this file only draws it and feeds
//! it user edits.

use macroquad::prelude::*;
use pwdr_core::material::{self, MaterialId, Phase, EMPTY};
use pwdr_core::Grid;
use std::time::Instant;

const SCALE: f32 = 3.0; // screen pixels per cell
const PANEL_W: f32 = 240.0;
const SEED: u64 = 0xC0FFEE;
const SAVE_PATH: &str = "pwdr.save";
const MAX_BRUSH: usize = 64;

/// Fixed simulation timestep (60 Hz), decoupled from render framerate.
const TICK_DT: f64 = 1.0 / 60.0;

fn window_conf() -> Conf {
    Conf {
        window_title: "pwdr".to_owned(),
        window_width: (256.0 * SCALE + PANEL_W) as i32,
        window_height: (256.0 * SCALE) as i32,
        window_resizable: true,
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

/// Cell grid dimensions that fit the current window (leaving room for the panel).
fn grid_dims(win_w: f32, win_h: f32) -> (usize, usize) {
    let avail = (win_w - PANEL_W).max(SCALE * 16.0);
    let gw = (avail / SCALE).floor() as usize;
    let gh = (win_h / SCALE).floor() as usize;
    (gw.max(16), gh.max(16))
}

fn make_texture(gw: usize, gh: usize) -> (Image, Texture2D) {
    let image = Image::gen_image_color(gw as u16, gh as u16, BLACK);
    let texture = Texture2D::from_image(&image);
    texture.set_filter(FilterMode::Nearest);
    (image, texture)
}

#[macroquad::main(window_conf)]
async fn main() {
    let (mut win_w, mut win_h) = (screen_width(), screen_height());
    let (mut gw, mut gh) = grid_dims(win_w, win_h);
    let mut grid = Grid::new(gw, gh, SEED);
    let (mut image, mut texture) = make_texture(gw, gh);

    let mut acc = 0.0f64;
    let mut brush = 4usize;
    let mut selected: MaterialId = material::SAND;
    let mut paused = false;
    let mut search = String::new();
    let mut last_tick_ms = 0.0f32;
    let mut status = String::new();
    let mut pending_resize = false;

    loop {
        let sim_px_w = gw as f32 * SCALE;
        let sim_px_h = gh as f32 * SCALE;
        let panel_x = screen_width() - PANEL_W;

        // --- window resize: prompt before wiping ---
        if (screen_width() - win_w).abs() > 0.5 || (screen_height() - win_h).abs() > 0.5 {
            pending_resize = true;
        }
        if pending_resize {
            if is_key_pressed(KeyCode::Enter) {
                win_w = screen_width();
                win_h = screen_height();
                let (ngw, ngh) = grid_dims(win_w, win_h);
                gw = ngw;
                gh = ngh;
                grid = Grid::new(gw, gh, SEED);
                let (ni, nt) = make_texture(gw, gh);
                image = ni;
                texture = nt;
                pending_resize = false;
                status = format!("resized to {gw}x{gh}");
            } else if is_key_pressed(KeyCode::Escape) {
                // Keep the canvas; stop tracking this size so we don't re-prompt.
                win_w = screen_width();
                win_h = screen_height();
                pending_resize = false;
            }
            draw_resize_prompt(gw, gh);
            next_frame().await;
            continue;
        }

        // --- palette search typing (alphabetic only, so controls never collide) ---
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

        // --- controls ---
        if is_key_pressed(KeyCode::Space) {
            paused = !paused;
        }
        let mut do_single_step = false;
        if is_key_pressed(KeyCode::Right) {
            do_single_step = true;
        }
        // Brush size: Shift + mouse wheel.
        let (_, wheel) = mouse_wheel();
        if wheel != 0.0 && (is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift)) {
            if wheel > 0.0 {
                brush = (brush + 1).min(MAX_BRUSH);
            } else {
                brush = brush.saturating_sub(1);
            }
        }
        if is_key_pressed(KeyCode::Delete) {
            grid = Grid::new(gw, gh, SEED);
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
                Some(g) => {
                    gw = g.width();
                    gh = g.height();
                    grid = g;
                    let (ni, nt) = make_texture(gw, gh);
                    image = ni;
                    texture = nt;
                    status = format!("loaded {SAVE_PATH} ({gw}x{gh})");
                }
                None => status = "load failed".into(),
            }
        }

        // --- palette layout (shared by draw + click) ---
        let palette = build_palette(&search, panel_x);

        // --- mouse: palette click vs. brush paint ---
        let (mx, my) = mouse_position();
        let in_panel = mx >= panel_x;
        if is_mouse_button_pressed(MouseButton::Left) && in_panel {
            for entry in &palette {
                if let PaletteItem::Mat(id, rect) = entry {
                    if rect.contains(vec2(mx, my)) {
                        selected = *id;
                    }
                }
            }
        }
        let painting = !in_panel
            && mx < sim_px_w
            && my < sim_px_h
            && (is_mouse_button_down(MouseButton::Left) || is_mouse_button_down(MouseButton::Right));
        if painting {
            let gx = (mx / SCALE) as usize;
            let gy = (my / SCALE) as usize;
            if gx < gw && gy < gh {
                // Left paints (only into empty cells); right erases anything.
                let mat = if is_mouse_button_down(MouseButton::Right) {
                    EMPTY
                } else {
                    selected
                };
                grid.paint(gx, gy, brush, mat);
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
            let t = Instant::now();
            let mut n = 0;
            while acc >= TICK_DT && n < 8 {
                grid.step();
                acc -= TICK_DT;
                n += 1;
            }
            if n > 0 {
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
                dest_size: Some(vec2(sim_px_w, sim_px_h)),
                ..Default::default()
            },
        );

        if !in_panel && mx < sim_px_w && my < sim_px_h {
            let r = brush as f32 * SCALE + SCALE * 0.5;
            draw_circle_lines(mx, my, r, 1.0, Color::from_rgba(255, 255, 255, 120));
        }

        draw_palette(&palette, selected, &search, panel_x);
        draw_hud(&grid, gw, gh, selected, brush, paused, last_tick_ms, &status, mx, my, sim_px_w, sim_px_h);

        next_frame().await;
    }
}

enum PaletteItem {
    Header(String, f32),
    Mat(MaterialId, Rect),
}

fn build_palette(search: &str, x0: f32) -> Vec<PaletteItem> {
    let mut items = Vec::new();
    let pad = 8.0;
    let row_h = 22.0;
    let mut y = 56.0;
    for (phase, label) in PHASE_ORDER {
        let mats: Vec<MaterialId> = (1..material::MATERIALS.len() as MaterialId)
            .filter(|&id| material::user_paintable(id))
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

    draw_text("search (type to filter):", x0 + 8.0, 22.0, 18.0, GRAY);
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
    gw: usize,
    gh: usize,
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

    draw_rectangle(0.0, 0.0, 340.0, 96.0, Color::from_rgba(0, 0, 0, 110));
    line(0.0, &format!("pwdr  {}x{}  {:.0} fps", gw, gh, get_fps()));
    line(1.0, &format!("tick {:.3} ms  {}", tick_ms, if paused { "PAUSED" } else { "running" }));
    line(2.0, &format!("brush {}  sel: {}", brush, material::props(selected).name));

    if mx >= 0.0 && mx < sim_w && my >= 0.0 && my < sim_h {
        let gx = (mx / SCALE) as usize;
        let gy = (my / SCALE) as usize;
        if gx < grid.width() && gy < grid.height() {
            let m = grid.material_at(gx, gy);
            line(
                3.0,
                &format!("({},{}) {}  {:.0}C", gx, gy, material::props(m).name, grid.temperature_at(gx, gy)),
            );
        }
    }

    draw_text(
        "L paint (empty only)  R erase  Space pause  -> step  Shift+wheel brush  F5 save  F9 load  Del clear",
        8.0,
        sim_h - 24.0,
        16.0,
        Color::from_rgba(200, 200, 210, 220),
    );
    if !status.is_empty() {
        draw_text(status, 8.0, sim_h - 8.0, 16.0, Color::from_rgba(140, 220, 140, 230));
    }
}

fn draw_resize_prompt(gw: usize, gh: usize) {
    draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::from_rgba(0, 0, 0, 180));
    let (cw, ch) = (520.0, 130.0);
    let (cx, cy) = ((screen_width() - cw) * 0.5, (screen_height() - ch) * 0.5);
    draw_rectangle(cx, cy, cw, ch, Color::from_rgba(32, 32, 40, 255));
    draw_rectangle_lines(cx, cy, cw, ch, 2.0, Color::from_rgba(120, 120, 140, 255));
    draw_text("Window resized", cx + 20.0, cy + 34.0, 26.0, WHITE);
    draw_text(
        "Resizing the canvas will WIPE everything you've drawn.",
        cx + 20.0,
        cy + 66.0,
        18.0,
        Color::from_rgba(230, 200, 200, 255),
    );
    let (ngw, ngh) = grid_dims(screen_width(), screen_height());
    draw_text(
        &format!("[Enter] resize & clear -> {ngw}x{ngh}     [Esc] keep current {gw}x{gh}"),
        cx + 20.0,
        cy + 98.0,
        18.0,
        Color::from_rgba(180, 220, 180, 255),
    );
}

/// Reinterpret a flat RGBA `[u8]` as `[[u8;4]]` for macroquad's `Image`.
fn rgba_chunks(fb: &[u8]) -> &[[u8; 4]] {
    // SAFETY: fb.len() is a multiple of 4 (RGBA) and [u8;4] has alignment 1.
    unsafe { std::slice::from_raw_parts(fb.as_ptr() as *const [u8; 4], fb.len() / 4) }
}
