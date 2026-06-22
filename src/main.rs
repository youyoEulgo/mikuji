mod cli;
mod fortune;
mod image;
mod paths;

use chrono::Local;
use clap::Parser;
use fortune::{
    FortuneEntry, load_fortunes, pick_by_date, pick_by_name, pick_by_number, pick_random,
};
use std::io::{self, Write as IoWrite};

fn main() {
    if let Err(e) = run() {
        eprintln!("mikuji: {e}");
        std::process::exit(1);
    }
}

// 图片显示宽度（列数）。直接改这个数即可。
const IMAGE_WIDTH: u16 = 100;
// 左侧偏移（列数）- 让图片不紧贴左边缘
const LEFT_MARGIN: u16 = 1;

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
    } else if let Some(num) = cli.number {
        pick_by_number(&entries, num).ok_or_else(|| format!("签号 {num} 不存在"))?
    } else if cli.random {
        pick_random(&entries)
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

    // 图片宽度：取 IMAGE_WIDTH 和终端宽度 1/3 的较小值
    let img_cells = IMAGE_WIDTH.min(tw / 3);
    let text_col = LEFT_MARGIN + img_cells + 3;
    let text_max_w = tw.saturating_sub(text_col);

    // 先加载图片
    let store = image::ImageStore::new();
    let png_bytes = store.load_png_bytes(&fortune.name).ok();

    // 计算文本换行
    let text = build_text(&fortune, lang);
    let mut wrapped_lines = Vec::new();
    for line in &text {
        wrapped_lines.extend(wrap_line(line, text_max_w as usize));
    }
    let text_rows = wrapped_lines.len() as u16;

    // 检测协议
    let protocol = image::detect_protocol();

    // ── 1. 开始输出 ──
    let mut out = io::stdout().lock();
    writeln!(out)?;
    drop(out);

    // Cell pixel size — CSI 16t 查询终端真实像素尺寸，用于图片缩放。
    image::init_cell_px();

    // ── 2. 发射图片 ──
    let img_h: u16 = if let Some(ref png) = png_bytes {
        match protocol {
            Some(image::Protocol::Kitty) => {
                let size = image::measure_image(png, img_cells)?;
                let mut out = io::stdout().lock();
                write!(out, "\x1b[{}G", LEFT_MARGIN)?;
                drop(out);
                image::kitty_emit(png, img_cells, size.cell_h)?;
                size.cell_h
            }
            Some(image::Protocol::Iterm2) => {
                eprintln!("[mikuji] 使用 iTerm2 IIP 协议");
                let (iip_seq, img_h) = image::iip_encode(png, img_cells)?;

                let text_rows = wrapped_lines.len() as u16;
                // 总高度 = 图片和文字中较大的 + 1行提示
                let total = img_h.max(text_rows) + 1;

                let mut full = String::new();
                use std::fmt::Write;

                // 1. 写文字（右侧），不够 total-1 行就补空行
                for (_row, line) in wrapped_lines.iter().enumerate() {
                    write!(full, "\x1b[{}G{}", text_col, line).unwrap();
                    full.push('\n');
                }
                for _ in text_rows..total - 1 {
                    full.push('\n');
                }
                // 2. 提示
                write!(full, "\x1b[{}G按任意键退出...", text_col).unwrap();

                // 3. 回退 total 行到起始位置，叠图片
                write!(full, "\x1b[{}A\x1b[{}G{}", total, LEFT_MARGIN, iip_seq).unwrap();

                // 4. 光标下移 total 行，回到底部
                write!(full, "\x1b[{}B", total).unwrap();

                #[cfg(unix)]
                {
                    use std::os::unix::io::AsRawFd;
                    let fd = std::io::stdout().as_raw_fd();
                    let n = unsafe { libc::write(fd, full.as_ptr() as _, full.len()) };
                    if n < 0 { return Err("libc::write failed".into()); }
                }
                img_h
            }
            Some(image::Protocol::Sixel) => {
                let h = image::sixel_compute_height(png, img_cells)?;
                let mut out = io::stdout().lock();
                write!(out, "\x1b7\x1b[{}G", LEFT_MARGIN)?;
                drop(out);
                image::sixel_emit(png, img_cells)?;
                let mut out = io::stdout().lock();
                write!(out, "\x1b8")?;
                h
            }
            None => 0,
        }
    } else {
        0
    };

    let iip_done = matches!(protocol, Some(image::Protocol::Iterm2));

    if !iip_done {
        // ── 3. 输出文字 ──
        let mut out = io::stdout().lock();

        // 图片比文字高时补空行
        if img_h > text_rows {
            for _ in 0..(img_h - text_rows + 1) {
                wrapped_lines.push(String::new());
            }
        }
        let _total = img_h.max(wrapped_lines.len() as u16);

        for (row, line) in wrapped_lines.iter().enumerate() {
            if row > 0 {
                writeln!(out)?;
            }
            write!(out, "\x1b[{}G{}", text_col, line)?;
        }

        write!(out, "\n按任意键退出...")?;
        out.flush()?;
        drop(out);
    }

    // 用 crossterm 事件读取按键，忽略 Kitty 响应等非按键事件
    crossterm::terminal::enable_raw_mode()?;
    loop {
        match crossterm::event::read()? {
            crossterm::event::Event::Key(k) => {
                use crossterm::event::KeyCode;
                match k.code {
                    KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown
                    | KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End => {}
                    _ => break,
                }
            }
            _ => {} // 忽略 Mouse, Resize 等
        }
    }
    crossterm::terminal::disable_raw_mode()?;

    Ok(())
}

fn luck_color(luck: &str) -> &'static str {
    let has = |s: &str| luck.contains(s);

    // 混合型（吉凶并存）→ 紫
    if (has("大吉") || luck == "吉") && (has("大凶") || has("最凶"))
        || has("大凶") && (has("部分") || has("一部"))
    {
        return "\x1b[35m";
    }

    // 大吉系 → 红
    if has("大吉")
        || has("超大吉")
        || has("最大吉")
        || has("大大吉")
        || has("大々吉")
        || luck == "吉"
        || has("奇迹")
        || has("ミラクル")
    {
        return "\x1b[31m";
    }

    // 中吉/小吉 → 亮红
    if has("中吉") || has("小吉") || has("小小吉") || has("小々吉") {
        return "\x1b[91m";
    }

    // 末吉/半吉 → 黄
    if has("末吉") || has("半吉") {
        return "\x1b[33m";
    }

    // 平/吉凶中间型 → 白（必须在泛凶之前）
    if has("平")
        || has("吉凶")
        || has("吉か凶")
        || has("吉或凶")
        || has("吉と凶")
        || has("自行决定")
        || has("自分次第")
    {
        return "\x1b[37m";
    }

    // 大凶/最凶系 → 灰
    if has("大凶")
        || has("超大凶")
        || has("最凶")
        || has("大大凶")
        || has("大々凶")
        || has("凶猛")
        || has("末大凶")
    {
        return "\x1b[90m";
    }

    // 小凶/末凶/泛凶 → 蓝
    if has("凶") || has("小凶") || has("小小凶") || has("小々凶") || has("末凶") {
        return "\x1b[34m";
    }

    "\x1b[35m" // 不明 / 乱 / 无 / 無 → 紫
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

fn build_text(f: &FortuneEntry, lang: Lang) -> Vec<String> {
    let w = "\x1b[37m";
    let c = "\x1b[36m";
    let y = "\x1b[33m";
    let g = "\x1b[32m";
    let d = "\x1b[90m";
    let bld = "\x1b[1m";
    let it = "\x1b[3m";
    let rst = "\x1b[0m";
    let raw = match lang {
        Lang::Cn => &f.cn_text,
        Lang::Jp => &f.jp_text,
    };

    // 按 | 拆块: header | identity | poem | fortunes | comment | artist
    let blocks: Vec<&[String]> = raw.split(|s| s == "|").collect();
    let header = blocks.first().copied().unwrap_or(&[]);
    let identity = blocks.get(1).copied().unwrap_or(&[]);
    let poem_block = blocks.get(2).copied().unwrap_or(&[]);
    let fortune_block = blocks.get(3).copied().unwrap_or(&[]);
    let comment_block = blocks.get(4).copied().unwrap_or(&[]);
    let artist_block = blocks.get(5).copied().unwrap_or(&[]);

    // 签号块：第, N, 号/番, 吉凶1, [吉凶2]
    let number = header.get(1).map(|s| s.as_str()).unwrap_or("");
    let luck1 = header.get(3).map(|s| s.as_str()).unwrap_or("");
    let luck2 = header.get(4).map(|s| s.as_str()).unwrap_or("");

    // 有第二个吉凶 → 第一个划掉
    let (luck_strike, luck_real) = if !luck2.is_empty() {
        (luck1, luck2)
    } else {
        ("", luck1)
    };
    let lc = luck_color(luck_real);

    // 身份块：标题, 角色名, 能力
    let title = identity.first().map(|s| s.as_str()).unwrap_or("");
    let name = identity.get(1).map(|s| s.as_str()).unwrap_or("");
    let ability = identity.get(2).map(|s| s.as_str()).unwrap_or("");

    // 诗歌、运势已由 | 分隔
    let poem: Vec<String> = poem_block.iter().map(|s| s.clone()).collect();
    let fortunes: Vec<String> = fortune_block.iter().map(|s| s.clone()).collect();

    // 评论块：首行来源，余行内容
    let comment_source = comment_block.first().map(|s| s.as_str()).unwrap_or("");
    let comment: Vec<String> = comment_block.iter().skip(1).map(|s| s.clone()).collect();

    // 画师
    let artist = artist_block.first().map(|s| s.as_str()).unwrap_or("");

    let num = match lang {
        Lang::Cn => format!("第 {} 号", number),
        Lang::Jp => format!("第 {} 番", number),
    };

    let mut v = Vec::new();

    // 吉凶行：划掉项 → 灰色删除线 + 彩色真实值
    if !luck_strike.is_empty() {
        v.push(format!(
            "{w}{num}  \x1b[90m\x1b[9m{luck_strike}{rst}  {lc}{bld}【{luck_real}】{rst}"
        ));
    } else {
        v.push(format!("{w}{num}  {lc}{bld}【{luck_real}】{rst}"));
    }

    v.push(format!("{c}{title}{rst}"));
    v.push(format!("{y}{bld}{name}{rst}"));
    v.push(format!("{w}{ability}{rst}"));
    v.push(String::new());
    v.push(format!("{d}──{rst}"));
    for l in &poem {
        v.push(format!("  {w}{it}{l}{rst}"));
    }
    if !poem.is_empty() && !fortunes.is_empty() {
        v.push(String::new());
    }
    for l in &fortunes {
        v.push(format!("  {g}{l}{rst}"));
    }
    if !comment.is_empty() {
        v.push(String::new());
        v.push(format!("  {d}── {comment_source} 评论 ──{rst}"));
        for l in &comment {
            v.push(format!("  {d}{l}{rst}"));
        }
    }
    v.push(String::new());
    if !artist.is_empty() {
        v.push(format!("{d}{it}{artist}{rst}"));
    }
    v
}
