use serde::Deserialize;
use chrono::NaiveDate;
use rand::prelude::*;
use rand::rngs::StdRng;

const FORTUNE_JSON: &str = include_str!("../output/data.json");

#[derive(Debug, Clone, Deserialize)]
pub struct FortuneEntry {
    pub name: String,
    #[allow(dead_code)]
    pub img_url: String,
    pub cn_text: Vec<String>,
    pub jp_text: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedFortune {
    pub number: String,
    pub luck: String,
    pub title: String,
    pub name: String,
    pub ability: String,
    pub poem: Vec<String>,
    pub fortunes: Vec<String>,
    pub comment: Vec<String>,
    pub artist: String,
    pub source: FortuneEntry,
}

pub fn load_fortunes() -> serde_json::Result<Vec<FortuneEntry>> {
    serde_json::from_str(FORTUNE_JSON)
}

pub fn pick_by_date(entries: &[FortuneEntry], date: NaiveDate) -> ParsedFortune {
    let y = date.format("%Y").to_string().parse::<u64>().unwrap_or(2026);
    let m = date.format("%m").to_string().parse::<u64>().unwrap_or(1);
    let d = date.format("%d").to_string().parse::<u64>().unwrap_or(1);
    let seed = y * 10000 + m * 100 + d;
    let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
    parse_fortune(&entries[rng.gen_range(0..entries.len())])
}

pub fn pick_by_name(entries: &[FortuneEntry], name: &str) -> Option<ParsedFortune> {
    entries.iter().find(|e| e.name == name).map(parse_fortune)
}

fn parse_fortune(entry: &FortuneEntry) -> ParsedFortune {
    let bracket_pos = entry.cn_text[7..].iter().position(|line| line == "[");

    let (poem, fortunes, comment, artist) = match bracket_pos {
        Some(pos) => {
            let content = &entry.cn_text[7..7 + pos];
            let (p, f) = split_poem_and_fortunes(content);
            let text_len = entry.cn_text.len();
            let comment_end = text_len.saturating_sub(1);
            let comment_body_start = (7 + pos + 3).min(comment_end);
            let comment_lines: Vec<String> = if comment_body_start < comment_end {
                entry.cn_text[comment_body_start..comment_end].to_vec()
            } else {
                vec![]
            };
            let artist = entry.cn_text.last().cloned().unwrap_or_default();
            (p, f, comment_lines, artist)
        }
        None => {
            let (p, f) = split_poem_and_fortunes(&entry.cn_text[7..]);
            (p, f, vec![], String::new())
        }
    };

    ParsedFortune {
        number: entry.cn_text[1].clone(),
        luck: entry.cn_text[3].clone(),
        title: entry.cn_text[4].clone(),
        name: entry.cn_text[5].clone(),
        ability: entry.cn_text[6].clone(),
        poem,
        fortunes,
        comment,
        artist,
        source: entry.clone(),
    }
}

fn split_poem_and_fortunes(lines: &[String]) -> (Vec<String>, Vec<String>) {
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
