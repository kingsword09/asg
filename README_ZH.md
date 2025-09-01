# ASG - Asciinema SVG Generator

将 Asciinema 录制文件（`.cast`）转换为动画 SVG 文件的 Rust 命令行工具。

English documentation: see README.md

## 特性

- 🎬 支持 Asciicast v2 格式
- 🌐 支持本地文件、远程 URL 与 Asciinema 录制 ID
- 🎨 完整的 ANSI 颜色与文本样式支持（按单元格渲染前景/背景色，支持粗体/斜体/下划线）
- ⚡ 高性能终端模拟器（基于 `vte`）
- 📦 生成独立的动画 SVG 文件（无需额外资源）
- 🔧 可自定义字体、字号、行高、主题与留白

## 安装

### 通过 npm 安装（推荐）

ASG 提供 npm 包 `asg-cli`，安装后命令名为 `asg`。

- 全局安装：

```bash
npm i -g asg-cli
# 然后
asg --help
```

- 临时一次性使用（无需全局安装）：

```bash
npx -p asg-cli asg --help
```

说明：
- 包名是 `asg-cli`，但安装的命令叫 `asg`（来自 package.json 的 `bin` 字段）。
- 目前未在 crates.io 发布可直接 `cargo install asg` 的预构建二进制/动态库；如需原生二进制，请参考下方“从源码构建”。

### 从源码构建

```bash
# 克隆仓库
git clone https://github.com/kingsword09/asg.git
cd asg

# 构建发布版本
cargo build --release

# 安装到系统（可选）
cargo install --path .
```

### 依赖项

- Rust（stable 渠道）
- Cargo 包管理器

## 使用方法

### 基本用法

```bash
# 转换本地 .cast 文件
asg examples/demo.cast examples/demo.svg

# 指定输出文件名（第二个位置参数）
asg demo.cast output.svg

# 从 asciinema.org 下载并转换（使用录制 ID）
asg 113643 output.svg

# 使用自定义字体
asg demo.cast output.svg --font-family "JetBrains Mono,Monaco,Consolas,Liberation Mono,Menlo,monospace"
```

### 命令行参数

```
USAGE:
    asg <INPUT> <OUTPUT> [OPTIONS]

ARGS:
    <INPUT>     Path to .cast file, URL, or remote ID (e.g., '113643')
    <OUTPUT>    Output SVG file path

OPTIONS:
        --theme <THEME>              Select color theme (or provide comma-separated hex colors)
        --speed <SPEED>              Adjust playback speed [default: 1.0]
        --fps <FPS>                  Frames per second [default: 30]
        --font-family <FONT_FAMILY>  Font family for the terminal text
                                     [default: JetBrains Mono,Monaco,Consolas,Liberation Mono,Menlo,monospace]
        --font-size <PX>             Font size in pixels [default: 14]
    -i, --idle-time-limit <SECS>     Idle time limit in seconds
        --cols <COLS>                Override terminal width (columns)
        --rows <ROWS>                Override terminal height (rows)
        --font-dir <DIR>             Path to a directory containing font files
        --no-loop                    Disable animation loop
        --line-height <FLOAT>        Line height [default: 1.4]
        --at <SECS>                  Timestamp of frame to render (static image)
        --from <SECS>                Lower range of timeline to render
        --to <SECS>                  Upper range of timeline to render
        --no-cursor                  Disable cursor rendering
        --window                     Render with window decorations
        --padding <PX>               Distance between text and image bounds [default: 10]
        --padding-x <PX>             Override padding on x axis
        --padding-y <PX>             Override padding on y axis
        --timeline <MODE>            Timeline mode: original|fixed [default: original]
    -v, --verbose                    Verbose output (-v, -vv, -vvv)
    -h, --help                       Print help information
```

## 架构设计

ASG 采用模块化架构，主要包含以下模块：

- `src/input.rs` — 输入处理层：统一处理本地文件、URL、远程 ID
- `src/asciicast.rs` — Asciicast v2 格式解析器
- `src/terminal.rs` — 基于 `vte` 的终端模拟器（解析 ANSI/SGR，生成帧）
- `src/renderer.rs` — SVG 渲染与 CSS 动画生成
- `src/main.rs` — CLI 入口与整体流程协作

### 数据流

```
输入（文件/URL/ID）
    ↓
输入处理器
    ↓
.cast 数据流 (NDJSON)
    ↓
Asciicast 解析器
    ↓
终端模拟器（VTE）
    ↓
帧序列（包含单元格 fg/bg 与样式）
    ↓
SVG 渲染器
    ↓
动画 SVG 文件
```

## 技术要点

### 终端模拟

- 使用 `vte` 解析 ANSI 转义序列
- 完整支持 SGR 参数：前景/背景色（标准/亮色/256 色/真彩），粗体、斜体、下划线
- 支持光标移动、清屏等控制序列

### SVG 渲染

- 使用 CSS Keyframes 与每帧不透明度切换实现动画
- 按行分组：保证背景矩形绘制在文字下方
- 背景：按连续相同背景色的单元格合并为 `<rect>` 以减少元素数量
- 文本：按（前景色 + 样式）分组；对 `<text>` 设置 `font-weight`/`font-style`/`text-decoration`
- 支持可配置的字体、字号、行高与留白（padding）

### 性能优化

- 流式解析：逐条事件处理（无需一次性加载完整文件）
- 只记录状态变化，减少内存与输出体积
- 合并背景与文本运行，控制 SVG 元素数量

## 示例

### 本地文件转换

```bash
# 录制终端会话
asciinema rec demo.cast

# 转换为 SVG
asg demo.cast demo.svg

# 浏览器打开
open demo.svg
```

### 远程录制转换

```bash
# 从 asciinema.org 获取录制（ID）
asg 113643 terminal-demo.svg
```

## 贡献

欢迎提交 Issue 和 Pull Request！

### 开发

```bash
# 运行测试
cargo test

# 静态检查
cargo clippy

# 格式化
cargo fmt
```

## 相关项目

- [asciinema](https://github.com/asciinema/asciinema) — 终端录制工具
- [svg-term-cli](https://github.com/marionebl/svg-term-cli) — JavaScript 实现的 SVG 生成器
- [agg](https://github.com/asciinema/agg) — Asciinema GIF 生成器

## 许可证

Apache-2.0 License

## 作者

Kingsword <kingsword09@gmail.com>
