use std::path::PathBuf;

/// 返回 mikuji 数据目录。
///
/// 查找顺序：
/// 1. 环境变量 `MIKUJI_DATA_DIR`（直接使用，不检查存在性）
/// 2. 环境变量 `XDG_DATA_HOME/mikuji`（同上）
/// 3. `~/.local/share/mikuji`（若存在）
/// 4. 当前工作目录 `assets/`（若存在）
/// 5. `~/.local/share/mikuji`（默认值）
pub fn data_dir() -> PathBuf {
    // 1. 用户显式指定
    if let Ok(dir) = std::env::var("MIKUJI_DATA_DIR") {
        return dir.into();
    }
    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(dir).join("mikuji");
    }
    #[cfg(target_os = "windows")]
    if let Ok(dir) = std::env::var("LOCALAPPDATA") {
        return PathBuf::from(dir).join("mikuji");
    }

    let home_xdg = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(|h| PathBuf::from(h).join(".local").join("share").join("mikuji"));

    // 2. ~/.local/share/mikuji 已存在，直接使用
    if let Ok(ref dir) = home_xdg {
        if dir.exists() {
            return dir.clone();
        }
    }

    // 3. 开发目录回退
    let assets = PathBuf::from("assets");
    if assets.exists() {
        return assets;
    }

    // 4. 默认值
    home_xdg.unwrap_or_else(|_| PathBuf::from("assets"))
}
