// 终端探测测试 — yazi 风格
// 检测环境变量 + raw mode 批量查询
// cargo build && ./target/debug/test_detect

use std::io::{Read, Write};

fn main() {
    let term = std::env::var("TERM").unwrap_or_default();
    let tp = std::env::var("TERM_PROGRAM").unwrap_or_default();
    eprintln!("=== 环境变量 ===");
    eprintln!("TERM             = {term:?}");
    eprintln!("TERM_PROGRAM     = {tp:?}");
    eprintln!("KITTY_WINDOW_ID  = {:?}", std::env::var("KITTY_WINDOW_ID"));
    eprintln!("WEZTERM_EXEC     = {:?}", std::env::var("WEZTERM_EXECUTABLE"));
    eprintln!("WEZTERM_PANE     = {:?}", std::env::var("WEZTERM_PANE"));
    eprintln!("ITERM_SESSION    = {:?}", std::env::var("ITERM_SESSION_ID"));
    eprintln!("WT_Session       = {:?}", std::env::var("WT_Session"));
    eprintln!("KONSOLE_VERSION  = {:?}", std::env::var("KONSOLE_VERSION"));
    eprintln!("GHOSTTY_RES_DIR  = {:?}", std::env::var("GHOSTTY_RESOURCES_DIR"));
    eprintln!("WSL              = {}", std::path::Path::new("/proc/sys/fs/binfmt_misc/WSLInterop").exists());

    // ── raw mode + yazi 风格批量查询 ──
    crossterm::terminal::enable_raw_mode().unwrap();

    let query = concat!(
        "\x1b[s",
        "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\", // KittyGraphicsQuery
        "\x1b[>q",                                       // RequestXtVersion (secondary DA)
        "\x1b[16t",                                      // RequestCellPixelSize
        "\x1b]11;?\x07",                                 // RequestBgColor
        "\x1b[c",                                        // RequestDA1 (primary DA)
        "\x1b[u",
    );
    let mut out = std::io::stdout().lock();
    out.write_all(query.as_bytes()).unwrap();
    out.flush().unwrap();
    drop(out);

    // ── 读取响应 ──
    let mut buf = Vec::with_capacity(512);
    let start = std::time::Instant::now();
    let mut stdin = std::io::stdin().lock();
    loop {
        if start.elapsed().as_millis() > 1000 { eprintln!("超时 1s"); break; }
        let mut b = [0u8; 1];
        match stdin.read(&mut b) {
            Ok(1) => {
                buf.push(b[0]);
                if b[0] == b'c' && buf.contains(&0x1b)
                    && buf.rsplitn(2, |&x| x == 0x1b).next().is_some_and(|s| s.starts_with(b"[?"))
                {
                    eprintln!("DA1, {} bytes, {}ms", buf.len(), start.elapsed().as_millis());
                    break;
                }
            }
            _ => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }

    crossterm::terminal::disable_raw_mode().unwrap();

    // ── 解析 ──
    let s = String::from_utf8_lossy(&buf);
    eprintln!("\n=== 解析 ===");
    let kgp = s.contains("\x1b_Gi=31;OK");
    let sixel = ["?4;", "?4c", ";4;", ";4c"].iter().any(|p| s.contains(*p));
    eprintln!("Kitty(KGP): {kgp}");
    eprintln!("Sixel:      {sixel}");

    for b in ["kitty", "Konsole", "iTerm2", "WezTerm", "foot", "ghostty", "tmux "] {
        if s.contains(b) { eprintln!("CSI品牌: {b}"); }
    }
    // XtVersion: \x1bP>|...\x1b\\
    if let Some(p) = s.find("\x1bP>|") {
        let rest = &s[p + 4..];
        if let Some(e) = rest.find("\x1b\\") {
            eprintln!("XtVersion: {:?}", &rest[..e]);
        }
    }
    // CSI 16t
    if let Some(p) = s.find("\x1b[6;") {
        let rest = &s[p + 3..];
        let end = rest.find('t').unwrap_or(0);
        eprintln!("CSI 16t:  \\x1b[6;{}", &rest[..end]);
    }

    eprintln!("\n=== 原始响应 ({}) ===", buf.len());
    for (i, chunk) in buf.chunks(32).enumerate() {
        eprint!("{i:02x}: ");
        for b in chunk { eprint!("{b:02x} "); }
        eprint!(" |");
        for b in chunk {
            if b.is_ascii_graphic() || *b == b' ' { eprint!("{}", *b as char); } else { eprint!("."); }
        }
        eprintln!("|");
    }
}
