use std::path::PathBuf;

/// 返回 mikuji 数据目录。
///
/// 查找顺序：
/// 1. `MIKUJI_DATA_DIR` 环境变量
/// 2. `XDG_DATA_HOME/mikuji`
/// 3. Windows: `%LOCALAPPDATA%\mikuji`（若存在）
/// 4. `~/.local/share/mikuji`（若存在）
/// 5. 当前目录 `assets/`（若存在）
/// 6. `~/.local/share/mikuji`（默认）
pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("MIKUJI_DATA_DIR") {
        return dir.into();
    }
    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(dir).join("mikuji");
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(dir) = std::env::var("LOCALAPPDATA") {
            let d = PathBuf::from(dir).join("mikuji");
            if d.exists() {
                return d;
            }
        }
    }

    let home_xdg = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(|h| PathBuf::from(h).join(".local").join("share").join("mikuji"));

    if let Ok(ref dir) = home_xdg {
        if dir.exists() {
            return dir.clone();
        }
    }

    let assets = PathBuf::from("assets");
    if assets.exists() {
        return assets;
    }

    home_xdg.unwrap_or_else(|_| PathBuf::from("assets"))
}
