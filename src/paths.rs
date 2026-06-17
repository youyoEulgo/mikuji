use std::path::PathBuf;

/// 返回 mikuji 数据目录。
///
/// 查找顺序：
/// 1. 环境变量 `MIKUJI_DATA_DIR`
/// 2. 环境变量 `XDG_DATA_HOME/mikuji`
/// 3. Windows: `%LOCALAPPDATA%\mikuji`
/// 4. `~/.local/share/mikuji`
/// 5. 可执行文件同级的 `assets/mikuji/`
/// 6. 当前工作目录的 `output/`
pub fn data_dir() -> PathBuf {
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

    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("mikuji");
    }

    // 兼容旧路径
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let d = parent.join("assets").join("mikuji");
            if d.exists() {
                return d;
            }
        }
    }

    PathBuf::from("output")
}
