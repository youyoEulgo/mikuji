use base64::Engine;
use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::path::PathBuf;

static BLOB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/image_blob.bin"));
static INDEX: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/image_index.bin"));

pub struct ImageStore {
    map: HashMap<String, &'static [u8]>,
}

impl ImageStore {
    pub fn new() -> Self {
        let mut map = HashMap::new();
        let mut c = Cursor::new(INDEX);
        for _ in 0..r32(&mut c) {
            let n = r16(&mut c) as usize;
            let name = String::from_utf8(INDEX[c.position() as usize..][..n].to_vec()).unwrap();
            c.set_position(c.position() + n as u64);
            let off = r32(&mut c) as usize;
            let len = r32(&mut c) as usize;
            map.insert(name, unsafe {
                std::mem::transmute::<&[u8], &'static [u8]>(&BLOB[off..][..len])
            });
        }
        Self { map }
    }

    pub fn load_png_bytes(&self, name: &str) -> Result<Vec<u8>, String> {
        if let Some(d) = self.map.get(name) { return Ok(d.to_vec()); }
        let path = image_path(name);
        if !path.exists() { return Err(format!("not found: {name}")); }
        std::fs::read(&path).map_err(|e| format!("{e}"))
    }
}

fn image_path(name: &str) -> PathBuf {
    if let Ok(d) = std::env::var("MIKUJI_IMAGES") { return d.into(); }
    let fname = name.replace(['·', '&'], "_") + ".png";
    if let Ok(exe) = std::env::current_exe()
        && let Some(p) = exe.parent() {
        let c = p.join("output").join("images").join(&fname);
        if c.exists() { return c; }
    }
    PathBuf::from("output/images").join(fname)
}

// ── Kitty protocol: a=T, q=1, f=100 (PNG), C=1 ──────

pub struct ImageSize { pub cell_h: u16 }

/// Transmit and display a PNG at current cursor position using Kitty Graphics Protocol.
/// Uses columns (c) and rows (r) instead of pixels to let the terminal calculate proper size.
/// q=1 suppresses terminal OK response (no stdin pollution).
/// C=1 means cursor stays in place after display.
/// f=100 means PNG format (terminal decodes it natively).
pub fn kitty_emit(png_bytes: &[u8], cell_w: u16) -> Result<ImageSize, String> {
    let img = image::ImageReader::new(Cursor::new(png_bytes))
        .with_guessed_format().map_err(|e| format!("decode: {e}"))?
        .decode().map_err(|e| format!("decode: {e}"))?;

    let (iw, ih) = (img.width(), img.height());

    // 计算行数时要考虑终端单元格的宽高比
    // 终端单元格通常是高度约为宽度的2倍
    // 所以: cell_h = (图片高/图片宽) * cell_w / (单元格高/单元格宽)
    //            = (ih/iw) * cell_w / 2.0
    let cell_h = ((ih as f64 / iw as f64) * cell_w as f64 / 2.0) as u16;

    let b64 = base64::engine::general_purpose::STANDARD.encode(png_bytes);
    let id = std::process::id() & 0x00ff_ffff;
    let mut out = std::io::stdout().lock();
    let total = b64.len().div_ceil(4096);

    for (i, chunk) in b64.as_bytes().chunks(4096).enumerate() {
        let more = u8::from(i + 1 < total);
        let data = std::str::from_utf8(chunk).unwrap();
        if i == 0 {
            // 使用列/行参数，让终端根据实际字体计算像素尺寸
            write!(out, "\x1b_Ga=T,C=1,q=1,f=100,c={cell_w},r={cell_h},i={id},m={more};{data}\x1b\\")
                .map_err(|e| format!("header: {e}"))?;
        } else {
            write!(out, "\x1b_Gm={more};{data}\x1b\\")
                .map_err(|e| format!("chunk: {e}"))?;
        }
    }
    out.flush().map_err(|e| format!("flush: {e}"))?;

    Ok(ImageSize { cell_h })
}

fn r32(c: &mut Cursor<&[u8]>) -> u32 {
    let mut b = [0u8;4]; let p=c.position() as usize;
    b.copy_from_slice(&c.get_ref()[p..p+4]); c.set_position(c.position()+4);
    u32::from_le_bytes(b)
}
fn r16(c: &mut Cursor<&[u8]>) -> u16 {
    let mut b = [0u8;2]; let p=c.position() as usize;
    b.copy_from_slice(&c.get_ref()[p..p+2]); c.set_position(c.position()+2);
    u16::from_le_bytes(b)
}
