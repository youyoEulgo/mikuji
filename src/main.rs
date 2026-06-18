mod cli;
mod fortune;
mod image;
mod paths;

use chrono::Local;
use clap::Parser;
use fortune::{FortuneEntry, load_fortunes, pick_by_date, pick_by_name};
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

    // 基础字段（两种语言结构相同，索引一致）
    let number = &raw[1];
    let luck = &raw[3];
    let title = &raw[4];
    let name = &raw[5];
    let ability = &raw[6];

    // 诗歌 / 运势：raw[7..] 中 [ 之前是诗歌+运势，[ 之后是评论
    let bracket_pos = raw[7..].iter().position(|line| line == "[");
    let (poem, fortunes): (Vec<String>, Vec<String>) = match bracket_pos {
        Some(pos) => fortune::split_poem_and_fortunes(&raw[7..7 + pos]),
        None => fortune::split_poem_and_fortunes(&raw[7..]),
    };

    // 评论：'[' '评论' ']上海...' 之后到末尾（不含可能的画师行）
    let comment: Vec<String> = match bracket_pos {
        Some(pos) => {
            let comment_start = 7 + pos + 3;
            let mut comment_end = raw.len();
            // 最后一行如果是 "本页画师：…" 则不算在评论里
            if matches!(lang, Lang::Cn) && raw.last().map_or(false, |s| s.starts_with("本页画师："))
            {
                comment_end = comment_end.saturating_sub(1);
            }
            raw[comment_start.min(comment_end)..comment_end]
                .iter()
                .map(|s| s.to_string())
                .collect()
        }
        None => Vec::new(),
    };

    // 画师（仅中文）
    let artist = match lang {
        Lang::Cn if raw.last().map_or(false, |s| s.starts_with("本页画师：")) => {
            raw.last().unwrap().clone()
        }
        _ => String::new(),
    };

    let lc = luck_color(luck);

    let num = match lang {
        Lang::Cn => format!("第 {} 号", number),
        Lang::Jp => format!("第 {} 番", number),
    };

    // 评论来源：'[' '评论' ']来源名' 中的来源名
    let comment_source = match bracket_pos {
        Some(pos) => raw
            .get(7 + pos + 2)
            .map(|s| s.trim_start_matches(']'))
            .unwrap_or(""),
        None => "",
    };

    let mut v = Vec::new();
    v.push(format!("{w}{num}  {lc}{bld}【{}】{rst}", luck));
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
