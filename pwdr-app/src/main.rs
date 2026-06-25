//! `pwdr-app` — macroquad frontend. Owns window, input, render, UI.
//! All simulation logic lives in `pwdr-core`; this file only draws it and feeds
//! it user edits.

// Release builds are a GUI app: no console window. Debug keeps the console for logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use macroquad::prelude::*;
use pwdr_core::material::{self, Category, MaterialId, Phase, EMPTY};
use pwdr_core::Grid;
use std::time::Instant;

const SCALE: f32 = 3.0; // screen pixels per cell
const PANEL_W: f32 = 248.0;
const SEED: u64 = 0xC0FFEE;
const MAX_BRUSH: usize = 64;
/// Discrete simulation-speed multipliers, selected with `,` / `.`.
const SPEEDS: [f32; 6] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0];
/// How many undo snapshots to keep (each is a serialized grid).
const UNDO_CAP: usize = 24;
/// Height of the bottom control bar, reserved below the canvas.
const BAR_H: f32 = 26.0;
/// Y where the scrollable element list begins (below the panel header).
const PANEL_LIST_TOP: f32 = 96.0;

// --- theme (a calm dark slate with a warm amber accent) ---
fn col(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba(r, g, b, 255)
}
fn cola(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color::from_rgba(r, g, b, a)
}
fn c_bg() -> Color {
    col(15, 16, 21)
}
fn c_panel() -> Color {
    col(23, 25, 32)
}
fn c_header() -> Color {
    col(17, 18, 24)
}
fn c_accent() -> Color {
    col(240, 172, 76)
}
fn c_text() -> Color {
    col(223, 226, 233)
}
fn c_muted() -> Color {
    col(132, 137, 152)
}
fn c_row_sel() -> Color {
    col(40, 43, 55)
}
fn c_hover() -> Color {
    col(32, 34, 44)
}

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
        window_title: format!("rust-cells v{}", env!("CARGO_PKG_VERSION")),
        // Default canvas: 400 wide x 250 tall cells (plus the side panel).
        window_width: (400.0 * SCALE + PANEL_W) as i32,
        window_height: (250.0 * SCALE + BAR_H) as i32,
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

const CATEGORY_ORDER: [(Category, &str); 8] = [
    (Category::Earth, "EARTH & SOLIDS"),
    (Category::Liquid, "LIQUIDS"),
    (Category::Gas, "GASES"),
    (Category::Fire, "FIRE & HEAT"),
    (Category::Electronic, "ELECTRONICS"),
    (Category::Explosive, "EXPLOSIVES"),
    (Category::Life, "LIFE"),
    (Category::Tool, "TOOLS"),
];

fn swatch(id: MaterialId) -> Color {
    let c = material::props(id).color;
    Color::from_rgba(c[0], c[1], c[2], 255)
}

/// A small phase mark, drawn from primitives (the bitmap font has no symbols):
/// solid = block, powder = grains, liquid = droplet, gas = bubbles, life =
/// diamond, energy = spark. Centered on `(cx, cy)`, ~12px.
fn draw_phase_glyph(cx: f32, cy: f32, phase: Phase) {
    let c = col(150, 156, 172);
    match phase {
        Phase::Solid => draw_rectangle(cx - 4.0, cy - 4.0, 8.0, 8.0, c),
        Phase::Powder => {
            draw_circle(cx, cy - 2.6, 1.7, c);
            draw_circle(cx - 3.0, cy + 2.2, 1.7, c);
            draw_circle(cx + 3.0, cy + 2.2, 1.7, c);
        }
        Phase::Liquid => {
            draw_triangle(
                vec2(cx, cy - 5.0),
                vec2(cx - 3.6, cy + 1.0),
                vec2(cx + 3.6, cy + 1.0),
                c,
            );
            draw_circle(cx, cy + 1.6, 3.4, c);
        }
        Phase::Gas => {
            draw_circle_lines(cx - 2.8, cy + 2.0, 2.0, 1.0, c);
            draw_circle_lines(cx + 2.6, cy + 0.8, 1.7, 1.0, c);
            draw_circle_lines(cx - 0.4, cy - 3.0, 1.8, 1.0, c);
        }
        Phase::Life => {
            draw_triangle(
                vec2(cx, cy - 5.0),
                vec2(cx - 4.0, cy),
                vec2(cx + 4.0, cy),
                c,
            );
            draw_triangle(
                vec2(cx, cy + 5.0),
                vec2(cx - 4.0, cy),
                vec2(cx + 4.0, cy),
                c,
            );
        }
        Phase::Energy => {
            draw_line(cx - 4.5, cy, cx + 4.5, cy, 1.5, c);
            draw_line(cx, cy - 4.5, cx, cy + 4.5, 1.5, c);
            draw_line(cx - 3.0, cy - 3.0, cx + 3.0, cy + 3.0, 1.0, c);
            draw_line(cx - 3.0, cy + 3.0, cx + 3.0, cy - 3.0, 1.0, c);
        }
        Phase::Empty => {}
    }
}

/// A flame mark for flammable elements.
fn draw_flame_glyph(cx: f32, cy: f32) {
    draw_triangle(
        vec2(cx, cy - 6.0),
        vec2(cx - 4.0, cy + 4.0),
        vec2(cx + 4.0, cy + 4.0),
        col(255, 140, 40),
    );
    draw_triangle(
        vec2(cx, cy - 1.0),
        vec2(cx - 2.3, cy + 4.0),
        vec2(cx + 2.3, cy + 4.0),
        col(255, 222, 96),
    );
}

/// A burst/star mark for explosive elements.
fn draw_burst_glyph(cx: f32, cy: f32) {
    let r = col(240, 72, 60);
    for k in 0..8 {
        let ang = k as f32 * std::f32::consts::FRAC_PI_4;
        let (s, c) = ang.sin_cos();
        draw_line(cx, cy, cx + c * 5.5, cy + s * 5.5, 1.6, r);
    }
    draw_circle(cx, cy, 1.9, col(255, 204, 86));
}

/// Cell grid dimensions that fit the current window (leaving room for the panel).
fn grid_dims(win_w: f32, win_h: f32) -> (usize, usize) {
    let avail_w = (win_w - PANEL_W).max(SCALE * 16.0);
    let avail_h = (win_h - BAR_H).max(SCALE * 16.0);
    let gw = (avail_w / SCALE).floor() as usize;
    let gh = (avail_h / SCALE).floor() as usize;
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
    let mut speed_idx = 2usize; // index into SPEEDS; 2 = 1.0x
    let mut undo: Vec<Vec<u8>> = Vec::new();
    let mut redo: Vec<Vec<u8>> = Vec::new();
    let mut stroke_prev: Option<(isize, isize)> = None; // last cell painted this drag
    let mut show_help = false;
    let showcase_path = desktop_path("rust-cells-showcase.save");
    let desktop_dir = showcase_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| ".".into());

    loop {
        let panel_x = screen_width() - PANEL_W;
        let canvas_h = (screen_height() - BAR_H).max(1.0); // sim area height (bar below)

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
                push_undo(&mut undo, &mut redo, &grid);
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

        // --- modifier keys (read once; used by typing guard + canvas tools) ---
        let ctrl = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl);
        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

        // --- char stream: Ctrl+letter shortcuts (undo/redo) and palette search.
        // Driven by the typed character, not a physical KeyCode, so it respects
        // the OS keyboard layout (e.g. QWERTZ, where the Z key emits KeyCode::Y). ---
        let mut undo_req = false;
        let mut redo_req = false;
        while let Some(c) = get_char_pressed() {
            let lc = c.to_ascii_lowercase();
            if ctrl {
                match lc {
                    'z' => undo_req = true,
                    'y' => redo_req = true,
                    _ => {}
                }
            } else if lc.is_ascii_alphabetic() {
                search.push(lc);
            }
        }
        if is_key_pressed(KeyCode::Backspace) {
            search.pop();
        }
        if is_key_pressed(KeyCode::Escape) {
            // Esc closes help first, then clears the search.
            if show_help {
                show_help = false;
            } else {
                search.clear();
            }
        }

        // --- controls ---
        if is_key_pressed(KeyCode::Space) {
            paused = !paused;
        }
        if is_key_pressed(KeyCode::F1) {
            show_help = !show_help;
        }
        if is_key_pressed(KeyCode::F2) {
            heat_overlay = !heat_overlay;
        }
        let mut do_single_step = false;
        if is_key_pressed(KeyCode::Right) {
            do_single_step = true;
        }
        // Simulation speed: `-` slower, `+` faster (clamped to the SPEEDS table).
        if is_key_pressed(KeyCode::Equal) || is_key_pressed(KeyCode::KpAdd) {
            speed_idx = (speed_idx + 1).min(SPEEDS.len() - 1);
        }
        if is_key_pressed(KeyCode::Minus) || is_key_pressed(KeyCode::KpSubtract) {
            speed_idx = speed_idx.saturating_sub(1);
        }
        let speed = SPEEDS[speed_idx];

        // Undo (Ctrl+Z) / Redo (Ctrl+Y), flagged from the layout-aware char stream.
        // Snapshots are full serialized grids; restoring adopts their dimensions.
        if undo_req {
            if let Some(blob) = undo.pop() {
                redo.push(grid.serialize());
                restore_grid(
                    &blob,
                    &mut grid,
                    &mut gw,
                    &mut gh,
                    &mut image,
                    &mut texture,
                    &mut win_w,
                    &mut win_h,
                    &mut resize_grace,
                );
                status = "undo".into();
            }
        }
        if redo_req {
            if let Some(blob) = redo.pop() {
                undo.push(grid.serialize());
                restore_grid(
                    &blob,
                    &mut grid,
                    &mut gw,
                    &mut gh,
                    &mut image,
                    &mut texture,
                    &mut win_w,
                    &mut win_h,
                    &mut resize_grace,
                );
                status = "redo".into();
            }
        }

        let (mx, my) = mouse_position();
        let in_panel = mx >= panel_x;

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
        // Zoom is Ctrl+wheel; `0` resets it. (`+`/`-` drive sim speed instead.)
        if is_key_pressed(KeyCode::Key0) {
            zoom = 1.0;
            view_x = 0.0;
            view_y = 0.0;
        }

        let psc = SCALE * zoom; // recompute after any zoom change
        let vis_w = panel_x / psc;
        let vis_h = canvas_h / psc;

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
            push_undo(&mut undo, &mut redo, &grid);
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
                    push_undo(&mut undo, &mut redo, &grid);
                    gw = g.width();
                    gh = g.height();
                    grid = g;
                    let (ni, nt) = make_texture(gw, gh);
                    image = ni;
                    texture = nt;
                    // Resize the window to the loaded map (skip the wipe prompt
                    // while it settles) and start PAUSED so nothing reacts until
                    // the user is ready.
                    let (tw, th) = (gw as f32 * SCALE + PANEL_W, gh as f32 * SCALE + BAR_H);
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

        // --- palette hover (tooltip) + click-to-select ---
        let mut hovered_mat: Option<MaterialId> = None;
        if in_panel {
            for entry in &palette {
                if let PaletteItem::Mat(id, rect) = entry {
                    // rects are in base (unscrolled) coords; shift the test point.
                    if rect.contains(vec2(mx, my + palette_scroll)) {
                        hovered_mat = Some(*id);
                        if is_mouse_button_pressed(MouseButton::Left) {
                            selected = *id;
                        }
                    }
                }
            }
        }

        // --- canvas tools: eyedropper / flood-fill / brush stroke ---
        let on_canvas = !in_panel && !over_mm && my < canvas_h;
        let cell = (
            (view_x + mx / psc).floor() as isize,
            (view_y + my / psc).floor() as isize,
        );
        let in_grid =
            cell.0 >= 0 && cell.1 >= 0 && (cell.0 as usize) < gw && (cell.1 as usize) < gh;
        if on_canvas && in_grid {
            let (gx, gy) = (cell.0 as usize, cell.1 as usize);
            let lp = is_mouse_button_pressed(MouseButton::Left);
            let rp = is_mouse_button_pressed(MouseButton::Right);
            let mp = is_mouse_button_pressed(MouseButton::Middle);
            let ld = is_mouse_button_down(MouseButton::Left);
            let rd = is_mouse_button_down(MouseButton::Right);

            if mp {
                // Eyedropper: middle-click picks the material under the cursor.
                // (A middle-drag still pans; a plain click just picks.)
                let m = grid.material_at(gx, gy);
                if material::user_paintable(m) {
                    selected = m;
                    status = format!("picked {}", material::props(m).name);
                }
            // Flood fill (Ctrl + click) disabled for now: too easy to trigger by
            // accident. Core `Grid::flood_fill` stays; re-enable by restoring this.
            // } else if ctrl && (lp || rp) {
            //     push_undo(&mut undo, &mut redo, &grid);
            //     grid.flood_fill(gx, gy, if rp { EMPTY } else { selected });
            //     status = "filled".into();
            } else if (ld || rd) && !ctrl {
                // Continuous brush stroke. Snapshot once at the start of a drag,
                // then paint a line from the previous cell so fast drags leave no
                // gaps. Left paints (Shift = overwrite matter); Right erases.
                if lp || rp {
                    push_undo(&mut undo, &mut redo, &grid);
                    stroke_prev = None;
                }
                let mat = if rd { EMPTY } else { selected };
                let overwrite = shift && !rd;
                let (px, py) = stroke_prev.unwrap_or(cell);
                grid.paint_line(px, py, cell.0, cell.1, brush, mat, overwrite);
                stroke_prev = Some(cell);
            }
        }
        if !is_mouse_button_down(MouseButton::Left) && !is_mouse_button_down(MouseButton::Right) {
            stroke_prev = None; // drag ended; next press starts a fresh stroke
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
            // Sim speed scales the timestep: slow-mo lengthens dt (fewer steps),
            // fast-forward shortens it (more steps, with a higher substep cap).
            let dt = (TICK_DT / speed as f64).max(1.0 / 480.0);
            let cap = ((8.0 * speed).ceil() as i32).clamp(1, 64);
            acc += get_frame_time() as f64;
            let t = Instant::now();
            let mut n = 0;
            while acc >= dt && n < cap {
                grid.step();
                acc -= dt;
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

        clear_background(c_bg());
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

        if !in_panel && !over_mm && my < canvas_h {
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

        draw_palette(&palette, selected, &search, panel_x, palette_scroll, mx, my);
        draw_hud(
            &grid,
            gw,
            gh,
            selected,
            brush,
            paused,
            heat_overlay,
            speed,
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
            canvas_h,
        );
        // Tooltip for the hovered palette row (drawn over the panel).
        if let Some(id) = hovered_mat {
            draw_tooltip(id, panel_x, my);
        }
        // Help overlay on top of everything.
        if show_help {
            draw_help();
        }

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
    let pad = 10.0;
    let row_h = 23.0;
    let mut y = PANEL_LIST_TOP + 6.0;
    for (cat, label) in CATEGORY_ORDER {
        let mats: Vec<MaterialId> = (1..material::MATERIALS.len() as MaterialId)
            .filter(|&id| material::user_paintable(id))
            .filter(|&id| material::category(id) == cat)
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

#[allow(clippy::too_many_arguments)]
fn draw_palette(
    palette: &[PaletteItem],
    selected: MaterialId,
    search: &str,
    x0: f32,
    scroll: f32,
    mx: f32,
    my: f32,
) {
    let h = screen_height();
    draw_rectangle(x0, 0.0, PANEL_W, h, c_panel());

    for item in palette {
        match item {
            PaletteItem::Header(label, y) => {
                let dy = y - scroll;
                if dy + 16.0 >= PANEL_LIST_TOP && dy < h {
                    draw_text(label, x0 + 12.0, dy + 15.0, 15.0, c_accent());
                }
            }
            PaletteItem::Mat(id, rect) => {
                let dy = rect.y - scroll;
                if dy + rect.h < PANEL_LIST_TOP || dy > h {
                    continue;
                }
                let hovered = mx >= x0 && my >= dy && my < dy + rect.h;
                if *id == selected {
                    draw_rectangle(x0, dy - 1.0, PANEL_W, rect.h + 2.0, c_row_sel());
                    draw_rectangle(x0, dy - 1.0, 3.0, rect.h + 2.0, c_accent());
                } else if hovered {
                    draw_rectangle(x0, dy - 1.0, PANEL_W, rect.h + 2.0, c_hover());
                }
                // swatch chip
                draw_rectangle(rect.x, dy + 2.0, 16.0, rect.h - 4.0, swatch(*id));
                draw_rectangle_lines(
                    rect.x,
                    dy + 2.0,
                    16.0,
                    rect.h - 4.0,
                    1.0,
                    cola(0, 0, 0, 150),
                );
                let tcol = if *id == selected {
                    c_text()
                } else {
                    col(200, 204, 214)
                };
                draw_text(
                    material::props(*id).name,
                    rect.x + 24.0,
                    dy + 15.0,
                    17.0,
                    tcol,
                );
                // Right-aligned markers: a phase glyph (powder / liquid / gas /
                // solid / …) then a hazard glyph (flammable / explosive).
                let gy = dy + rect.h * 0.5;
                let phase_x = x0 + PANEL_W - 13.0;
                draw_phase_glyph(phase_x, gy, material::phase(*id));
                let haz_x = phase_x - 18.0;
                if material::is_explosive(*id) {
                    draw_burst_glyph(haz_x, gy);
                } else if material::is_flammable(*id) {
                    draw_flame_glyph(haz_x, gy);
                }
            }
        }
    }

    // Header (title + search) drawn last so it covers items scrolled under it.
    draw_rectangle(x0, 0.0, PANEL_W, PANEL_LIST_TOP, c_header());
    draw_rectangle(x0, 0.0, PANEL_W, 3.0, c_accent()); // top accent strip
    draw_line(x0, 0.0, x0, h, 1.0, col(48, 51, 63));
    draw_line(
        x0,
        PANEL_LIST_TOP,
        x0 + PANEL_W,
        PANEL_LIST_TOP,
        1.0,
        col(48, 51, 63),
    );

    draw_text("rust-cells", x0 + 12.0, 30.0, 26.0, c_text());
    draw_text("powder sandbox", x0 + 13.0, 47.0, 14.0, c_muted());

    // search field
    draw_rectangle(x0 + 12.0, 60.0, PANEL_W - 24.0, 26.0, col(12, 13, 18));
    let border = if search.is_empty() {
        col(48, 51, 63)
    } else {
        c_accent()
    };
    draw_rectangle_lines(x0 + 12.0, 60.0, PANEL_W - 24.0, 26.0, 1.0, border);
    if search.is_empty() {
        draw_text("type to search", x0 + 18.0, 78.0, 16.0, c_muted());
    } else {
        draw_text(search, x0 + 18.0, 78.0, 16.0, c_text());
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
    heat_overlay: bool,
    speed: f32,
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
    // --- top-left status card ---
    let (cx, cy, cw, ch) = (10.0, 10.0, 232.0, 82.0);
    draw_rectangle(cx, cy, cw, ch, cola(14, 15, 20, 220));
    draw_rectangle(cx, cy, 3.0, ch, c_accent());
    draw_rectangle_lines(cx, cy, cw, ch, 1.0, cola(70, 74, 90, 200));

    let tx = cx + 12.0;
    draw_text("rust-cells", tx, cy + 22.0, 22.0, c_text());
    let state = if paused {
        "PAUSED".to_string()
    } else if (speed - 1.0).abs() < 0.01 {
        "LIVE".to_string()
    } else {
        format!("{speed}x")
    };
    let state_col = if paused {
        c_accent()
    } else {
        col(120, 210, 140)
    };
    draw_text(&state, cx + cw - 64.0, cy + 20.0, 18.0, state_col);

    draw_text(
        &format!(
            "{}x{}   {:.0} fps   {:.2} ms{}",
            gw,
            gh,
            fps,
            tick_ms,
            if zoom > 1.01 {
                format!("   {zoom:.1}x")
            } else {
                String::new()
            }
        ),
        tx,
        cy + 42.0,
        15.0,
        c_muted(),
    );

    // selected element + a small swatch chip
    draw_rectangle(tx, cy + 50.0, 12.0, 12.0, swatch(selected));
    draw_rectangle_lines(tx, cy + 50.0, 12.0, 12.0, 1.0, cola(0, 0, 0, 150));
    draw_text(
        &format!(
            "{}   brush {}{}",
            material::props(selected).name,
            brush,
            if heat_overlay { "   HEAT" } else { "" }
        ),
        tx + 18.0,
        cy + 60.0,
        15.0,
        c_text(),
    );

    // cursor readout (under the card)
    if mx >= 0.0 && mx < sim_w && my >= 0.0 && my < sim_h {
        let gx = (view_x + mx / psc).floor() as i32;
        let gy = (view_y + my / psc).floor() as i32;
        if gx >= 0 && gy >= 0 && (gx as usize) < grid.width() && (gy as usize) < grid.height() {
            let (gx, gy) = (gx as usize, gy as usize);
            let m = grid.material_at(gx, gy);
            draw_text(
                &format!(
                    "({}, {})  {}  {:.0}C",
                    gx,
                    gy,
                    material::props(m).name,
                    grid.temperature_at(gx, gy)
                ),
                tx,
                cy + ch + 18.0,
                15.0,
                c_muted(),
            );
        }
    }

    // --- bottom control bar (reserved strip below the canvas, not over it) ---
    draw_rectangle(0.0, sim_h, sim_w, BAR_H, c_header());
    draw_line(0.0, sim_h, sim_w, sim_h, 1.0, cola(70, 74, 90, 200));
    draw_text(
        "L paint  R erase  Shift overwrite  Mid-click pick  wheel brush  +/- speed  Ctrl+Z/Y undo  Space pause  F1 help",
        10.0,
        sim_h + 17.0,
        15.0,
        c_muted(),
    );
    if !status.is_empty() {
        let sx = sim_w - 230.0;
        draw_text(status, sx, sim_h + 17.0, 15.0, col(150, 210, 150));
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

/// Push the current grid onto the undo stack (capped) and clear the redo stack.
/// Called before any destructive edit (stroke start, fill, clear, resize, load).
fn push_undo(undo: &mut Vec<Vec<u8>>, redo: &mut Vec<Vec<u8>>, grid: &Grid) {
    undo.push(grid.serialize());
    if undo.len() > UNDO_CAP {
        undo.remove(0);
    }
    redo.clear();
}

/// Replace `grid` (and its texture/dimensions) from a serialized snapshot. If
/// the snapshot's size differs from the current canvas, the window is resized to
/// match, mirroring the load path.
#[allow(clippy::too_many_arguments)]
fn restore_grid(
    blob: &[u8],
    grid: &mut Grid,
    gw: &mut usize,
    gh: &mut usize,
    image: &mut Image,
    texture: &mut Texture2D,
    win_w: &mut f32,
    win_h: &mut f32,
    resize_grace: &mut i32,
) {
    if let Some(g) = Grid::deserialize(blob) {
        let (nw, nh) = (g.width(), g.height());
        let dims_changed = nw != *gw || nh != *gh;
        *gw = nw;
        *gh = nh;
        *grid = g;
        let (ni, nt) = make_texture(nw, nh);
        *image = ni;
        *texture = nt;
        if dims_changed {
            let (tw, th) = (nw as f32 * SCALE + PANEL_W, nh as f32 * SCALE + BAR_H);
            request_new_screen_size(tw, th);
            *win_w = tw;
            *win_h = th;
            *resize_grace = 20;
        }
    }
}

/// Greedy word-wrap to at most `max_chars` per line (the bitmap font is roughly
/// monospaced at the sizes we draw, so a char budget is good enough).
fn wrap(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if cur.is_empty() {
            cur = word.to_string();
        } else if cur.len() + 1 + word.len() <= max_chars {
            cur.push(' ');
            cur.push_str(word);
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// A hover tooltip for a palette element: name, phase + density, the one-line
/// blurb, and a hazard tag. Drawn to the left of the panel so it never overlaps
/// the list it describes.
fn draw_tooltip(id: MaterialId, panel_x: f32, my: f32) {
    let p = material::props(id);
    let phase = match material::phase(id) {
        Phase::Powder => "Powder",
        Phase::Liquid => "Liquid",
        Phase::Gas => "Gas",
        Phase::Solid => "Solid",
        Phase::Energy => "Energy",
        Phase::Life => "Critter",
        Phase::Empty => "-",
    };
    let lines = wrap(material::blurb(id), 34);
    let haz = if material::is_explosive(id) {
        "Explosive"
    } else if material::is_flammable(id) {
        "Flammable"
    } else {
        ""
    };
    let line_h = 17.0;
    let w = 244.0;
    let h = 56.0 + lines.len() as f32 * line_h + if haz.is_empty() { 0.0 } else { line_h } + 10.0;
    let x = (panel_x - w - 10.0).max(8.0);
    let y = my.min(screen_height() - h - 8.0).max(8.0);
    draw_rectangle(x, y, w, h, cola(18, 19, 26, 244));
    draw_rectangle(x, y, 3.0, h, c_accent());
    draw_rectangle_lines(x, y, w, h, 1.0, cola(80, 84, 100, 220));
    let tx = x + 12.0;
    draw_rectangle(tx, y + 12.0, 12.0, 12.0, swatch(id));
    draw_rectangle_lines(tx, y + 12.0, 12.0, 12.0, 1.0, cola(0, 0, 0, 150));
    draw_text(p.name, tx + 20.0, y + 23.0, 19.0, c_text());
    draw_text(
        &format!("{phase}   density {}", p.density),
        tx,
        y + 42.0,
        14.0,
        c_muted(),
    );
    let mut yy = y + 62.0;
    for ln in &lines {
        draw_text(ln, tx, yy, 15.0, col(202, 206, 216));
        yy += line_h;
    }
    if !haz.is_empty() {
        draw_text(haz, tx, yy, 14.0, col(240, 150, 90));
    }
}

/// Full-screen controls + interaction cheat-sheet, toggled with F1.
fn draw_help() {
    draw_rectangle(
        0.0,
        0.0,
        screen_width(),
        screen_height(),
        cola(0, 0, 0, 180),
    );
    let (w, h) = (564.0, 428.0);
    let (x, y) = ((screen_width() - w) * 0.5, (screen_height() - h) * 0.5);
    draw_rectangle(x, y, w, h, col(24, 26, 33));
    draw_rectangle(x, y, w, 3.0, c_accent());
    draw_rectangle_lines(x, y, w, h, 1.0, cola(90, 94, 110, 230));
    draw_text("rust-cells - controls", x + 20.0, y + 34.0, 24.0, c_text());
    let rows = [
        ("Left drag", "paint selected element (into empty)"),
        ("Shift + left", "overwrite existing matter"),
        ("Right drag", "erase"),
        ("Middle click", "eyedropper - pick element under cursor"),
        ("Mouse wheel", "brush size   (Ctrl+wheel = zoom)"),
        ("+ / -", "faster / slower simulation"),
        ("Ctrl+Z / Ctrl+Y", "undo / redo"),
        ("Space / Right", "pause / single step"),
        ("0", "reset zoom"),
        ("Middle drag, minimap", "pan the view"),
        ("F2", "temperature overlay"),
        ("F5 / F9 / F8", "save / load / load showcase"),
        ("Del", "clear canvas"),
        ("type, click swatch", "filter palette, select element"),
    ];
    let mut yy = y + 64.0;
    for (k, d) in rows {
        draw_text(k, x + 20.0, yy, 16.0, c_accent());
        draw_text(d, x + 214.0, yy, 16.0, col(205, 209, 219));
        yy += 22.0;
    }
    draw_text(
        "Try: oil on water then a spark, salt on ice, lava into water, wire battery-fuse-TNT.",
        x + 20.0,
        yy + 10.0,
        14.0,
        c_muted(),
    );
    draw_text(
        "F1 or Esc to close",
        x + 20.0,
        y + h - 16.0,
        15.0,
        c_muted(),
    );
}

/// Reinterpret a flat RGBA `[u8]` as `[[u8;4]]` for macroquad's `Image`.
fn rgba_chunks(fb: &[u8]) -> &[[u8; 4]] {
    // SAFETY: fb.len() is a multiple of 4 (RGBA) and [u8;4] has alignment 1.
    unsafe { std::slice::from_raw_parts(fb.as_ptr() as *const [u8; 4], fb.len() / 4) }
}
