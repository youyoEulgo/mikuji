mod cli;
mod fortune;
mod image;
mod paths;

use chrono::Local;
use clap::Parser;
use fortune::{ParsedFortune, load_fortunes, pick_by_date, pick_by_name};
use std::io::{self, Write};

fn main() {
    if let Err(e) = run() {
        eprintln!("mikuji: {e}");
        std::process::exit(1);
    }
}

// 图片显示宽度（列数）。直接改这个数即可。
const IMAGE_WIDTH: u16 = 55;

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = cli::Cli::parse();
    let entries = load_fortunes()?;
    if cli.list {
        for e in &entries {
            println!("{}", e.name);
        }
        return Ok(());
    }

    let lang = match cli.lang.as_str() {
        "ja" | "jp" | "japanese" => Lang::Jp,
        _ => Lang::Cn,
    };
    let fortune = if let Some(n) = &cli.name {
        pick_by_name(&entries, n).ok_or_else(|| format!("not found: {n}"))?
    } else {
        let d = if let Some(d) = &cli.date {
            chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")?
        } else {
            Local::now().date_naive()
        };
        pick_by_date(&entries, d)
    };

    // 使用 crossterm 获取终端宽度（主屏幕方案不需要高度做动态调整）
    let (tw, _th) = if let Some(w) = cli.width {
        (w, 24)
    } else if let Ok((cols, rows)) = crossterm::terminal::size() {
        (cols, rows)
    } else {
        (180, 24)
    };

    // 图片宽度：取 IMAGE_WIDTH 和终端宽度 1/4 的较小值
    let img_cells = IMAGE_WIDTH.min(tw / 3);
    let text_col = img_cells + 3;
    let text_max_w = tw.saturating_sub(text_col);

    // 先加载图片并测量尺寸
    let store = image::ImageStore::new();
    let png_bytes = store.load_png_bytes(&fortune.name).ok();
    let img_h = png_bytes
        .as_ref()
        .and_then(|b| image::measure_image(b, img_cells).ok())
        .map(|s| s.cell_h)
        .unwrap_or(0);

    // 计算文本换行后的高度
    let text = build_text(&fortune, lang);
    let mut wrapped_lines = Vec::new();
    for line in &text {
        wrapped_lines.extend(wrap_line(line, text_max_w as usize));
    }

    // 若图片比文字高，补空行使文字比图片多一行
    let text_rows = wrapped_lines.len() as u16;
    if img_h > text_rows {
        for _ in 0..(img_h - text_rows + 1) {
            wrapped_lines.push(String::new());
        }
    }

    let _total = img_h.max(wrapped_lines.len() as u16);

    // ── 1. 在当前命令行下方直接开始输出（不清屏，让内容进入回滚缓冲区）──
    let mut out = io::stdout().lock();
    writeln!(out)?; // 先换一行，避免和 shell 提示符粘在一起

    // ── 2. 发射图片 ──
    if let Some(ref png) = png_bytes {
        image::kitty_emit(png, img_cells, img_h)?;
    }

    // ── 3. 输出文字 ──
    // 图片显示后光标在左上角（C=1），每行文字先定位到 text_col 列再写
    for (row, line) in wrapped_lines.iter().enumerate() {
        if row > 0 {
            writeln!(out)?;
        }
        write!(out, "\x1b[{}G{}", text_col, line)?;
    }

    // ── 5. 提示并等待用户按键 ──
    write!(out, "\n按任意键退出...")?;
    out.flush()?;
    drop(out); // 释放锁，避免 read 时死锁

    // 用 crossterm 事件读取按键，忽略 Kitty 响应等非按键事件
    crossterm::terminal::enable_raw_mode()?;
    loop {
        if let crossterm::event::Event::Key(_) = crossterm::event::read()? {
            break;
        }
    }
    crossterm::terminal::disable_raw_mode()?;

    Ok(())
}

// ── helpers ───────────────────────────────────────────

enum Lang {
    Cn,
    Jp,
}

fn wrap_line(s: &str, max_display_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        // ANSI 转义序列（如 \x1b[37m、\x1b[0m）不计入显示宽度
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            chars.next(); // 消费 '['
            current_line.push(ch);
            current_line.push('[');
            while let Some(seq_ch) = chars.next() {
                current_line.push(seq_ch);
                if seq_ch == 'm' {
                    break;
                }
            }
            continue;
        }

        let char_width = if ch.is_ascii() { 1 } else { 2 };

        // 如果加上当前字符会超出宽度，先保存当前行
        if current_width + char_width > max_display_width && !current_line.is_empty() {
            lines.push(current_line);
            current_line = String::new();
            current_width = 0;
        }

        current_line.push(ch);
        current_width += char_width;
    }

    // 保存最后一行（如果有内容）
    if !current_line.is_empty() {
        lines.push(current_line);
    }

    // 如果原字符串为空，返回一个空行
    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn build_text(f: &ParsedFortune, lang: Lang) -> Vec<String> {
    let w = "\x1b[37m";
    let c = "\x1b[36m";
    let y = "\x1b[33m";
    let g = "\x1b[32m";
    let d = "\x1b[90m";
    let r = "\x1b[31m";
    let br = "\x1b[91m";
    let bl = "\x1b[34m";
    let m = "\x1b[35m";
    let bld = "\x1b[1m";
    let it = "\x1b[3m";
    let rst = "\x1b[0m";

    let lc = match f.luck.as_str() {
        "大吉" | "【大大吉】" | "大大吉" | "吉" | "末大吉" => r,
        "中吉" | "小吉" => br,
        "末吉" | "半吉" => y,
        "凶" | "末凶" | "小小凶" | "小々凶" => bl,
        "大凶" | "末大凶" | "【大大凶】" | "【最凶】" => d,
        "平" | "吉凶未卜" | "吉凶分界线" => w,
        _ => m,
    };
    let num = match lang {
        Lang::Cn => format!("第 {} 号", f.number),
        Lang::Jp => format!("第 {} 番", f.number),
    };
    let ti = match lang {
        Lang::Cn => f.title.clone(),
        Lang::Jp => f.source.jp_text.get(4).cloned().unwrap_or_default(),
    };
    let nm = match lang {
        Lang::Cn => f.name.clone(),
        Lang::Jp => f.source.jp_text.get(5).cloned().unwrap_or_default(),
    };
    let ab = match lang {
        Lang::Cn => f.ability.clone(),
        Lang::Jp => f.source.jp_text.get(6).cloned().unwrap_or_default(),
    };

    let mut v = Vec::new();
    v.push(format!("{w}{num}  {lc}{bld}【{}】{rst}", f.luck));
    v.push(format!("{c}{ti}{rst}"));
    v.push(format!("{y}{bld}{nm}{rst}"));
    v.push(format!("{w}{ab}{rst}"));
    v.push(String::new());
    v.push(format!("{d}──{rst}"));
    for l in &f.poem {
        v.push(format!("  {w}{it}{l}{rst}"));
    }
    if !f.poem.is_empty() && !f.fortunes.is_empty() {
        v.push(String::new());
    }
    for l in &f.fortunes {
        v.push(format!("  {g}{l}{rst}"));
    }
    if matches!(lang, Lang::Cn) && !f.comment.is_empty() {
        v.push(String::new());
        v.push(format!("  {d}── ZUN 评论 ──{rst}"));
        for l in &f.comment {
            v.push(format!("  {d}{l}{rst}"));
        }
    }
    v.push(String::new());
    let ar = match lang {
        Lang::Cn => f.artist.clone(),
        Lang::Jp => String::new(),
    };
    if !ar.is_empty() {
        v.push(format!("{d}{it}{ar}{rst}"));
    }
    v
}
