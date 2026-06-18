# mikuji

东方主题每日御神签。终端内显示角色立绘、诗歌、运势与评论。

![mikuji 截图](mikuji.png)

## 安装

### 前置条件

需要 Rust 工具链。如未安装：

- **Linux / macOS**: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Windows**: 前往 [rustup.rs](https://rustup.rs/) 下载安装器

或参考 [Rust 官方安装指南](https://www.rust-lang.org/zh-CN/tools/install)。

### 获取源码

```bash
git clone https://github.com/youyoEulgo/mikuji.git
cd mikuji
```

### 编译运行（开发时用）

```bash
cargo run
```

开发时数据目录回退到 `assets/`，无需额外配置。

### 编译安装（发布用）

```bash
cargo build --release
cp target/release/mikuji ~/.local/bin/
```

## 数据部署

安装后需将签池数据和图片放到数据目录。

```bash
# 创建数据目录
mkdir -p ~/.local/share/mikuji/images

# 从源码目录复制数据
cp assets/data.json ~/.local/share/mikuji/
cp assets/images/*.png ~/.local/share/mikuji/images/
```

程序运行时会自动查找数据目录，优先级如下：

| 顺序 | 路径 | 说明 |
|------|------|------|
| 1 | `$MIKUJI_DATA_DIR` | 环境变量，完全自定义 |
| 2 | `$XDG_DATA_HOME/mikuji` | XDG 规范 |
| 3 | `~/.local/share/mikuji` | 默认数据目录（Linux/macOS） |
| 4 | `assets/` | 开发时项目目录回退 |

Windows 下默认 `%LOCALAPPDATA%\mikuji\`。

数据目录结构：

```
~/.local/share/mikuji/
├── data.json    ← 签池数据
└── images/      ← 角色立绘（PNG）
```

## 终端兼容性

图片显示依赖 **Kitty Graphics Protocol**。

| ✅ 支持 | ❌ 不支持 |
|---------|----------|
| Kitty | Apple Terminal.app |
| iTerm2 (3.x+) | Windows Terminal |
| WezTerm | Alacritty |
| Konsole (24.08+) | foot |
| | Gnome Terminal |

不支持时图片区域为空，文字正常显示。

## 用法

```bash
mikuji                    # 按当天日期抽取
mikuji -n 博丽灵梦         # 指定角色
mikuji -d 2026-02-18      # 指定日期
mikuji -l ja               # 日文模式
mikuji --list              # 列出所有角色
mikuji -w 120              # 指定终端宽度
```

同一天同一签池结果固定（种子 = 年×10000 + 月×100 + 日）。

## 图片宽度

编辑 `src/main.rs` 开头的常量：

```rust
const IMAGE_WIDTH: u16 = 55;
```

修改后重新编译。图片宽度不会超过终端宽度的 1/3。

## 自定义签池

用自己的 `data.json` 替换默认签池。格式：

```json
[
  {
    "name": "角色名",
    "cn_text": [
      "第", "1", "号",
      "大吉",
      "标题",
      "角色名",
      "能力描述",
      "诗歌第一行",
      "诗歌第二行",
      "运势：...",
      "...",
      "[",
      "评论",
      "]来源名称",
      "评论内容...",
      "本页画师：xxx"
    ],
    "jp_text": [
      "第", "1", "番",
      "大吉",
      "タイトル",
      "名前",
      "能力",
      "詩歌...",
      "運勢：...",
      "[",
      "ｺﾒﾝﾄ",
      "]上海アリス幻樂団",
      "コメント..."
    ]
  }
]
```

**字段规范：**

| 位置 | 含义 | 备注 |
|------|------|------|
| `[0]` | `"第"` | 固定 |
| `[1]` | 签号 | 任意数字字符串 |
| `[2]` | `"号"` 或 `"番"` | 按语言 |
| `[3]` | 吉凶等级 | 自动匹配颜色，见下文 |
| `[4]` | 标题 | |
| `[5]` | 角色名 | |
| `[6]` | 能力 | |
| `[7]` | `"["` | 诗歌/运势区分隔标记 |
| 评论标题 | `"评论"` / `"ｺﾒﾝﾄ"` | |
| 来源 | `"]来源名"` | `]` 开头，显示为评论来源 |
| 末尾 | `"本页画师：xxx"` | 仅中文，单独格式 |

- 诗歌与运势按 **是否含 `：` 或 `:`** 自动区分——含冒号的是运势，否则是诗歌。
- 条数任意，增删不影响稳定性（同一天同一签池结果固定）。
- 图片文件名：`角色名.png`（`·` 和 `&` 替换为 `_`）。

### 吉凶等级颜色

| 颜色 | 关键词 |
|------|--------|
| 🔴 红 | `大吉` `超大吉` `最大吉` `大大吉` `吉` `奇迹` |
| 🔴 亮红 | `中吉` `小吉` `小小吉` |
| 🟡 黄 | `末吉` `半吉` |
| ⚪ 白 | `平` `吉凶*` `吉或凶` `自行决定` |
| 🔵 蓝 | `凶` `小凶` `小小凶` `末凶` |
| ⚫ 灰 | `大凶` `超大凶` `最凶` `大大凶` `凶猛` `末大凶` |
| 🟣 紫 | 混合型（大吉+大凶并存）、`不明` `乱` `无` |

不在表中的等级默认紫色，不会报错。

## 许可

MIT
