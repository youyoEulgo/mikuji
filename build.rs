use std::fs;
use std::io;
use std::path::Path;

/// 目标缩放宽度（像素），保持竖版比例
const TARGET_WIDTH: u32 = 400;

fn main() -> io::Result<()> {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let project_root = Path::new(&manifest_dir);

    let should_embed = std::env::var("MIKUJI_EMBED_IMAGES").is_ok();

    // ── 快速模式：不嵌入图片 ──
    if !should_embed {
        eprintln!("MIKUJI_EMBED_IMAGES not set — skipping image embedding (fast build)");
        // 写入空索引（0 条目）
        let index = 0u32.to_le_bytes();
        fs::write(format!("{}/image_index.bin", out_dir), index)?;
        fs::write(format!("{}/image_blob.bin", out_dir), [])?;
        return Ok(());
    }

    // ── 完整模式：嵌入全部缩放后的图片 ──
    eprintln!("MIKUJI_EMBED_IMAGES set — embedding all images (slow build)...");

    let data_path = project_root.join("output").join("data.json");
    let images_dir = project_root.join("output").join("images");

    println!("cargo:rerun-if-changed=output/data.json");
    println!("cargo:rerun-if-changed=output/images/");

    // 读取 data.json，获取 128 个角色名
    let data: Vec<serde_json::Value> = {
        let content = fs::read_to_string(&data_path)?;
        serde_json::from_str(&content).expect("Failed to parse data.json")
    };

    // 收集所有 (name, resized_png_bytes)
    let mut entries: Vec<(String, Vec<u8>)> = Vec::with_capacity(data.len());
    let mut processed = 0u32;
    let total = data.len() as u32;

    for entry in &data {
        let name = entry["name"].as_str().unwrap().to_string();
        let filename = name_to_filename(&name);
        let img_path = images_dir.join(&filename);

        let png_bytes = match process_image(&img_path) {
            Ok(bytes) => bytes,
            Err(e) => {
                panic!("Failed to process image {}: {}", img_path.display(), e);
            }
        };

        processed += 1;
        eprintln!(
            "[{} / {}] {}  ({} KiB)",
            processed,
            total,
            name,
            png_bytes.len() / 1024
        );

        entries.push((name, png_bytes));
    }

    // ── 构建索引 + 拼接 Blob ──

    let mut index_buf = Vec::new();
    let mut blob_buf = Vec::new();

    // index header: num_entries (u32 LE)
    index_buf.extend_from_slice(&(entries.len() as u32).to_le_bytes());

    for (name, png_bytes) in &entries {
        let offset = blob_buf.len() as u32;
        let len = png_bytes.len() as u32;

        // name_len (u16) + name (UTF-8)
        let name_bytes = name.as_bytes();
        index_buf.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        index_buf.extend_from_slice(name_bytes);

        // offset (u32) + len (u32)
        index_buf.extend_from_slice(&offset.to_le_bytes());
        index_buf.extend_from_slice(&len.to_le_bytes());

        blob_buf.extend_from_slice(png_bytes);
    }

    // 写入 OUT_DIR
    let index_path = format!("{}/image_index.bin", out_dir);
    let blob_path = format!("{}/image_blob.bin", out_dir);

    fs::write(&index_path, &index_buf)?;
    fs::write(&blob_path, &blob_buf)?;

    eprintln!(
        "Embedded {} images, total blob size: {:.1} MiB",
        entries.len(),
        blob_buf.len() as f64 / (1024.0 * 1024.0)
    );

    Ok(())
}

/// 加载 PNG，缩放，重新编码为 PNG 字节
fn process_image(path: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let img = image::ImageReader::open(path)?.decode()?;

    // 计算等比缩放后的高度
    let (orig_w, orig_h) = (img.width(), img.height());
    let new_h = (orig_h as f32 * TARGET_WIDTH as f32 / orig_w as f32) as u32;

    let resized = img.resize(TARGET_WIDTH, new_h, image::imageops::FilterType::Lanczos3);

    // 编码为 PNG
    let mut png_bytes = Vec::new();
    {
        let mut cursor = io::Cursor::new(&mut png_bytes);
        resized.write_to(&mut cursor, image::ImageFormat::Png)?;
    }

    Ok(png_bytes)
}

/// 角色名 → 文件名（· → _，& → _）
fn name_to_filename(name: &str) -> String {
    let mut s = name.replace(['·', '&'], "_");
    s.push_str(".png");
    s
}
