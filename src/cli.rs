use clap::Parser;

/// 东方主题每日御神签
///
/// 不指定参数时按当天日期抽取一签。
/// 同一天同签池结果固定。
#[derive(Parser, Debug)]
#[command(
    name = "mikuji",
    version,
    about = "东方主题每日御神签",
    after_help = "示例:
  mikuji                 按当天日期抽取
  mikuji -r              真随机抽取
  mikuji --name 博丽灵梦   指定角色
  mikuji --number 84      指定签号
  mikuji --date 2026-02-18 指定日期
  mikuji --lang ja         日文模式
  mikuji --list            列出所有角色
  mikuji --width 120       指定终端宽度（列数）

数据目录: ~/.local/share/mikuji/ （可用 MIKUJI_DATA_DIR 覆盖）
图片宽度: 见源码 IMAGE_WIDTH 常量，默认 55 列
"
)]
pub struct Cli {
    /// 指定角色名（模糊匹配请自行确保准确）
    #[arg(short, long)]
    pub name: Option<String>,

    /// 指定日期，格式 YYYY-MM-DD
    #[arg(short, long)]
    pub date: Option<String>,

    /// 语言：cn（中文）/ ja（日文）
    #[arg(short, long, default_value = "cn")]
    pub lang: String,

    /// 列出所有角色
    #[arg(long)]
    pub list: bool,

    /// 指定签号
    #[arg(short = 'N', long)]
    pub number: Option<u16>,

    /// 随机抽取（不依赖日期种子）
    #[arg(short, long)]
    pub random: bool,

    /// 手动指定终端宽度（列数）
    #[arg(short, long)]
    pub width: Option<u16>,
}
