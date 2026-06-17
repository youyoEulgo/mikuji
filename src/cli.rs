use clap::Parser;

/// 东方每日一签
#[derive(Parser, Debug)]
#[command(name = "mikuji", version, about = "东方主题每日御神签")]
pub struct Cli {
    /// 指定角色名
    #[arg(short, long)]
    pub name: Option<String>,

    /// 指定日期 YYYY-MM-DD
    #[arg(short, long)]
    pub date: Option<String>,

    /// 语言：cn / ja
    #[arg(short, long, default_value = "cn")]
    pub lang: String,

    /// 列出所有角色
    #[arg(long)]
    pub list: bool,

    /// 手动指定终端宽度（列数）
    #[arg(short, long)]
    pub width: Option<u16>,
}
