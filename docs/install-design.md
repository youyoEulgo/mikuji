# mikuji 安装与发布设计

本文记录一键安装方案的设计与取舍，实现见 `scripts/install.sh`、`scripts/install_test.sh`
与 `.github/workflows/`。

## 1. 为什么不能照抄 yakumo_router

`yakumo_router` 的安装脚本只搬一个文件：它的前端产物用 `rust-embed` 编进了二进制，
安装 = 复制二进制 + `yakumo init`。

mikuji 不同：

| | yakumo_router | mikuji |
| --- | --- | --- |
| 二进制 | 单文件，资源内嵌 | 约 2 MB |
| 数据 | 无 | `data.json` 292 KB + 128 张立绘约 224 MB |
| 初始化 | `init` 子命令 | 无；`user_seed.txt` 首次运行自动生成 |
| 可验证性 | `--version` | `--version`，且 `--list` 能端到端验证数据 |

因此脚本骨架（平台探测、PATH 写入、本地/下载双模式、macOS quarantine、`mktemp` + `trap`）
可以复用，但数据部署与"升级不能毁掉用户种子"才是真正的设计重点。

## 2. 发布形态：二进制与数据解耦

每个 `vX.Y.Z` 发布四个二进制约 3 MB 的包，数据用独立 tag 发布一次、跨版本复用：

```
vX.Y.Z/mikuji-vX.Y.Z-x86_64-unknown-linux-musl.tar.gz
vX.Y.Z/mikuji-vX.Y.Z-aarch64-unknown-linux-musl.tar.gz
vX.Y.Z/mikuji-vX.Y.Z-aarch64-apple-darwin.tar.gz
vX.Y.Z/mikuji-vX.Y.Z-x86_64-apple-darwin.tar.gz
vX.Y.Z/checksums.txt
data-vN/mikuji-data-vN.tar.gz          ← tag 形如 data-v1
data-vN/mikuji-data-vN.json            ← 仅 data.json（约 292 KB），--no-images 用
data-vN/checksums.txt
```

二进制包内含 `mikuji` + `README.md` + `LICENSE` + `install.sh`；数据包只含 `data.json` + `images/`。
数据包不放 `install.sh`，避免旧数据 tag 里残留过期的安装脚本。

数据 release 同时提供一份单独的 `data.json`（约 292 KB）。`--no-images` 只下这一份，
所以"纯文字安装"是真的省下 224 MB，而不是下完整包再丢掉立绘。

好处：升级二进制只下约 3 MB；签池更新时只发一个新 data tag，所有后续版本复用。

### 目标平台

| target | runner | 说明 |
| --- | --- | --- |
| `x86_64-unknown-linux-musl` | `ubuntu-latest` | 静态链接，不挑 glibc 版本 |
| `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` | |
| `aarch64-apple-darwin` | `macos-14` | |
| `x86_64-apple-darwin` | `macos-15-intel` | `macos-13` 已被 GitHub 下线 |

mikuji 对 libc 的依赖只有 `ioctl(TIOCGWINSZ)` / `fcntl` / `write`，musl 全部支持，静态链接无障碍。

### 数据 release 不能成为 Latest

`install.sh` 通过 `/releases/latest` 的跳转解析最新版本。若数据 release 被标成 Latest，
解析会拿到 `data-v1` 这种非版本 tag。两道保险：

1. 数据 release 用 `make_latest: false` 创建；
2. 脚本侧只接受匹配 `^v[0-9]` 的 tag，否则回退到列出 releases 再筛选。

### 打包端不再产生 macOS 垃圾

旧的手工包里有 `._*` AppleDouble 文件、`com.apple.provenance` xattr，二进制权限还是 0700。
CI 里用 `COPYFILE_DISABLE: 1`、macOS 侧 `tar --no-xattrs`（GNU tar 不支持该参数，故按 runner 分支）、
`chmod 0755` 修掉。安装脚本另有兜底：复制立绘时跳过 `._*` 与 `.DS_Store`。

## 3. 安装脚本的行为约定

### 目录解析必须与 `src/paths.rs` 一致

```
MIKUJI_DATA_DIR  >  $XDG_DATA_HOME/mikuji  >  ~/.local/share/mikuji
```

脚本与 Rust 侧规则不一致会导致"装到 A、去 B 找"。XDG 变量为空串时脚本按未设置处理
（`paths.rs` 会把空串当已设置，属病态输入，不在脚本内复刻）。

### 升级语义（mikuji 特有，最重要）

- **只覆盖** `data.json` 与 `images/`；
- `user_seed.txt`、`config.toml` **永不触碰**，README 明确"不要随便删除种子"；
- `images/` 只覆盖同名文件，不删除用户自有文件；
- `data.json` 用"临时文件 + `mv`"原子替换；
- 数据版本标记写在 `$DATA_DIR/.mikuji_data_version`；值相同则跳过下载。
  纯文字安装写 `data-vN+text`，之后换成完整安装会重新拉取（因为 `+text` 不等于 `data-vN`）。
- 卸载默认保留数据目录；`--purge` 只允许删除"内含 `data.json` 或版本标记"的目录，
  且拒绝 `/`、`$HOME`、空路径。

### 先全部下载校验，再落盘

1. 解析参数与目录，打印计划；
2. `mktemp -d` + `trap` 清理；
3. 下载二进制包与数据包到临时目录，逐个用 `checksums.txt`（SHA-256）校验；
   任何一步失败就退出，最终安装目录完全不动；
4. 安装二进制（临时文件 + `mv`，避免覆盖正在运行的二进制）、原子替换 `data.json`、复制立绘；
5. 处理 PATH（幂等 `grep -Fqs` + 标记注释）；
6. 自检：`mikuji --version` 比对版本，`mikuji --list` 核对签池条数（`--list` 只需要 `data.json`，
   是验证数据部署的最小端到端检查，替代 yakumo 的 `init`）。

### 本地包模式

脚本同目录（或仓库 `assets/`）存在 `mikuji` / `data.json` 时优先用本地文件，缺什么下什么。
`curl | sh` 时 `$0` 是 `sh`，此时只有在当前目录真的存在名为 `$0` 的文件才会启用本地模式，
避免把当前目录误当成发布包。

### 旧版包回退

v0.5.0 及更早的资产名是 `mikuji-vX.Y.Z-x86_64-linux.tar.gz` / `-arm64-macos.tar.gz` 整包
（二进制 + 数据同包）。新命名 404 时回退到旧命名，并复用整包内的 `data.json` 与 `images/`，
使用户在 CI 新包发布前也能一键安装。

过渡期注意：旧版整包里没有单独的 `data.json` 资产，所以在这一类 release 上 `--no-images`
仍会下载整个 234 MB 包（只是不安装立绘）。等第一个带 `data-vN` 的版本发布后，
`--no-images` 才真正只下约 292 KB。

### 自测

`scripts/install_test.sh` 用 `python3 -m http.server` 提供伪造 release，覆盖 64 项断言：
默认目录解析、纯文字/全量安装、幂等、`--force`、校验失败中止、数据缺失、旧版回退、
本地离线模式、环境变量等价、PATH 幂等、卸载与 purge、帮助与非法参数。全程不访问 GitHub。

## 4. 已知取舍与后续

- **立绘 224 MB 是硬伤**。PNG 已压缩，gzip 几乎无收益。中长期可考虑重编码为 WebP/AVIF
  （需要改 `image` 依赖的 feature 与 `src/image.rs`），可降到几十 MB 量级。
- **国内下载**：脚本支持 `MIKUJI_BASE_URL` 指向镜像/代理，但版本解析仍走 github.com，
  完全无法访问 GitHub 时需同时用 `MIKUJI_VERSION` 固定版本。
- **`--from-source` 代价高**：`cargo install --git` 会 clone 含 224 MB 立绘的仓库。
- **未启用 fmt/clippy 门禁**：当前代码存在 `cargo fmt --check` 差异与 clippy 告警，
  清理后可加入 CI（`ci.yml` 里已留 ShellCheck 非门禁步骤）。
- **数据 tag 操作顺序**：首次发布须先推 `data-v1` 再宣布，否则新命名的二进制包会找不到数据；
  旧版整包回退路径不受影响。
