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
    let date_seed = y * 10000 + m * 100 + d;
    let seed = date_seed ^ user_seed();
    let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
    entries[rng.gen_range(0..entries.len())].clone()
}

/// 编译时生成的用户种子。同一个人同一天结果固定，不同人不同。
fn user_seed() -> u64 {
    include_str!(concat!(env!("OUT_DIR"), "/user_seed.txt"))
        .trim()
        .parse()
        .unwrap_or(2026)
}

pub fn pick_random(entries: &[FortuneEntry]) -> FortuneEntry {
    let mut rng: StdRng = SeedableRng::from_entropy();
    entries[rng.gen_range(0..entries.len())].clone()
}

pub fn pick_by_name(entries: &[FortuneEntry], name: &str) -> Option<FortuneEntry> {
    entries.iter().find(|e| e.name == name).cloned()
}

pub fn pick_by_number(entries: &[FortuneEntry], num: u16) -> Option<FortuneEntry> {
    let target = num.to_string();
    entries.iter().find(|e| e.cn_text.get(1).map(|s| s.as_str()) == Some(&target)).cloned()
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
