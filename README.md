# ASG - Asciinema SVG Generator

将 Asciinema 录制文件（`.cast`）转换为动画 SVG 文件的 Rust 命令行工具。

## 特性

- 🎬 支持 Asciicast v2 格式
- 🌐 支持本地文件和远程录制 ID
- 🎨 完整的 ANSI 颜色和样式支持
- ⚡ 高性能的终端模拟器（基于 vte）
- 📦 生成独立的动画 SVG 文件
- 🔧 可自定义字体和样式

## 安装

### 从源码构建

```bash
# 克隆仓库
git clone https://github.com/yourusername/asg.git
cd asg

# 构建发布版本
cargo build --release

# 安装到系统（可选）
cargo install --path .
```

### 依赖项

- Rust 1.70.0 或更高版本
- Cargo 包管理器

## 使用方法

### 基本用法

```bash
# 转换本地 .cast 文件
asg demo.cast

# 指定输出文件名
asg demo.cast -o output.svg

# 从 asciinema.org 下载并转换
asg 113643

# 使用自定义字体
asg demo.cast --font "Monaco, monospace"
```

### 命令行参数

```
USAGE:
    asg [OPTIONS] <INPUT>

ARGS:
    <INPUT>    Path to .cast file or remote ID (e.g., '113643')

OPTIONS:
    -o, --output <OUTPUT>     Output SVG file path
    -f, --font <FONT>         Font family for the terminal text 
                             [default: "Consolas, Monaco, 'Courier New', monospace"]
    -s, --server <SERVER>     Asciinema server URL (for remote recordings) 
                             [default: https://asciinema.org]
    -v, --verbose            Verbose output
    -h, --help               Print help information
```

## 架构设计

ASG 采用模块化架构设计，主要包含以下模块：

### 核心模块

- **input.rs** - 输入处理层，统一处理本地文件和远程 URL
- **asciicast.rs** - Asciicast v2 格式解析器
- **terminal.rs** - 基于 vte 的终端模拟器
- **renderer.rs** - SVG 生成和 CSS 动画渲染
- **main.rs** - CLI 入口和工作流协调

### 数据流

```
输入（文件/ID）
    ↓
输入处理器
    ↓
.cast 数据流
    ↓
Asciicast 解析器
    ↓
终端模拟器（VTE）
    ↓
状态快照序列
    ↓
SVG 渲染器
    ↓
动画 SVG 文件
```

## 技术特点

### 终端模拟

- 使用 `vte` crate 实现高性能、标准兼容的 ANSI 转义序列解析
- 支持完整的 SGR（Select Graphic Rendition）参数
- 实现光标控制、屏幕清除等终端操作

### SVG 生成

- 使用 CSS Keyframes 实现流畅的动画效果
- 优化文件体积：相同样式的文本共享 CSS 类
- 支持字体自定义和主题颜色

### 性能优化

- 流式处理：逐行解析 NDJSON 格式，无需加载整个文件
- 智能状态跟踪：只记录变化的单元格
- 高效的样式合并：减少 DOM 元素数量

## 示例

### 本地文件转换

```bash
# 录制终端会话
asciinema rec demo.cast

# 转换为 SVG
asg demo.cast

# 在浏览器中查看
open demo.svg
```

### 远程录制转换

```bash
# 从 asciinema.org 获取录制
asg 113643 -o terminal-demo.svg

# 使用自定义服务器
asg 12345 --server https://my-asciinema-server.com
```

## 贡献

欢迎提交 Issue 和 Pull Request！

### 开发

```bash
# 运行测试
cargo test

# 检查代码
cargo clippy

# 格式化代码
cargo fmt
```

## 相关项目

- [asciinema](https://github.com/asciinema/asciinema) - 终端录制工具
- [svg-term-cli](https://github.com/marionebl/svg-term-cli) - JavaScript 实现的类似工具
- [agg](https://github.com/asciinema/agg) - Asciinema GIF 生成器

## 许可证

MIT License

## 作者

Your Name <your.email@example.com>
