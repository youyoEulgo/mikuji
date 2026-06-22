use anyhow::Context;
use base64::Engine;
use image::ImageEncoder;
use std::io::{Cursor, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct ImageStore;

impl ImageStore {
    pub fn new() -> Self {
        Self
    }

    pub(crate) fn load_png_bytes(&self, name: &str) -> Result<Vec<u8>, anyhow::Error> {
        let path = image_path(name);
        std::fs::read(&path)
            .with_context(|| format!("无法读取图片 {}", path.display()))
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

// ── Kitty 协议 ───────────────────────────────────────

/// 通过 Kitty Graphics Protocol 显示 PNG，返回占用的单元格行高。
#[must_use]
pub(crate) fn kitty_emit(png_bytes: &[u8], cell_w: u16) -> Result<u16, anyhow::Error> {
    let img = image::ImageReader::new(Cursor::new(png_bytes))
        .with_guessed_format().context("decode")?
        .decode().context("decode")?;
    let (iw, ih) = (img.width(), img.height());
    let cell_h = ((ih as f64 / iw as f64) * cell_w as f64 / cell_aspect_ratio()) as u16;

    let b64 = base64::engine::general_purpose::STANDARD.encode(png_bytes);
    let id = std::process::id() & 0x00ff_ffff;
    let mut out = std::io::stdout().lock();
    let total = b64.len().div_ceil(4096);

    for (i, chunk) in b64.as_bytes().chunks(4096).enumerate() {
        let more = u8::from(i + 1 < total);
        let data = std::str::from_utf8(chunk).expect("base64 is always ASCII");
        if i == 0 {
            write!(
                out,
                "\x1b_Ga=T,C=1,q=1,f=100,c={cell_w},r={cell_h},i={id},m={more};{data}\x1b\\"
            )
            .context("header")?;
        } else {
            write!(out, "\x1b_Gm={more};{data}\x1b\\").context("chunk")?;
        }
    }
    out.flush().context("flush")?;

    Ok(cell_h)
}

// ── iTerm2 协议 (OSC 1337) ──────────────────────────

/// 构建 iTerm2 inline-images 协议的 OSC 1337 序列，返回 (序列, 行高)。
///
/// 调用方必须用 `libc::write` 发送序列——Rust `stdout().lock()` 的 `LineWriter` 会拆断终止符。
#[must_use]
pub(crate) fn iip_emit(png_bytes: &[u8], cell_w: u16) -> Result<(String, u16), anyhow::Error> {
    use base64::Engine;
    use std::fmt::Write;

    let img = image::ImageReader::new(Cursor::new(png_bytes))
        .with_guessed_format().context("decode")?
        .decode().context("decode")?;

    let rgba = img.to_rgba8();
    let (iw, ih) = (rgba.width(), rgba.height());

    let px = px_per_col();
    let target_w = (cell_w as f64 * px) as u32;
    let target_h = ((ih as f64 / iw as f64) * target_w as f64) as u32;
    let resized = image::imageops::resize(&rgba, target_w, target_h, image::imageops::FilterType::Triangle);
    let (w, h) = (resized.width(), resized.height());

    let mut png_buf = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png_buf)
        .write_image(&resized, w, h, image::ExtendedColorType::Rgba8)
        .context("png encode")?;

    let mut seq = String::with_capacity(200 + png_buf.len() * 4 / 3);
    write!(
        seq,
        "\x1b]1337;File=inline=1;size={};width={w}px;height={h}px;doNotMoveCursor=1:",
        png_buf.len(),
    )
    .context("iip buf")?;
    base64::engine::general_purpose::STANDARD.encode_string(&png_buf, &mut seq);
    write!(seq, "\x07").context("iip term")?;

    let cell_h = (target_h as f64 / cell_aspect_ratio() / px).ceil() as u16;
    Ok((seq, cell_h.max(1)))
}

// ── Sixel 协议 ────────────────────────────────────────

/// 通过 Sixel 协议显示 PNG，返回占用的单元格行高。
#[must_use]
pub(crate) fn sixel_emit(png_bytes: &[u8], cell_w: u16) -> Result<u16, anyhow::Error> {
    let img = image::ImageReader::new(Cursor::new(png_bytes))
        .with_guessed_format().context("decode")?
        .decode().context("decode")?;

    let rgba = img.to_rgba8();
    let (iw, ih) = (rgba.width(), rgba.height());

    let scale = std::env::var("MIKUJI_SIXEL_SCALE")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(10);
    let sw = cell_w as u32 * scale;
    let sh = ((ih as f64 / iw as f64) * sw as f64 / 2.0) as u32;
    let rows = (sh as f64 / (scale as f64 * cell_aspect_ratio())).ceil() as u16;

    let resized = image::imageops::resize(&rgba, sw, sh, image::imageops::FilterType::Lanczos3);

    let q = |c: u8| -> u8 { ((c as u32 * 7 + 127) / 255 * 255 / 7) as u8 };

    let mut palette: Vec<(u8, u8, u8)> = Vec::new();
    let mut color_map = std::collections::HashMap::new();

    let white = (255u8, 255u8, 255u8);
    palette.push(white);
    color_map.insert(white, 0);

    for y in 0..sh {
        for x in 0..sw {
            let p = resized.get_pixel(x, y);
            let key = if p[3] < 128 { white } else { (q(p[0]), q(p[1]), q(p[2])) };
            if !color_map.contains_key(&key) && palette.len() < 256 {
                color_map.insert(key, palette.len());
                palette.push(key);
            }
        }
    }

    let mut out = std::io::stdout().lock();
    write!(out, "\x1bP0;1q").context("sixel start")?;

    for (i, &(r, g, b)) in palette.iter().enumerate() {
        let rp = (r as u32 * 100 / 255) as u8;
        let gp = (g as u32 * 100 / 255) as u8;
        let bp = (b as u32 * 100 / 255) as u8;
        write!(out, "#{};2;{};{};{}", i, rp, gp, bp).context("sixel palette")?;
    }

    for band_y in (0..sh).step_by(6) {
        for (ci, &color) in palette.iter().enumerate() {
            let mut has_data = false;
            let mut run_len = 0u32;
            let mut last_byte = 0u8;

            for x in 0..sw {
                let mut byte: u8 = 0;
                for dy in 0..6 {
                    let y = band_y + dy;
                    if y >= sh { break; }
                    let p = resized.get_pixel(x, y);
                    let pixel_color = if p[3] < 128 { white } else { (q(p[0]), q(p[1]), q(p[2])) };
                    if pixel_color == color { byte |= 1 << dy; }
                }

                if byte == last_byte && run_len > 0 {
                    run_len += 1;
                } else {
                    if run_len > 0 {
                        if !has_data {
                            write!(out, "#{}", ci).context("sixel color")?;
                            has_data = true;
                        }
                        if run_len >= 3 {
                            write!(out, "!{}{}", run_len, (last_byte + 63) as char)
                                .context("sixel rle")?;
                        } else {
                            for _ in 0..run_len {
                                write!(out, "{}", (last_byte + 63) as char)
                                    .context("sixel char")?;
                            }
                        }
                    }
                    last_byte = byte;
                    run_len = 1;
                }
            }

            if run_len > 0 {
                if !has_data {
                    write!(out, "#{}", ci).context("sixel color")?;
                }
                if run_len >= 3 {
                    write!(out, "!{}{}", run_len, (last_byte + 63) as char)
                        .context("sixel rle")?;
                } else {
                    for _ in 0..run_len {
                        write!(out, "{}", (last_byte + 63) as char)
                            .context("sixel char")?;
                    }
                }
            }

            if has_data && ci + 1 < palette.len() {
                write!(out, "$").context("sixel cr")?;
            }
        }

        if band_y + 6 < sh {
            write!(out, "-").context("sixel lf")?;
        }
    }

    write!(out, "\x1b\\").context("sixel end")?;
    out.flush().context("sixel flush")?;

    Ok(rows)
}

// ── 协议检测 ───────────────────────────────────────────

#[derive(Debug)]
pub enum Protocol {
    Kitty,
    Iterm2,
    Sixel,
}

/// 检测终端支持的图形协议。跟 yazi 的 Brand → Driver 映射对齐。
///
/// - WezTerm → Iterm2 (OSC 1337)
/// - iTerm2  → Iterm2 (OSC 1337)
/// - Kitty   → Kitty
/// - Ghostty/Konsole → Kitty
/// - Windows Terminal / WSL → Sixel
/// - 默认    → Kitty
///
/// 可通过 `MIKUJI_PROTOCOL` 环境变量覆盖，
/// 编译时 `--features force-sixel` 强制 Sixel。
pub fn detect_protocol() -> Option<Protocol> {
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

        if term_program.contains("WezTerm") {
            return Some(Protocol::Iterm2);
        }

        if term_program.contains("iTerm") || std::env::var("ITERM_SESSION_ID").is_ok() {
            return Some(Protocol::Iterm2);
        }

        // WSL 下默认走 Sixel（Windows Terminal 支持）
        if std::path::Path::new("/proc/sys/fs/binfmt_misc/WSLInterop").exists() {
            return Some(Protocol::Sixel);
        }

        Some(Protocol::Kitty)
    }
}

// ── 终端单元格像素尺寸查询 ────────────────────────────────

static CELL_PX_W: AtomicU64 = AtomicU64::new(0);
static CELL_PX_H: AtomicU64 = AtomicU64::new(0);

/// 通过 CSI 16t 查询终端真实单元格像素尺寸并缓存。
/// 查询失败时保持默认值 10px/col, 20px/row（ratio=2.0）。
pub(crate) fn init_cell_px() {
    // Priority 1: ioctl(TIOCGWINSZ) — yazi 同款方案
    if let Some((w, h)) = query_ioctl_winsize() {
        CELL_PX_W.store(w.clamp(5.0, 30.0).to_bits(), Ordering::Release);
        CELL_PX_H.store(h.clamp(10.0, 60.0).to_bits(), Ordering::Release);
        return;
    }
    // Priority 2: CSI 16t
    if let Some((w, h)) = query_csi_16t() {
        CELL_PX_W.store(w.clamp(5.0, 30.0).to_bits(), Ordering::Release);
        CELL_PX_H.store(h.clamp(10.0, 60.0).to_bits(), Ordering::Release);
    }
}

/// 终端每列对应的像素宽度。
/// CSI 16t 查询失败时回退到 10px。
pub(crate) fn px_per_col() -> f64 {
    let bits = CELL_PX_W.load(Ordering::Acquire);
    if bits == 0 { 10.0 } else { f64::from_bits(bits) }
}

/// 终端单元格高宽比 (height / width)。
/// CSI 16t 查询失败时回退到 2.0。
pub(crate) fn cell_aspect_ratio() -> f64 {
    let wb = CELL_PX_W.load(Ordering::Acquire);
    let hb = CELL_PX_H.load(Ordering::Acquire);
    if wb == 0 || hb == 0 {
        2.0
    } else {
        f64::from_bits(hb) / f64::from_bits(wb)
    }
}

/// 通过 ioctl(TIOCGWINSZ) 获取终端窗口总像素，除以行列数得单元格像素。
/// yazi 的同款方案。macOS iTerm2 / Linux 绝大多数终端可用。
fn query_ioctl_winsize() -> Option<(f64, f64)> {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        // SAFETY: stdout fd is always valid; winsize is POD, zeroed, and only read by ioctl
        let fd = std::io::stdout().as_raw_fd();
        let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
        if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws) } == 0
            && ws.ws_xpixel > 0
            && ws.ws_ypixel > 0
            && ws.ws_col > 0
            && ws.ws_row > 0
        {
            return Some((
                ws.ws_xpixel as f64 / ws.ws_col as f64,
                ws.ws_ypixel as f64 / ws.ws_row as f64,
            ));
        }
    }
    None
}

/// 发送 CSI 16t 查询，解析终端返回的单元格像素尺寸。
///
/// WezTerm 的响应格式为 `\x1b[6;;30;15t`（双分号），
/// 其他终端通常为 `\x1b[6;30;15t`（单分号），parser 兼容两种格式。
/// 返回 (px_per_col, px_per_row)。
fn query_csi_16t() -> Option<(f64, f64)> {
    crossterm::terminal::enable_raw_mode().ok()?;

    #[cfg(unix)]
    let saved_flags = {
        use std::os::unix::io::AsRawFd;
        // SAFETY: stdin fd is always valid; reading flags (F_GETFL) has no side effects
        let fd = std::io::stdin().as_raw_fd();
        let orig = unsafe { libc::fcntl(fd, libc::F_GETFL, 0) };
        if orig >= 0 {
            // SAFETY: O_NONBLOCK is added to prevent blocking on terminals that don't support 16t
            unsafe { libc::fcntl(fd, libc::F_SETFL, orig | libc::O_NONBLOCK) };
        }
        orig
    };

    let mut out = std::io::stdout().lock();
    write!(out, "\x1b[s\x1b[16t\x1b[u").ok()?;
    out.flush().ok()?;
    drop(out);

    let mut stdin = std::io::stdin().lock();
    let mut buf = Vec::new();
    let start = std::time::Instant::now();
    loop {
        if start.elapsed().as_millis() > 400 { break; }
        let mut b = [0u8; 1];
        match stdin.read(&mut b) {
            Ok(1) => {
                buf.push(b[0]);
                if b[0] == b't' && buf.windows(3).any(|w| w == b"\x1b[6") {
                    break;
                }
            }
            _ => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }

    #[cfg(unix)]
    if saved_flags >= 0 {
        use std::os::unix::io::AsRawFd;
        // SAFETY: restoring original flags (F_SETFL to saved value) — no side effects beyond reverting O_NONBLOCK
        let fd = std::io::stdin().as_raw_fd();
        unsafe { libc::fcntl(fd, libc::F_SETFL, saved_flags) };
    }

    crossterm::terminal::disable_raw_mode().ok()?;

    let s = String::from_utf8_lossy(&buf);
    let p = s.find("\x1b[6;")?;
    let rest = &s[p + 3..];
    let inner = &rest[..rest.find('t')?];
    let nums: Vec<u32> = inner.split(';').filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect();
    let h = *nums.first()?;
    let w = *nums.get(1)?;
    Some((w as f64, h as f64))
}
