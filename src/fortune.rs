use serde::Deserialize;
use chrono::NaiveDate;
use rand::prelude::*;
use rand::rngs::StdRng;

#[derive(Debug, Clone, Deserialize)]
pub struct FortuneEntry {
    pub name: String,
    pub cn_text: Vec<String>,
    pub jp_text: Vec<String>,
}

pub fn load_fortunes() -> Result<Vec<FortuneEntry>, String> {
    let path = crate::paths::data_dir().join("data.json");
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("无法读取数据文件 {}: {}", path.display(), e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("解析数据文件失败: {}", e))
}

pub fn pick_by_date(entries: &[FortuneEntry], date: NaiveDate) -> FortuneEntry {
    let y = date.format("%Y").to_string().parse::<u64>().unwrap_or(2026);
    let m = date.format("%m").to_string().parse::<u64>().unwrap_or(1);
    let d = date.format("%d").to_string().parse::<u64>().unwrap_or(1);
    let seed = y * 10000 + m * 100 + d;
    let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
    entries[rng.gen_range(0..entries.len())].clone()
}

pub fn pick_by_name(entries: &[FortuneEntry], name: &str) -> Option<FortuneEntry> {
    entries.iter().find(|e| e.name == name).cloned()
}

/// 将 raw[7..][..pos] 诗歌+运势区拆分为 (诗歌, 运势)。含 `：` 或 `:` 的行视为运势。
pub(crate) fn split_poem_and_fortunes(lines: &[String]) -> (Vec<String>, Vec<String>) {
    let mut poem = Vec::new();
    let mut fortunes = Vec::new();
    for line in lines {
        if line.trim().is_empty() { continue; }
        if line.contains('：') || line.contains(':') {
            fortunes.push(line.to_string());
        } else {
            poem.push(line.to_string());
        }
    }
    (poem, fortunes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_date_seed_is_stable() {
        let d1 = NaiveDate::from_ymd_opt(2026, 6, 17).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2026, 6, 17).unwrap();
        let y = |d: NaiveDate| d.format("%Y").to_string().parse::<u64>().unwrap_or(2026);
        let m = |d: NaiveDate| d.format("%m").to_string().parse::<u64>().unwrap_or(1);
        let dd = |d: NaiveDate| d.format("%d").to_string().parse::<u64>().unwrap_or(1);
        let s1 = y(d1) * 10000 + m(d1) * 100 + dd(d1);
        let s2 = y(d2) * 10000 + m(d2) * 100 + dd(d2);
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_name_to_filename() {
        let f = |s: &str| s.replace(['·', '&'], "_") + ".png";
        assert_eq!(f("博丽灵梦"), "博丽灵梦.png");
        assert_eq!(f("帕秋莉·诺蕾姬"), "帕秋莉_诺蕾姬.png");
    }
}
