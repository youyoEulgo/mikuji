use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> io::Result<()> {
    let out_dir = std::env::var("OUT_DIR").unwrap();

    // 编译时纳秒 + 进程 ID，每个用户每次编译都不同
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
        ^ std::process::id() as u64;

    std::fs::write(format!("{out_dir}/user_seed.txt"), format!("{seed}"))?;

    println!("cargo:rerun-if-changed=build.rs");
    Ok(())
}
