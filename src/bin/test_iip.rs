// IIP 布局测试 — 先文字再图片，doNotMoveCursor=1
// cargo run --bin test_iip

use image::ImageEncoder;
use std::fmt::Write as _;

fn main() {
    let png_bytes = std::fs::read("assets/images/射命丸文.png").unwrap();

    let (tw, _th) = crossterm::terminal::size().unwrap_or((180, 24));
    let img_cells = 100_u16.min(tw / 3);
    let left_margin = 1;
    let text_col = left_margin + img_cells + 3;
    let text_max_w = tw.saturating_sub(text_col) as usize;

    let px_per_col = {
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = std::io::stdout().as_raw_fd();
            let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
            if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws) } == 0
                && ws.ws_xpixel > 0
                && ws.ws_col > 0
            {
                ws.ws_xpixel as f64 / ws.ws_col as f64
            } else {
                10.0
            }
        }
        #[cfg(not(unix))]
        {
            10.0
        }
    };

    let img = image::ImageReader::new(std::io::Cursor::new(&png_bytes))
        .with_guessed_format()
        .unwrap()
        .decode()
        .unwrap();
    let rgba = img.to_rgba8();
    let (iw, ih) = (rgba.width(), rgba.height());
    let target_w = img_cells as u32 * px_per_col as u32;
    let target_h = (target_w as u64 * ih as u64 / iw as u64) as u32;

    let resized = image::imageops::resize(
        &rgba,
        target_w,
        target_h,
        image::imageops::FilterType::Triangle,
    );
    let (w, h) = (resized.width(), resized.height());
    let mut buf = Vec::new();
    image::codecs::png::PngEncoder::new(&mut buf)
        .write_image(&resized, w, h, image::ExtendedColorType::Rgba8)
        .unwrap();
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &buf);
    let iip = format!(
        "\x1b]1337;File=inline=1;size={};width={w}px;height={h}px;doNotMoveCursor=1:{b64}\x07",
        buf.len(),
    );

    // ── 文字 ──
    let raw = load_text();
    let blocks: Vec<&[String]> = raw.split(|s| s == "|").collect();
    let hdr = blocks[0];
    let id = blocks[1];
    let poem_b = blocks[2];
    let fort_b = blocks[3];
    let comm_b = blocks[4];
    let art_b = blocks[5];

    let num = format!("第 {} 号", &hdr[1]);
    let l1 = &hdr[3];
    let l2 = hdr.get(4).map(|s| s.as_str()).unwrap_or("");
    let (ls, lr) = if !l2.is_empty() {
        (l1.as_str(), l2)
    } else {
        ("", l1.as_str())
    };
    let ti = id[0].as_str();
    let nm = id[1].as_str();
    let ab = id[2].as_str();
    let pm: Vec<_> = poem_b.iter().collect();
    let ft: Vec<_> = fort_b.iter().collect();
    let cs = comm_b.first().map(|s| s.as_str()).unwrap_or("");
    let cm: Vec<_> = comm_b.iter().skip(1).collect();
    let ar = art_b.first().map(|s| s.as_str()).unwrap_or("");

    let mut lines: Vec<String> = Vec::new();
    let (w, c, y, g, d, b, i, rst, r) = (
        "\x1b[37m", "\x1b[36m", "\x1b[33m", "\x1b[32m", "\x1b[90m", "\x1b[1m", "\x1b[3m",
        "\x1b[0m", "\x1b[31m",
    );
    if !ls.is_empty() {
        lines.push(format!(
            "{w}{num}  \x1b[90m\x1b[9m{ls}{rst}  {r}{b}【{lr}】{rst}"
        ));
    } else {
        lines.push(format!("{w}{num}  {r}{b}【{lr}】{rst}"));
    }
    lines.push(format!("{c}{ti}{rst}"));
    lines.push(format!("{y}{b}{nm}{rst}"));
    lines.push(format!("{w}{ab}{rst}"));
    lines.push(String::new());
    lines.push(format!("{d}──{rst}"));
    for l in &pm {
        lines.push(format!("  {w}{i}{l}{rst}"));
    }
    if !pm.is_empty() && !ft.is_empty() {
        lines.push(String::new());
    }
    for l in &ft {
        lines.push(format!("  {g}{l}{rst}"));
    }
    if !cm.is_empty() {
        lines.push(String::new());
        lines.push(format!("  {d}── {cs} 评论 ──{rst}"));
        for l in &cm {
            lines.push(format!("  {d}{l}{rst}"));
        }
    }
    lines.push(String::new());
    if !ar.is_empty() {
        lines.push(format!("{d}{i}{ar}{rst}"));
    }

    let wrapped: Vec<String> = lines
        .iter()
        .flat_map(|l| wrap_line(l, text_max_w))
        .collect();
    let text_rows = wrapped.len();
    eprintln!("text_rows={text_rows}");

    // 写文字 → 回退 → 叠图 → 归位（和 Python 脚本对齐）
    let mut out = String::new();
    writeln!(out).unwrap();

    for line in &wrapped {
        write!(out, "\x1b[{text_col}G{line}").unwrap();
        out.push('\n');
    }
    write!(out, "\x1b[{text_rows}A\x1b[{left_margin}G{iip}").unwrap();
    write!(out, "\x1b[{text_rows}B").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = std::io::stdout().as_raw_fd();
        unsafe { libc::write(fd, out.as_ptr() as _, out.len()) };
    }
}

fn load_text() -> Vec<String> {
    let data: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string("assets/data.json").unwrap()).unwrap();
    for e in &data {
        if e["name"].as_str() == Some("射命丸文") {
            return e["cn_text"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect();
        }
    }
    vec![]
}

fn wrap_line(s: &str, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let (mut cur, mut w) = (String::new(), 0);
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            cur.push(ch);
            cur.push('[');
            while let Some(sc) = chars.next() {
                cur.push(sc);
                if sc == 'm' {
                    break;
                }
            }
            continue;
        }
        let cw = if ch.is_ascii() { 1 } else { 2 };
        if w + cw > max && !cur.is_empty() {
            lines.push(cur);
            cur = String::new();
            w = 0;
        }
        cur.push(ch);
        w += cw;
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}
