//! Build script: on Windows, procedurally generate the app icon (the same
//! falling-sand motif the window uses) and embed it into the executable so it
//! shows up in Explorer, the taskbar, and shortcuts.

fn main() {
    #[cfg(windows)]
    embed_icon();
}

#[cfg(windows)]
fn embed_icon() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let ico_path = std::path::Path::new(&out_dir).join("icon.ico");
    std::fs::write(&ico_path, build_ico(&[16, 32, 48, 64])).expect("write icon.ico");

    let mut res = winresource::WindowsResource::new();
    res.set_icon(ico_path.to_str().unwrap());
    res.compile().expect("embed windows resource");
}

/// One icon pixel (normalized coords), RGBA. Mirrors the window icon.
#[cfg(windows)]
fn icon_pixel(nx: f32, ny: f32) -> [u8; 4] {
    let sand = [201, 182, 112, 255];
    let water = [42, 96, 205, 255];
    let fire = [255, 140, 42, 255];
    let bg = [24, 24, 38, 255];

    let (dx, dy) = (nx - 0.74, ny - 0.24);
    if dx * dx + dy * dy < 0.018 {
        return fire;
    }
    if (0.47..0.55).contains(&nx) && (0.26..0.60).contains(&ny) && ((ny * 22.0) as i32 % 3 != 0) {
        return sand;
    }
    if nx < 0.40 && ny > 0.64 {
        return water;
    }
    let surface = 0.66 - (0.18 - (nx - 0.60).abs()).max(0.0);
    if ny > surface && nx > 0.36 {
        return sand;
    }
    bg
}

/// Build a 32bpp BMP image (DIB) for one icon size: BITMAPINFOHEADER + bottom-up
/// BGRA pixels + a (zeroed) AND mask, as required by the ICO format.
#[cfg(windows)]
fn bmp_dib(size: u32) -> Vec<u8> {
    let s = size as i32;
    let mut v = Vec::new();
    // BITMAPINFOHEADER (40 bytes). Height is doubled (XOR image + AND mask).
    v.extend_from_slice(&40u32.to_le_bytes());
    v.extend_from_slice(&s.to_le_bytes());
    v.extend_from_slice(&(s * 2).to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes()); // planes
    v.extend_from_slice(&32u16.to_le_bytes()); // bpp
    v.extend_from_slice(&[0u8; 24]); // compression..clrimportant

    // XOR pixels, bottom-up, BGRA.
    for y in (0..size).rev() {
        for x in 0..size {
            let nx = (x as f32 + 0.5) / size as f32;
            let ny = (y as f32 + 0.5) / size as f32;
            let [r, g, b, a] = icon_pixel(nx, ny);
            v.extend_from_slice(&[b, g, r, a]);
        }
    }
    // AND mask: 1bpp, rows padded to 4 bytes; all zero (alpha handles opacity).
    let row = ((size + 31) / 32) * 4;
    v.extend(std::iter::repeat_n(0u8, (row * size) as usize));
    v
}

/// Assemble an .ico from several sizes.
#[cfg(windows)]
fn build_ico(sizes: &[u32]) -> Vec<u8> {
    let mut images: Vec<Vec<u8>> = sizes.iter().map(|&s| bmp_dib(s)).collect();

    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());

    let mut offset = 6 + 16 * sizes.len();
    for (&s, img) in sizes.iter().zip(&images) {
        let dim = if s >= 256 { 0u8 } else { s as u8 };
        out.push(dim); // width
        out.push(dim); // height
        out.push(0); // palette count
        out.push(0); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bpp
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += img.len();
    }
    for img in images.drain(..) {
        out.extend_from_slice(&img);
    }
    out
}
