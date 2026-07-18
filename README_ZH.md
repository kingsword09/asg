# ASG

> 将 asciicast v3 录制转换为清晰、紧凑的动画 SVG。

[![CI](https://github.com/kingsword09/asg/actions/workflows/ci.yml/badge.svg)](https://github.com/kingsword09/asg/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/asg.svg)](https://crates.io/crates/asg)
[![npm](https://img.shields.io/npm/v/%40kingsword%2Fasg.svg)](https://www.npmjs.com/package/@kingsword/asg)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

[English](README.md)

![ASG 生成的终端动画 SVG](https://raw.githubusercontent.com/kingsword09/asg/main/examples/demo.svg)

_使用 `--window` 从 [asciinema/agg 的 v3 demo 录制](https://github.com/asciinema/agg/blob/e7b0e3bd597798734a86669d7118eef7799da2ab/demo.cast)生成；[来源与授权说明](examples/README.md)。_

## 为什么选择 ASG？

- **原生解析 asciicast v3。** 正确处理 v3 header、相对事件时间、resize、录制内主题以及 zstd 压缩输入。
- **适合直接放进 README。** 输出是一个自包含 SVG，不需要 JavaScript、逐帧位图或外部播放器。
- **缩放仍然清晰。** 像素原生几何、整数文本基线和终端图形 SVG path，避免常见的文字发糊与线段接缝。
- **两种发行方式，一套 CLI。** 可使用原生 Rust 二进制，也可通过 npm/WASI 在 Node.js 中直接运行。

## 30 秒上手

使用 npm 直接运行：

```bash
npx --yes @kingsword/asg@latest recording.cast recording.svg --window
```

或安装任意一种发行版：

```bash
# 原生 Rust 二进制
cargo install asg --locked

# Node.js / WASI CLI
npm install -g @kingsword/asg
```

两种安装方式都会提供相同的 `asg` 命令：

```bash
asg recording.cast recording.svg --window
```

## 应该选择哪个终端渲染工具？

| 工具 | 输出 | asciicast 支持 | 最适合 |
|---|---|---|---|
| **ASG** | 动画 SVG | **仅 v3** | 在 GitHub、npm 和技术文档中展示清晰、紧凑的终端动画 |
| [svg-term-cli](https://github.com/marionebl/svg-term-cli) | 动画 SVG | 旧版录制，不支持 v3 | 已有的 pre-v3 svg-term 工作流 |
| [agg](https://github.com/asciinema/agg) | 动画 GIF | v1、v2、v3 | 字体无关的栅格结果与更广泛的录制兼容性 |
| [asciinema player](https://github.com/asciinema/asciinema-player) | 交互式 HTML/JS | v3 | 需要播放控制的交互式网页 |

如果输入是 asciicast v3，而目标位置只能嵌入图片、不能运行 JavaScript，ASG 是最直接的选择。SVG 文本会使用观看设备上的字体；如果更重视所有设备逐像素一致，而不是矢量缩放和文字清晰度，应选择 agg。

## 输入与兼容范围

ASG 支持：

- 本地 `.cast` 与 zstd 压缩的 `.cast.zst`；
- 通过 `-` 读取 stdin；
- HTTP(S) URL；
- asciinema.org 录制 ID；
- 输出到文件，或通过 `-` 写入 stdout。

终端行为由 asciinema 的 `avt` 虚拟终端实现，包括 16/256/真彩色、文本属性、备用屏、宽字符、光标可见性和终端 resize。

> [!IMPORTANT]
> ASG 有意只接受 asciicast v3。旧录制需要先转换：
>
> ```bash
> asciinema convert old.cast recording-v3.cast
> ```

## 常用场景

```bash
# 本地或压缩录制
asg recording.cast recording.svg
asg recording.cast.zst recording.svg

# stdin/stdout
cat recording.cast | asg - - > recording.svg

# 直接读取远程录制
asg https://example.com/recording.cast recording.svg --window

# 单个静态画面或动画片段，时间单位为秒
asg recording.cast still.svg --at 4.5
asg recording.cast excerpt.svg --from 3 --to 12

# 调整尺寸、倍速并隐藏光标
asg recording.cast demo.svg --cols 100 --rows 30 --speed 1.5 --no-cursor
```

重新生成 README 演示：

```bash
asg examples/demo.cast examples/demo.svg --window --from 0.1
```

<details>
<summary><strong>完整参数摘要与内置主题</strong></summary>

```text
--speed <N>                  播放倍速
--fps <N>                    每秒最大视觉帧数，默认 30
--idle-time-limit <SECONDS>  覆盖 v3 header 的空闲时间限制
--cols/--width <N>           固定终端列数
--rows/--height <N>          固定终端行数
--font-family <FAMILY>       CSS 字体栈
--font-size <PX>             输出字号，默认 16
--line-height <N>            行高倍数，默认 1.4
--padding[-x|-y] <PX>        输出留白，默认 0
--theme <NAME|COLORS>        命名主题或 18 色自定义主题
--at/--from/--to <SECONDS>   静态画面或动画范围
--no-cursor --no-loop --window
```

内置主题：`svg-term`、`atom-one`、`asciinema`、`dracula`、`github-dark`、`github-light`、`monokai`、`solarized-dark`、`solarized-light`。

以 `asg --help` 的输出作为完整、准确的接口说明。

</details>

## 渲染模型与体积

ASG 使用紧凑的横向 reel，而不是重复编码完整屏幕：

- 只有真正改变画面的事件才生成帧；
- 相同状态和重复行会被复用；
- 文本样式共享 CSS class；
- 只用一个离散 CSS 动画移动 reel；
- box-drawing 和 block 字符使用清晰的原生 SVG path。

上方演示使用 `--window --from 0.1` 生成：

| 输入 | 输出 | 帧数 | 时长 | 画布 |
|---|---:|---:|---:|---:|
| 1,744,897 字节 v3 cast | 986,999 字节 SVG | 146 | 29.964 秒 | 930×544 |

SVG 体积为 cast 输入的 56.56%，同时保持矢量和自包含。默认终端几何为 16px 字号、10px 单元格宽、22px 行高、1:1 物理像素 `viewBox` 和 0 padding。

解析器、时间轴、终端和 renderer 的设计见[架构文档](docs/architecture.md)。Rust 库入口为 `asg::generate`。

## 开发验证

```bash
cargo +nightly x lint
cargo +nightly x test
cargo build -p asg --target wasm32-wasip2 --release
```

## 许可证

ASG 源代码使用 Apache-2.0。演示录制及其生成的 SVG 保留上游 demo 资产的 GPL-3.0-or-later 条款，详见 [examples/README.md](examples/README.md)。
