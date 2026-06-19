//! `pwdr-app` — macroquad frontend. Owns window, input, render, UI.
//! Holds as little logic as possible; all simulation lives in `pwdr-core`.

use macroquad::prelude::*;
use pwdr_core::material::{self, EMPTY, SAND, STONE, WATER};
use pwdr_core::Grid;

const GRID_W: usize = 256;
const GRID_H: usize = 256;
const SEED: u64 = 0xC0FFEE;

/// Fixed simulation timestep (60 Hz), decoupled from render framerate.
const TICK_DT: f64 = 1.0 / 60.0;

fn window_conf() -> Conf {
    Conf {
        window_title: "pwdr".to_owned(),
        window_width: (GRID_W * 3) as i32,
        window_height: (GRID_H * 3) as i32,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut grid = Grid::new(GRID_W, GRID_H, SEED);

    // Single texture we blit the framebuffer into every frame.
    let mut image = Image::gen_image_color(GRID_W as u16, GRID_H as u16, BLACK);
    let texture = Texture2D::from_image(&image);
    texture.set_filter(FilterMode::Nearest);

    let mut acc = 0.0f64;
    let mut brush = 3usize;
    let mut selected = SAND;

    loop {
        // Material selection.
        if is_key_pressed(KeyCode::Key1) {
            selected = SAND;
        }
        if is_key_pressed(KeyCode::Key2) {
            selected = WATER;
        }
        if is_key_pressed(KeyCode::Key3) {
            selected = STONE;
        }
        // Brush size.
        if is_key_pressed(KeyCode::LeftBracket) && brush > 0 {
            brush -= 1;
        }
        if is_key_pressed(KeyCode::RightBracket) {
            brush += 1;
        }

        // Brush: left paints selected, right erases. Map screen -> grid coords.
        if is_mouse_button_down(MouseButton::Left) || is_mouse_button_down(MouseButton::Right) {
            let (mx, my) = mouse_position();
            let gx = (mx / screen_width() * GRID_W as f32) as i32;
            let gy = (my / screen_height() * GRID_H as f32) as i32;
            if gx >= 0 && gy >= 0 && (gx as usize) < GRID_W && (gy as usize) < GRID_H {
                let mat = if is_mouse_button_down(MouseButton::Right) {
                    EMPTY
                } else {
                    selected
                };
                grid.paint(gx as usize, gy as usize, brush, mat);
            }
        }

        // Fixed-timestep sim, decoupled from render.
        acc += get_frame_time() as f64;
        while acc >= TICK_DT {
            grid.step();
            acc -= TICK_DT;
        }

        // Blit core framebuffer into the texture.
        let fb = grid.render_rgba();
        image.get_image_data_mut().copy_from_slice(bytemuck_rgba(fb));
        texture.update(&image);

        clear_background(DARKGRAY);
        draw_texture_ex(
            &texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(screen_width(), screen_height())),
                ..Default::default()
            },
        );

        draw_text(
            &format!(
                "pwdr  {}x{}  {:.0} fps  [{}] brush {}",
                GRID_W,
                GRID_H,
                get_fps(),
                material::props(selected).name,
                brush
            ),
            8.0,
            20.0,
            24.0,
            WHITE,
        );

        next_frame().await;
    }
}

/// Reinterpret a flat RGBA `[u8]` as `[Color]`-sized chunks for `Image`.
/// macroquad's `Image` stores RGBA8 contiguously, so this is a straight copy.
fn bytemuck_rgba(fb: &[u8]) -> &[[u8; 4]] {
    // SAFETY: fb.len() is a multiple of 4 (RGBA), and [u8;4] has the same
    // alignment (1) as u8, so this reinterpretation is sound.
    unsafe {
        std::slice::from_raw_parts(fb.as_ptr() as *const [u8; 4], fb.len() / 4)
    }
}
