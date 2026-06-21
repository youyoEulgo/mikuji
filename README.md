# mikuji

东方主题每日御神签。终端内显示角色立绘、诗歌、运势与评论。

![mikuji 截图](mikuji.png)

> **关于签池**：默认签文与立绘来自 ZUN（上海爱丽丝幻乐团）官方出版物 **《东方幻存神签》**（KADOKAWA, 2025）。
> 图片版权归原作者与出版社所有。勿作商业用途，请购买原书。愿神主宽恕。

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

**Windows Terminal 用户**：编译时指定使用 Sixel 协议。WSL 中默认自动走 Sixel。

```bash
cargo build --release --features force-sixel
cp target/release/mikuji ~/.local/bin/
```

这样编译出的二进制会固定使用 Sixel，无需设置环境变量。

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

| 顺序 | 路径                    | 说明                                |
| ---- | ----------------------- | ----------------------------------- |
| 1    | `$MIKUJI_DATA_DIR`      | 环境变量，完全自定义                |
| 2    | `$XDG_DATA_HOME/mikuji` | XDG 规范                            |
| 3    | `%LOCALAPPDATA%\mikuji` | Windows（若存在）                   |
| 4    | `~/.local/share/mikuji` | 默认数据目录（Linux/macOS，若存在） |
| 5    | `assets/`               | 开发时项目目录回退（若存在）        |
| 6    | `~/.local/share/mikuji` | 默认值                              |

Windows 下默认 `%LOCALAPPDATA%\mikuji\`。

数据目录结构：

```
~/.local/share/mikuji/
├── data.json    ← 签池数据
└── images/      ← 角色立绘（PNG）
```

## 终端兼容性

协议检测全自动，无需用户关心。当前支持以下终端：

| 终端 | 协议 |
|------|------|
| WezTerm, iTerm2 | iTerm2 (OSC 1337) |
| Kitty, Ghostty, Konsole | Kitty Graphics Protocol |
| Windows Terminal, foot | Sixel |
| xterm (部分), 原生 Linux | Kitty |

启动时通过环境变量识别终端品牌，并查询 CSI 16t 获取像素尺寸用于图片缩放。
终端不支持图片时图片区域为空，文字正常显示。

### 手动指定协议

```bash
MIKUJI_PROTOCOL=iterm2 mikuji   # iTerm2 协议
MIKUJI_PROTOCOL=kitty mikuji    # Kitty 协议
MIKUJI_PROTOCOL=sixel mikuji    # Sixel 协议
MIKUJI_PROTOCOL=none mikuji     # 禁用图片
```

或编译时指定：

```bash
cargo build --release --features force-sixel   # 固定 Sixel
```

或设置永久环境变量：`export MIKUJI_PROTOCOL=sixel`。

## 用法

```bash
mikuji                    # 按当天日期抽取（各人结果不同，自己同一天固定）
mikuji -r                 # 真随机抽取
mikuji -n 博丽灵梦         # 指定角色
mikuji -N 84              # 指定签号
mikuji -d 2026-02-18      # 指定日期
mikuji -l ja              # 日文模式
mikuji --list             # 列出所有角色
mikuji -w 120             # 指定终端宽度
```

默认抽取的种子由 **日期 + 编译时随机数** 混合而成。同一个人同一天结果固定，不同人编译出来的二进制结果不同。`--date` 用于回看特定日期的签（含编译种子的固定结果）。

## 图片宽度

编辑 `src/main.rs` 开头的常量：

```rust
const IMAGE_WIDTH: u16 = 100;
```

修改后重新编译。图片宽度不会超过终端宽度的 1/3。

终端单元格的像素尺寸通过 CSI 16t 自动查询（`init_cell_px`），图片据此
缩放到精确匹配字符列宽的像素尺寸。如果查询失败，回退到默认 10px/列。

## 自定义签池

用自己的 `data.json` 替换默认签池。格式：

```json
[
  {
    "name": "角色名",
    "cn_text": [
      "第",
      "1",
      "号",
      "大吉",
      "|",
      "标题",
      "角色名",
      "能力描述",
      "|",
      "诗歌第一行",
      "诗歌第二行",
      "|",
      "运势：...",
      "...",
      "|",
      "来源名称",
      "评论内容...",
      "|",
      "本页画师：xxx"
    ],
    "jp_text": [
      "第",
      "1",
      "番",
      "大吉",
      "|",
      "タイトル",
      "名前",
      "能力",
      "|",
      "詩歌...",
      "|",
      "運勢：...",
      "|",
      "上海アリス幻樂団",
      "コメント...",
      "|",
      "本页画师：xxx"
    ]
  }
]
```

**块结构：**

| 块  | 内容                           | 说明                         |
| --- | ------------------------------ | ---------------------------- |
| 0   | `第, N, 号/番, 吉凶1[, 吉凶2]` | 双吉凶时前一个显示灰色删除线 |
| 1   | `标题, 角色名, 能力`           | 固定 3 行                    |
| 2   | 诗歌                           | 纯诗歌行                     |
| 3   | 运势                           | 运势行（含 `运势：` 等的行） |
| 4   | `来源名, 评论...`              | 首行为来源，余行为评论内容   |
| 5   | `本页画师：xxx`                | 可选，无画师可省略此块       |

- 每个 `|` 独占一行，分隔各块。
- 诗歌和运势不再靠冒号自动区分——全由块位置决定。
- 条数任意，增删不影响稳定性。
- 图片文件名：`角色名.png`（`·` 和 `&` 替换为 `_`）。

### 吉凶等级颜色

| 颜色    | 关键词                                                              |
| ------- | ------------------------------------------------------------------- |
| 🔴 红   | `大吉` `超大吉` `最大吉` `大大吉` `大々吉` `吉` `奇迹☆` `ミラクル☆` |
| 🔴 亮红 | `中吉` `小吉` `小小吉` `小々吉`                                     |
| 🟡 黄   | `末吉` `半吉`                                                       |
| ⚪ 白   | `平` `吉凶*` `吉或凶` `吉か凶` `吉と凶` `自行决定` `自分次第`       |
| 🔵 蓝   | `凶` `小凶` `小小凶` `小々凶` `末凶`                                |
| ⚫ 灰   | `大凶` `超大凶` `最凶` `大大凶` `大々凶` `凶猛` `末大凶`            |
| 🟣 紫   | 混合型（`大凶`+`大吉` 并存）、`不明` `乱` `无` `無`                 |

不在表中的等级默认紫色，不会报错。

## 编译种子

每次编译会在 `build.rs` 中生成一个随机数（纳秒时间戳 XOR 进程 PID），与日期种子混合。这意味着：

- 你编译出来的二进制跟别人不一样，同一天各人结果不同
- 你自己的二进制同一天结果固定
- 重新编译后种子会变，历史结果不复现
- `-r` / `--random` 不依赖种子，每次真随机

## 许可

MIT
