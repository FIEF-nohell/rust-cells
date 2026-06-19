//! `pwdr-app` — macroquad frontend. Owns window, input, render, UI.
//! All simulation logic lives in `pwdr-core`; this file only draws it and feeds
//! it user edits.

// Release builds are a GUI app: no console window. Debug keeps the console for logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use macroquad::prelude::*;
use pwdr_core::material::{self, MaterialId, Phase, EMPTY};
use pwdr_core::Grid;
use std::time::Instant;

const SCALE: f32 = 3.0; // screen pixels per cell
const PANEL_W: f32 = 240.0;
const SEED: u64 = 0xC0FFEE;
const MAX_BRUSH: usize = 64;

/// A path on the user's Desktop (so saves are easy to find).
fn desktop_path(name: &str) -> std::path::PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    std::path::Path::new(&home).join("Desktop").join(name)
}

/// Fixed simulation timestep (60 Hz), decoupled from render framerate.
const TICK_DT: f64 = 1.0 / 60.0;

fn window_conf() -> Conf {
    Conf {
        window_title: "rust-cells".to_owned(),
        // Default canvas: 400 wide x 250 tall cells (plus the side panel).
        window_width: (400.0 * SCALE + PANEL_W) as i32,
        window_height: (250.0 * SCALE) as i32,
        window_resizable: true,
        icon: Some(make_icon()),
        ..Default::default()
    }
}

/// One icon pixel (normalized coords): a little falling-sand scene — sand mound,
/// a falling grain stream, a water pool, and a fire ember, on a dark tile.
fn icon_pixel(nx: f32, ny: f32) -> [u8; 4] {
    let sand = [201, 182, 112, 255];
    let water = [42, 96, 205, 255];
    let fire = [255, 140, 42, 255];
    let bg = [24, 24, 38, 255];

    // fire ember (top-right)
    let (dx, dy) = (nx - 0.74, ny - 0.24);
    if dx * dx + dy * dy < 0.018 {
        return fire;
    }
    // falling grain stream (dotted)
    if (0.47..0.55).contains(&nx) && (0.26..0.60).contains(&ny) && ((ny * 22.0) as i32 % 3 != 0) {
        return sand;
    }
    // water pool (bottom-left)
    if nx < 0.40 && ny > 0.64 {
        return water;
    }
    // sand mound (right hill)
    let surface = 0.66 - (0.18 - (nx - 0.60).abs()).max(0.0);
    if ny > surface && nx > 0.36 {
        return sand;
    }
    bg
}

fn icon_image(n: usize) -> Vec<u8> {
    let mut v = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let nx = (x as f32 + 0.5) / n as f32;
            let ny = (y as f32 + 0.5) / n as f32;
            let c = icon_pixel(nx, ny);
            let i = (y * n + x) * 4;
            v[i..i + 4].copy_from_slice(&c);
        }
    }
    v
}

fn make_icon() -> macroquad::miniquad::conf::Icon {
    macroquad::miniquad::conf::Icon {
        small: icon_image(16).try_into().unwrap(),
        medium: icon_image(32).try_into().unwrap(),
        big: icon_image(64).try_into().unwrap(),
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
    let mut smooth_tick_ms = 0.0f32;
    let mut smooth_fps = 60.0f32;
    let mut status = String::new();
    let mut pending_resize = false;
    let mut heat_overlay = false;
    // Frames to ignore resize detection after we programmatically resize (load).
    let mut resize_grace = 0i32;
    let mut palette_scroll = 0.0f32;
    let mut zoom = 1.0f32;
    let mut view_x = 0.0f32; // top-left of the view, in grid cells
    let mut view_y = 0.0f32;
    let mut prev_mouse = (0.0f32, 0.0f32);
    let showcase_path = desktop_path("rust-cells-showcase.save");
    let desktop_dir = showcase_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| ".".into());

    loop {
        let panel_x = screen_width() - PANEL_W;

        // --- window resize: prompt before wiping (skip during load grace) ---
        // Ignore minimize / bogus tiny sizes so restoring doesn't trigger a wipe.
        let minimized = screen_width() < 120.0 || screen_height() < 120.0;
        if minimized {
            // don't touch win_w/win_h; restoring returns to the tracked size
        } else if resize_grace > 0 {
            resize_grace -= 1;
            win_w = screen_width();
            win_h = screen_height();
        } else if (screen_width() - win_w).abs() > 1.5 || (screen_height() - win_h).abs() > 1.5 {
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
        if is_key_pressed(KeyCode::F2) {
            heat_overlay = !heat_overlay;
        }
        let mut do_single_step = false;
        if is_key_pressed(KeyCode::Right) {
            do_single_step = true;
        }

        let (mx, my) = mouse_position();
        let in_panel = mx >= panel_x;

        let ctrl = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl);

        // Zoom toward a screen point, keeping that point fixed.
        let apply_zoom = |zoom: &mut f32, vx: &mut f32, vy: &mut f32, sx: f32, sy: f32, f: f32| {
            let p = SCALE * *zoom;
            let (gx, gy) = (*vx + sx / p, *vy + sy / p);
            *zoom = (*zoom * f).clamp(1.0, 16.0);
            let p2 = SCALE * *zoom;
            *vx = gx - sx / p2;
            *vy = gy - sy / p2;
        };

        // Mouse wheel: over panel -> scroll; Ctrl -> zoom; otherwise brush size.
        let (_, wheel) = mouse_wheel();
        if wheel != 0.0 {
            if in_panel {
                palette_scroll = (palette_scroll - wheel * 24.0).max(0.0);
            } else if ctrl {
                apply_zoom(
                    &mut zoom,
                    &mut view_x,
                    &mut view_y,
                    mx,
                    my,
                    if wheel > 0.0 { 1.2 } else { 1.0 / 1.2 },
                );
            } else if wheel > 0.0 {
                brush = (brush + 1).min(MAX_BRUSH);
            } else {
                brush = brush.saturating_sub(1);
            }
        }
        // Reliable keyboard zoom (no modifier): +/- and 0 to reset.
        let (cx, cy) = (panel_x * 0.5, screen_height() * 0.5);
        if is_key_pressed(KeyCode::Equal) || is_key_pressed(KeyCode::KpAdd) {
            apply_zoom(&mut zoom, &mut view_x, &mut view_y, cx, cy, 1.25);
        }
        if is_key_pressed(KeyCode::Minus) || is_key_pressed(KeyCode::KpSubtract) {
            apply_zoom(&mut zoom, &mut view_x, &mut view_y, cx, cy, 1.0 / 1.25);
        }
        if is_key_pressed(KeyCode::Key0) {
            zoom = 1.0;
            view_x = 0.0;
            view_y = 0.0;
        }

        let psc = SCALE * zoom; // recompute after any zoom change
        let vis_w = panel_x / psc;
        let vis_h = screen_height() / psc;

        // Minimap (overview) in the top-right of the canvas; click/drag to navigate.
        let mm_box = 150.0;
        let (mm_w, mm_h) = if gw >= gh {
            (mm_box, mm_box * gh as f32 / gw as f32)
        } else {
            (mm_box * gw as f32 / gh as f32, mm_box)
        };
        let (mm_x, mm_y) = (panel_x - mm_w - 8.0, 8.0);
        let over_mm = mx >= mm_x && mx < mm_x + mm_w && my >= mm_y && my < mm_y + mm_h;
        if over_mm && is_mouse_button_down(MouseButton::Left) {
            let fx = (mx - mm_x) / mm_w;
            let fy = (my - mm_y) / mm_h;
            view_x = fx * gw as f32 - vis_w * 0.5;
            view_y = fy * gh as f32 - vis_h * 0.5;
        }
        // Middle-drag pans the zoomed view.
        if is_mouse_button_down(MouseButton::Middle) {
            view_x -= (mx - prev_mouse.0) / psc;
            view_y -= (my - prev_mouse.1) / psc;
        }
        prev_mouse = (mx, my);
        // Clamp the view to the grid.
        view_x = view_x.clamp(0.0, (gw as f32 - vis_w).max(0.0));
        view_y = view_y.clamp(0.0, (gh as f32 - vis_h).max(0.0));
        if is_key_pressed(KeyCode::Delete) {
            grid = Grid::new(gw, gh, SEED);
            status = "cleared".into();
        }
        // F5 = save-as dialog. Native file picker; default to the Desktop.
        if is_key_pressed(KeyCode::F5) {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("rust-cells map", &["save"])
                .set_directory(&desktop_dir)
                .set_file_name("map.save")
                .save_file()
            {
                match std::fs::write(&path, grid.serialize()) {
                    Ok(_) => status = format!("saved {}", path.display()),
                    Err(e) => status = format!("save failed: {e}"),
                }
            }
        }
        // F9 = open dialog (pick any map), F8 = load the bundled showcase.
        let load_req = if is_key_pressed(KeyCode::F9) {
            rfd::FileDialog::new()
                .add_filter("rust-cells map", &["save"])
                .set_directory(&desktop_dir)
                .pick_file()
        } else if is_key_pressed(KeyCode::F8) {
            Some(showcase_path.clone())
        } else {
            None
        };
        if let Some(path) = load_req {
            match std::fs::read(&path)
                .ok()
                .and_then(|b| Grid::deserialize(&b))
            {
                Some(g) => {
                    gw = g.width();
                    gh = g.height();
                    grid = g;
                    let (ni, nt) = make_texture(gw, gh);
                    image = ni;
                    texture = nt;
                    // Resize the window to the loaded map (skip the wipe prompt
                    // while it settles) and start PAUSED so nothing reacts until
                    // the user is ready.
                    let (tw, th) = (gw as f32 * SCALE + PANEL_W, gh as f32 * SCALE);
                    request_new_screen_size(tw, th);
                    win_w = tw;
                    win_h = th;
                    resize_grace = 20;
                    paused = true;
                    status = format!("loaded {} ({gw}x{gh}) — PAUSED", path.display());
                }
                None => status = format!("load failed: {}", path.display()),
            }
        }

        // --- palette layout (shared by draw + click) ---
        let (palette, content_h) = build_palette(&search, panel_x);
        let max_scroll = (content_h - (screen_height() - 8.0)).max(0.0);
        palette_scroll = palette_scroll.min(max_scroll);

        // --- mouse: palette click vs. brush paint ---
        if is_mouse_button_pressed(MouseButton::Left) && in_panel {
            for entry in &palette {
                if let PaletteItem::Mat(id, rect) = entry {
                    // rects are in base (unscrolled) coords; shift the test point.
                    if rect.contains(vec2(mx, my + palette_scroll)) {
                        selected = *id;
                    }
                }
            }
        }
        let painting = !in_panel
            && !over_mm
            && (is_mouse_button_down(MouseButton::Left)
                || is_mouse_button_down(MouseButton::Right));
        if painting {
            let gx = (view_x + mx / psc).floor();
            let gy = (view_y + my / psc).floor();
            if gx >= 0.0 && gy >= 0.0 && (gx as usize) < gw && (gy as usize) < gh {
                // Left paints (only into empty cells); right erases anything.
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

        // --- telemetry smoothing (EMA) so the readout isn't jumpy ---
        let inst_fps = 1.0 / get_frame_time().max(1e-4);
        smooth_fps += (inst_fps - smooth_fps) * 0.08;
        smooth_tick_ms += (last_tick_ms - smooth_tick_ms) * 0.08;

        // --- render ---
        let fb = if heat_overlay {
            grid.render_temperature_rgba()
        } else {
            grid.render_rgba()
        };
        image.get_image_data_mut().copy_from_slice(rgba_chunks(fb));
        texture.update(&image);

        clear_background(Color::from_rgba(18, 18, 22, 255));
        // Zoomed view: scale the whole grid texture and offset to the view origin.
        // The panel (drawn after) hides any overflow on the right.
        draw_texture_ex(
            &texture,
            -view_x * psc,
            -view_y * psc,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(gw as f32 * psc, gh as f32 * psc)),
                ..Default::default()
            },
        );

        if !in_panel && !over_mm {
            if brush == 0 {
                // Smallest brush: a single-cell square snapped to the grid.
                let gx = (view_x + mx / psc).floor();
                let gy = (view_y + my / psc).floor();
                let sx = (gx - view_x) * psc;
                let sy = (gy - view_y) * psc;
                draw_rectangle_lines(sx, sy, psc, psc, 1.0, Color::from_rgba(255, 255, 255, 160));
            } else {
                let r = brush as f32 * psc + psc * 0.5;
                draw_circle_lines(mx, my, r, 1.0, Color::from_rgba(255, 255, 255, 120));
            }
        }

        // Minimap overview + viewport box (only useful when zoomed in).
        if zoom > 1.01 {
            draw_rectangle(
                mm_x - 2.0,
                mm_y - 2.0,
                mm_w + 4.0,
                mm_h + 4.0,
                Color::from_rgba(0, 0, 0, 200),
            );
            draw_texture_ex(
                &texture,
                mm_x,
                mm_y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(mm_w, mm_h)),
                    ..Default::default()
                },
            );
            let vx = mm_x + (view_x / gw as f32) * mm_w;
            let vy = mm_y + (view_y / gh as f32) * mm_h;
            let vw = (vis_w / gw as f32) * mm_w;
            let vh = (vis_h / gh as f32) * mm_h;
            draw_rectangle_lines(vx, vy, vw, vh, 2.0, Color::from_rgba(255, 255, 120, 230));
            draw_rectangle_lines(
                mm_x,
                mm_y,
                mm_w,
                mm_h,
                1.0,
                Color::from_rgba(120, 120, 140, 200),
            );
        }

        draw_palette(&palette, selected, &search, panel_x, palette_scroll);
        draw_hud(
            &grid,
            gw,
            gh,
            selected,
            brush,
            paused,
            heat_overlay,
            zoom,
            smooth_fps,
            smooth_tick_ms,
            &status,
            mx,
            my,
            view_x,
            view_y,
            psc,
            panel_x,
            screen_height(),
        );

        next_frame().await;
    }
}

enum PaletteItem {
    Header(String, f32),
    Mat(MaterialId, Rect),
}

/// Layout the palette in base (unscrolled) coordinates. Returns the items and
/// the total content height (for clamping the scroll offset).
fn build_palette(search: &str, x0: f32) -> (Vec<PaletteItem>, f32) {
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
                    || material::props(id)
                        .name
                        .to_ascii_lowercase()
                        .contains(search)
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
    (items, y)
}

fn draw_palette(palette: &[PaletteItem], selected: MaterialId, search: &str, x0: f32, scroll: f32) {
    const LIST_TOP: f32 = 54.0; // below the search box; items above this are clipped
    draw_rectangle(
        x0,
        0.0,
        PANEL_W,
        screen_height(),
        Color::from_rgba(28, 28, 34, 255),
    );

    for item in palette {
        match item {
            PaletteItem::Header(label, y) => {
                let dy = y - scroll;
                if dy + 15.0 >= LIST_TOP && dy < screen_height() {
                    draw_text(
                        label,
                        x0 + 8.0,
                        dy + 15.0,
                        16.0,
                        Color::from_rgba(150, 150, 165, 255),
                    );
                }
            }
            PaletteItem::Mat(id, rect) => {
                let dy = rect.y - scroll;
                if dy + rect.h < LIST_TOP || dy > screen_height() {
                    continue;
                }
                if *id == selected {
                    draw_rectangle(
                        rect.x - 2.0,
                        dy - 1.0,
                        rect.w + 4.0,
                        rect.h + 2.0,
                        Color::from_rgba(80, 80, 100, 255),
                    );
                }
                draw_rectangle(rect.x, dy, 16.0, rect.h, swatch(*id));
                draw_rectangle_lines(
                    rect.x,
                    dy,
                    16.0,
                    rect.h,
                    1.0,
                    Color::from_rgba(0, 0, 0, 180),
                );
                draw_text(
                    material::props(*id).name,
                    rect.x + 22.0,
                    dy + 14.0,
                    18.0,
                    WHITE,
                );
            }
        }
    }

    // Search box drawn last so it covers any item scrolled up under it.
    draw_rectangle(
        x0,
        0.0,
        PANEL_W,
        LIST_TOP,
        Color::from_rgba(28, 28, 34, 255),
    );
    draw_line(
        x0,
        0.0,
        x0,
        screen_height(),
        1.0,
        Color::from_rgba(60, 60, 70, 255),
    );
    draw_text("search (type to filter):", x0 + 8.0, 22.0, 18.0, GRAY);
    draw_rectangle(
        x0 + 8.0,
        28.0,
        PANEL_W - 16.0,
        22.0,
        Color::from_rgba(12, 12, 16, 255),
    );
    let shown = if search.is_empty() { "_" } else { search };
    draw_text(shown, x0 + 12.0, 44.0, 18.0, WHITE);
}

#[allow(clippy::too_many_arguments)]
fn draw_hud(
    grid: &Grid,
    gw: usize,
    gh: usize,
    selected: MaterialId,
    brush: usize,
    paused: bool,
    heat_overlay: bool,
    zoom: f32,
    fps: f32,
    tick_ms: f32,
    status: &str,
    mx: f32,
    my: f32,
    view_x: f32,
    view_y: f32,
    psc: f32,
    sim_w: f32,
    sim_h: f32,
) {
    let line = |i: f32, s: &str| draw_text(s, 8.0, 18.0 + i * 18.0, 18.0, WHITE);

    draw_rectangle(0.0, 0.0, 380.0, 96.0, Color::from_rgba(0, 0, 0, 110));
    line(
        0.0,
        &format!(
            "rust-cells  {}x{}  {:.0} fps{}",
            gw,
            gh,
            fps,
            if zoom > 1.01 {
                format!("  {zoom:.1}x")
            } else {
                String::new()
            }
        ),
    );
    line(
        1.0,
        &format!(
            "tick {:.2} ms  {}{}",
            tick_ms,
            if paused { "PAUSED" } else { "running" },
            if heat_overlay { "  [HEAT]" } else { "" }
        ),
    );
    line(
        2.0,
        &format!("brush {}  sel: {}", brush, material::props(selected).name),
    );

    if mx >= 0.0 && mx < sim_w && my >= 0.0 && my < sim_h {
        let gx = (view_x + mx / psc).floor() as i32;
        let gy = (view_y + my / psc).floor() as i32;
        if gx >= 0 && gy >= 0 && (gx as usize) < grid.width() && (gy as usize) < grid.height() {
            let (gx, gy) = (gx as usize, gy as usize);
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

    draw_text(
        "L paint  R erase  Space pause  -> step  wheel=brush  Ctrl+wheel=zoom  0 reset  mid-drag pan  F2 heat  F5/F9 save/load  F8 showcase",
        8.0,
        sim_h - 24.0,
        16.0,
        Color::from_rgba(200, 200, 210, 220),
    );
    if !status.is_empty() {
        draw_text(
            status,
            8.0,
            sim_h - 8.0,
            16.0,
            Color::from_rgba(140, 220, 140, 230),
        );
    }
}

fn draw_resize_prompt(gw: usize, gh: usize) {
    draw_rectangle(
        0.0,
        0.0,
        screen_width(),
        screen_height(),
        Color::from_rgba(0, 0, 0, 180),
    );
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
