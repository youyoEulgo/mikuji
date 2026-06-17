use base64::Engine;
use std::io::{Cursor, Write};
use std::path::PathBuf;

pub struct ImageStore;

impl ImageStore {
    pub fn new() -> Self {
        Self
    }

    pub fn load_png_bytes(&self, name: &str) -> Result<Vec<u8>, String> {
        let path = image_path(name);
        std::fs::read(&path).map_err(|e| format!("无法读取图片 {}: {}", path.display(), e))
    }
}

fn image_path(name: &str) -> PathBuf {
    if let Ok(d) = std::env::var("MIKUJI_IMAGES") {
        return PathBuf::from(d).join(name.replace(['·', '&'], "_") + ".png");
    }
    crate::paths::data_dir()
        .join("images")
        .join(name.replace(['·', '&'], "_") + ".png")
}

// ── Kitty protocol: a=T, q=1, f=100 (PNG), C=1 ──────

pub struct ImageSize { pub cell_h: u16 }

/// Measure how many rows a PNG will occupy when displayed at `cell_w` columns.
/// Accounts for terminal cells being roughly twice as tall as wide.
pub fn measure_image(png_bytes: &[u8], cell_w: u16) -> Result<ImageSize, String> {
    let img = image::ImageReader::new(Cursor::new(png_bytes))
        .with_guessed_format().map_err(|e| format!("decode: {e}"))?
        .decode().map_err(|e| format!("decode: {e}"))?;

    let (iw, ih) = (img.width(), img.height());
    // 终端单元格通常是高度约为宽度的2倍
    let cell_h = ((ih as f64 / iw as f64) * cell_w as f64 / 2.0) as u16;

    Ok(ImageSize { cell_h })
}

/// Transmit and display a PNG at current cursor position using Kitty Graphics Protocol.
/// Uses columns (c) and rows (r) parameters so the terminal calculates the proper pixel size.
/// q=1 suppresses terminal OK response (no stdin pollution).
/// C=1 means cursor stays in place after display.
/// f=100 means PNG format (terminal decodes it natively).
pub fn kitty_emit(png_bytes: &[u8], cell_w: u16, cell_h: u16) -> Result<(), String> {
    let b64 = base64::engine::general_purpose::STANDARD.encode(png_bytes);
    let id = std::process::id() & 0x00ff_ffff;
    let mut out = std::io::stdout().lock();
    let total = b64.len().div_ceil(4096);

    for (i, chunk) in b64.as_bytes().chunks(4096).enumerate() {
        let more = u8::from(i + 1 < total);
        let data = std::str::from_utf8(chunk).unwrap();
        if i == 0 {
            write!(out, "\x1b_Ga=T,C=1,q=1,f=100,c={cell_w},r={cell_h},i={id},m={more};{data}\x1b\\")
                .map_err(|e| format!("header: {e}"))?;
        } else {
            write!(out, "\x1b_Gm={more};{data}\x1b\\")
                .map_err(|e| format!("chunk: {e}"))?;
        }
    }
    out.flush().map_err(|e| format!("flush: {e}"))?;

    Ok(())
}
