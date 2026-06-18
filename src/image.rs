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

pub struct ImageSize {
    pub cell_h: u16,
}

/// Measure how many rows a PNG will occupy when displayed at `cell_w` columns.
/// Accounts for terminal cells being roughly twice as tall as wide.
pub fn measure_image(png_bytes: &[u8], cell_w: u16) -> Result<ImageSize, String> {
    let img = image::ImageReader::new(Cursor::new(png_bytes))
        .with_guessed_format()
        .map_err(|e| format!("decode: {e}"))?
        .decode()
        .map_err(|e| format!("decode: {e}"))?;

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
            write!(
                out,
                "\x1b_Ga=T,C=1,q=1,f=100,c={cell_w},r={cell_h},i={id},m={more};{data}\x1b\\"
            )
            .map_err(|e| format!("header: {e}"))?;
        } else {
            write!(out, "\x1b_Gm={more};{data}\x1b\\").map_err(|e| format!("chunk: {e}"))?;
        }
    }
    out.flush().map_err(|e| format!("flush: {e}"))?;

    Ok(())
}

// ── Protocol detection ──────────────────────────────

#[derive(Debug)]
pub enum Protocol {
    Kitty,
    Sixel,
}

/// 检测终端支持的图形协议。
/// 可通过 `MIKUJI_PROTOCOL` 环境变量覆盖：`kitty` / `sixel` / `none`。
/// 编译时可通过 `--features force-sixel` 强制使用 Sixel。
pub fn detect_protocol() -> Option<Protocol> {
    // 编译时 feature 优先级最高
    #[cfg(feature = "force-sixel")]
    {
        return Some(Protocol::Sixel);
    }

    #[cfg(not(feature = "force-sixel"))]
    {
        if let Ok(p) = std::env::var("MIKUJI_PROTOCOL") {
            return match p.to_lowercase().as_str() {
                "kitty" => Some(Protocol::Kitty),
                "sixel" => Some(Protocol::Sixel),
                "none" | "off" | "0" => None,
                _ => None,
            };
        }

        let term = std::env::var("TERM").unwrap_or_default();
        if term.contains("kitty") {
            return Some(Protocol::Kitty);
        }
        if term.contains("ms-terminal") {
            return Some(Protocol::Sixel);
        }

        let term_program = std::env::var("TERM_PROGRAM").unwrap_or_default();
        if term_program.contains("Windows Terminal") {
            return Some(Protocol::Sixel);
        }

        if std::env::var("KITTY_WINDOW_ID").is_ok() {
            return Some(Protocol::Kitty);
        }

        // WSL 下默认走 Sixel（Windows Terminal 支持）
        if std::path::Path::new("/proc/sys/fs/binfmt_misc/WSLInterop").exists() {
            return Some(Protocol::Sixel);
        }

        Some(Protocol::Kitty)
    }
}

// ── Sixel protocol ──────────────────────────────────

/// 通过 Sixel 协议显示 PNG。
pub fn sixel_emit(png_bytes: &[u8], cell_w: u16, _cell_h: u16) -> Result<(), String> {
    let img = image::ImageReader::new(Cursor::new(png_bytes))
        .with_guessed_format()
        .map_err(|e| format!("decode: {e}"))?
        .decode()
        .map_err(|e| format!("decode: {e}"))?;

    let rgba = img.to_rgba8();
    let (iw, ih) = rgba.dimensions();

    // 缩放到目标像素宽（scale 需要匹配终端实际的单元格像素宽度）
    // Windows Terminal 通常是 8-12 像素/列，具体取决于字体和 DPI
    // 可通过环境变量 MIKUJI_SIXEL_SCALE 覆盖（例如：export MIKUJI_SIXEL_SCALE=12）
    let scale = std::env::var("MIKUJI_SIXEL_SCALE")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(10);
    let sw = cell_w as u32 * scale;
    // 终端字符高宽比约为 2:1，需要除以 2 来补偿
    let sh = ((ih as f64 / iw as f64) * sw as f64 / 2.0) as u32;

    let resized = image::imageops::resize(&rgba, sw, sh, image::imageops::FilterType::Lanczos3);

    // 构建全局调色板（避免 band 间颜色不一致导致条纹）
    // 使用更细致的量化：RGB 各 8 级（512 色）
    let q = |c: u8| -> u8 { ((c as u32 * 7 + 127) / 255 * 255 / 7) as u8 };

    let mut palette: Vec<(u8, u8, u8)> = Vec::new();
    let mut color_map = std::collections::HashMap::new();

    // 白色作为背景色（透明区域）
    let white = (255u8, 255u8, 255u8);
    palette.push(white);
    color_map.insert(white, 0);

    // 扫描整个图像建立全局调色板
    for y in 0..sh {
        for x in 0..sw {
            let p = resized.get_pixel(x, y);
            let key = if p[3] < 128 {
                white
            } else {
                (q(p[0]), q(p[1]), q(p[2]))
            };
            if !color_map.contains_key(&key) && palette.len() < 256 {
                color_map.insert(key, palette.len());
                palette.push(key);
            }
        }
    }

    let mut out = std::io::stdout().lock();
    // Sixel 序列开始：P 后跟参数，q 表示 Sixel 模式
    // 参数 0;1: 第二个参数=1表示保持背景透明（不覆盖原有内容）
    write!(out, "\x1bP0;1q").map_err(|e| format!("sixel start: {e}"))?;

    // 定义全局颜色寄存器
    for (i, &(r, g, b)) in palette.iter().enumerate() {
        let rp = (r as u32 * 100 / 255) as u8;
        let gp = (g as u32 * 100 / 255) as u8;
        let bp = (b as u32 * 100 / 255) as u8;
        write!(out, "#{};2;{};{};{}", i, rp, gp, bp).map_err(|e| format!("sixel palette: {e}"))?;
    }

    // 按 6 像素高度的 band 输出
    for band_y in (0..sh).step_by(6) {
        // 每个颜色的 sixel 数据
        for (ci, &color) in palette.iter().enumerate() {
            let mut has_data = false;
            let mut run_len = 0u32;
            let mut last_byte = 0u8;

            for x in 0..sw {
                let mut byte: u8 = 0;
                for dy in 0..6 {
                    let y = band_y + dy;
                    if y >= sh {
                        break;
                    }
                    let p = resized.get_pixel(x, y);
                    let pixel_color = if p[3] < 128 {
                        white
                    } else {
                        (q(p[0]), q(p[1]), q(p[2]))
                    };
                    if pixel_color == color {
                        byte |= 1 << dy;
                    }
                }

                // Run-length encoding：连续相同的 sixel 字符可压缩
                if byte == last_byte && run_len > 0 {
                    run_len += 1;
                } else {
                    if run_len > 0 {
                        if !has_data {
                            write!(out, "#{}", ci).map_err(|e| format!("sixel color: {e}"))?;
                            has_data = true;
                        }
                        if run_len >= 3 {
                            write!(out, "!{}{}", run_len, (last_byte + 63) as char)
                                .map_err(|e| format!("sixel rle: {e}"))?;
                        } else {
                            for _ in 0..run_len {
                                write!(out, "{}", (last_byte + 63) as char)
                                    .map_err(|e| format!("sixel char: {e}"))?;
                            }
                        }
                    }
                    last_byte = byte;
                    run_len = 1;
                }
            }

            // 输出最后一个 run
            if run_len > 0 {
                if !has_data {
                    write!(out, "#{}", ci).map_err(|e| format!("sixel color: {e}"))?;
                }
                if run_len >= 3 {
                    write!(out, "!{}{}", run_len, (last_byte + 63) as char)
                        .map_err(|e| format!("sixel rle: {e}"))?;
                } else {
                    for _ in 0..run_len {
                        write!(out, "{}", (last_byte + 63) as char)
                            .map_err(|e| format!("sixel char: {e}"))?;
                    }
                }
            }

            // 回到行首准备下一个颜色
            if has_data && ci + 1 < palette.len() {
                write!(out, "$").map_err(|e| format!("sixel cr: {e}"))?;
            }
        }

        // 下一个 band（换行）
        if band_y + 6 < sh {
            write!(out, "-").map_err(|e| format!("sixel lf: {e}"))?;
        }
    }

    // Sixel 结束
    write!(out, "\x1b\\").map_err(|e| format!("sixel end: {e}"))?;
    out.flush().map_err(|e| format!("sixel flush: {e}"))?;

    Ok(())
}
