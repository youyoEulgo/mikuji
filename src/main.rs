mod cli;
mod fortune;
mod image;

use std::io::{self, Write};
use chrono::Local;
use clap::Parser;
use fortune::{load_fortunes, pick_by_date, pick_by_name, ParsedFortune};

fn main() { if let Err(e) = run() { eprintln!("mikuji: {e}"); std::process::exit(1); } }

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = cli::Cli::parse();
    let entries = load_fortunes()?;
    if cli.list { for e in &entries { println!("{}", e.name); } return Ok(()); }

    let lang = match cli.lang.as_str() { "ja"|"jp"|"japanese" => Lang::Jp, _ => Lang::Cn };
    let fortune = if let Some(n) = &cli.name {
        pick_by_name(&entries, n).ok_or_else(|| format!("not found: {n}"))?
    } else {
        let d = if let Some(d) = &cli.date {
            chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")?
        } else { Local::now().date_naive() };
        pick_by_date(&entries, d)
    };

    // 使用 crossterm 获取终端宽度（更可靠）
    let tw = if let Some(w) = cli.width {
        w
    } else if let Ok((cols, _rows)) = crossterm::terminal::size() {
        cols
    } else {
        180  // 最后的默认值
    };

    // 图片固定宽度 35 列，给文字留出更多空间
    let img_cells = 35_u16.min(tw / 4);  // 最多不超过终端宽度的 1/4
    let text_col = img_cells + 3;
    let text_max_w = tw.saturating_sub(text_col);

    // ── 1. 发射图片 ──
    let mut out = io::stdout().lock();
    write!(out, "\x1b[2J\x1b[H")?; out.flush()?; // clear screen + cursor to (1,1)

    let store = image::ImageStore::new();
    let img_h = match store.load_png_bytes(&fortune.name) {
        Ok(png) => image::kitty_emit(&png, img_cells).map(|s| s.cell_h).unwrap_or(0),
        Err(_) => 0,
    };

    // ── 2. 文字（支持自动换行）──
    let text = build_text(&fortune, lang);

    // 将每行文字按最大宽度换行
    let mut wrapped_lines = Vec::new();
    for line in &text {
        wrapped_lines.extend(wrap_line(line, text_max_w as usize));
    }

    let total = img_h.max(wrapped_lines.len() as u16);

    // 输出换行后的文本
    for (row, line) in wrapped_lines.iter().enumerate() {
        write!(out, "\x1b[{};{}H{}", row + 1, text_col, line)?;
    }

    // 光标移到输出下方
    write!(out, "\x1b[{};1H", total + 1)?;
    out.flush()?;

    Ok(())
}

// ── helpers ───────────────────────────────────────────

enum Lang { Cn, Jp }

fn wrap_line(s: &str, max_display_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;

    for ch in s.chars() {
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
    let w="\x1b[37m";let c="\x1b[36m";let y="\x1b[33m";let g="\x1b[32m";
    let d="\x1b[90m";let r="\x1b[31m";let br="\x1b[91m";let bl="\x1b[34m";
    let m="\x1b[35m";let bld="\x1b[1m";let it="\x1b[3m";let rst="\x1b[0m";

    let lc = match f.luck.as_str() {
        "大吉"|"【大大吉】"|"大大吉"|"吉"|"末大吉"=>r,"中吉"|"小吉"=>br,
        "末吉"|"半吉"=>y,"凶"|"末凶"|"小小凶"|"小々凶"=>bl,
        "大凶"|"末大凶"|"【大大凶】"|"【最凶】"=>d,
        "平"|"吉凶未卜"|"吉凶分界线"=>w,_=>m,
    };
    let num = match lang { Lang::Cn=>format!("第 {} 号",f.number), Lang::Jp=>format!("第 {} 番",f.number) };
    let ti = match lang { Lang::Cn=>f.title.clone(), Lang::Jp=>f.source.jp_text.get(4).cloned().unwrap_or_default() };
    let nm = match lang { Lang::Cn=>f.name.clone(), Lang::Jp=>f.source.jp_text.get(5).cloned().unwrap_or_default() };
    let ab = match lang { Lang::Cn=>f.ability.clone(), Lang::Jp=>f.source.jp_text.get(6).cloned().unwrap_or_default() };

    let mut v=Vec::new();
    v.push(format!("{w}{num}  {lc}{bld}【{}】{rst}",f.luck));
    v.push(format!("{c}{ti}{rst}")); v.push(format!("{y}{bld}{nm}{rst}")); v.push(format!("{w}{ab}{rst}"));
    v.push(String::new()); v.push(format!("{d}──{rst}"));
    for l in &f.poem { v.push(format!("  {w}{it}{l}{rst}")); }
    if !f.poem.is_empty()&&!f.fortunes.is_empty(){v.push(String::new());}
    for l in &f.fortunes { v.push(format!("  {g}{l}{rst}")); }
    if matches!(lang,Lang::Cn)&&!f.comment.is_empty(){
        v.push(String::new());v.push(format!("  {d}── ZUN 评论 ──{rst}"));
        for l in &f.comment { v.push(format!("  {d}{l}{rst}")); }
    }
    v.push(String::new());
    let ar=match lang{Lang::Cn=>f.artist.clone(),Lang::Jp=>String::new()};
    if !ar.is_empty(){v.push(format!("{d}{it}{ar}{rst}"));}
    v
}
