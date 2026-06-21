// IIP 图像显示测试 — 已敲定方案
// doNotMoveCursor=1 + libc::write + 空行撑空间 + \x1b[A 回退
// cargo build && ./target/debug/test_image

use base64::{Engine, engine::general_purpose::STANDARD};
use crossterm::event::{Event, KeyCode};
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use std::fmt::Write as _;

fn main() {
    let raw = std::fs::read("assets/images/埴安神袿姬.png").unwrap();
    let img = image::ImageReader::new(std::io::Cursor::new(&raw))
        .with_guessed_format().unwrap().decode().unwrap();
    let (iw, ih) = (img.width(), img.height());
    let rgba = img.to_rgba8();

    let tw = crossterm::terminal::size().map(|(c, _)| c).unwrap_or(180);
    let term_rows = crossterm::terminal::size().map(|(_, r)| r).unwrap_or(40);
    let img_cols = 100u16.min(tw / 3);
    let left_margin = 1u16;
    let text_col = left_margin + img_cols + 3;

    let px_per_col = 10.0;
    let target_w = (img_cols as f64 * px_per_col) as u32;
    let target_h = ((ih as f64 / iw as f64) * target_w as f64) as u32;
    let resized = image::imageops::resize(&rgba, target_w, target_h, image::imageops::FilterType::Triangle);
    let (w, h) = (resized.width(), resized.height());

    let mut b = vec![];
    PngEncoder::new(&mut b).write_image(&resized, w, h, ExtendedColorType::Rgba8).unwrap();

    let mut seq = String::with_capacity(200 + b.len() * 4 / 3);
    write!(seq, "\x1b]1337;File=inline=1;size={};width={w}px;height={h}px;doNotMoveCursor=1:", b.len()).unwrap();
    STANDARD.encode_string(&b, &mut seq);
    write!(seq, "\x07").unwrap();

    let cell_ratio = 2.0;
    let img_h = (target_h as f64 / cell_ratio / px_per_col).ceil() as u16;

    eprintln!("终端{tw}col×{term_rows}row | 图片{img_cols}col×{img_h}row | seq={}b", seq.len());

    // 先输出 img_h+2 行空行撑出空间，再回退到图片起始行
    let mut full = String::new();
    for _ in 0..img_h + 2 {
        full.push('\n');
    }
    write!(full, "\x1b[{}A\x1b[{}G{seq}", img_h + 2, left_margin + 1).unwrap();
    for i in 0..img_h {
        write!(full, "\x1b[{}G行{i}: 测试文字  运势：大吉", text_col).unwrap();
        full.push('\n');
    }
    write!(full, "\x1b[{}G按 Enter/Esc/q 退出", text_col).unwrap();

    libc_write(&full);

    crossterm::terminal::enable_raw_mode().unwrap();
    loop {
        match crossterm::event::read().unwrap() {
            Event::Key(k) => match k.code {
                KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown
                | KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End => {}
                KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => break,
                _ => {}
            },
            _ => {}
        }
    }
    crossterm::terminal::disable_raw_mode().unwrap();
    println!();
}

fn libc_write(s: &str) {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = std::io::stdout().as_raw_fd();
        unsafe { libc::write(fd, s.as_ptr() as _, s.len()) };
    }
}
