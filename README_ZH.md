# ASG — asciicast v3 转 SVG

ASG 是一个 Rust CLI 与库，**只解析 asciicast v3**，将录制转换为紧凑、独立、可直接嵌入 README 的动画 SVG。本次实现已经抛弃原有 v2 数据模型、手写终端状态机和逐帧 SMIL 输出方案，并以 `svg-term-cli` 的画布几何与紧凑 reel 结构为兼容基准。

详细设计、旧实现问题与验收依据见 [docs/architecture.md](docs/architecture.md)。

## 已支持范围

- v3 嵌套 `term` 头信息、8/16 色终端主题
- v3 相对时间间隔及正确的累计时间轴
- `o` 输出、`i` 输入、`r` resize、`m` marker、`x` exit 与未知事件
- 普通 `.cast` 和 zstd 压缩 `.cast.zst`
- 本地文件、stdin、HTTP(S) URL、asciinema.org 录制 ID
- 基于 asciinema `avt` 的 ANSI/DEC 终端模拟
- 16/256/真彩色、反色、粗体、淡化、斜体、下划线、删除线、闪烁、宽字符、备用屏、光标与 resize
- speed、header/CLI idle limit、FPS 上限、静态帧、时间范围、主题、padding、窗口装饰
- 原生 Rust 与 `wasm32-wasip2`

本项目有意拒绝 v1/v2，避免把 v2 绝对时间错误地当成 v3 增量时间。旧录制可先转换：

```bash
asciinema convert old.cast recording-v3.cast
```

## 构建与使用

```bash
cargo build --release -p asg

asg recording.cast recording.svg
asg recording.cast.zst recording.svg
cat recording.cast | asg - - > recording.svg

# 时间单位为秒
asg recording.cast still.svg --at 4.5
asg recording.cast excerpt.svg --from 3 --to 12

asg recording.cast window.svg --window --no-cursor
```

npm/WASI 版本安装后提供相同命令：

```bash
npm install -g @kingsword/asg
```

主要参数：

```text
--speed <N>                  播放倍速
--fps <N>                    每秒最大视觉帧数，默认 30
--idle-time-limit <SECONDS>  覆盖 v3 header 的空闲时间限制
--cols/--width <N>           固定终端列数
--rows/--height <N>          固定终端行数
--font-size <PX>             输出字号，默认 16.7
--line-height <N>            行高倍数，默认 1.3
--padding[-x|-y] <PX>        输出留白，默认 0
--theme <NAME|COLORS>        命名主题或 18 色自定义主题
--no-cursor --no-loop --window
```

## 与 svg-term-cli 的尺寸和体积对齐

默认几何严格使用 svg-term 坐标体系：

- 宽度 = `列数 × 10px`
- 高度 = `行数 × 16.7px × 1.3`
- padding = `0px`
- 未提供录制主题时使用 svg-term 的 Atom One 配色

仓库中语义等价的 80×16 demo 实测：

| 生成器 | SVG 字节数 | 画布 |
|---|---:|---:|
| 旧 ASG | 5,309,409 | 默认尺寸不一致 |
| `svg-term-cli` 基准 | 791,872 | 800×347.36 |
| 新 ASG | 364,504 | 800×347.36 |

新实现对该样例比 svg-term-cli 小约 54%，比旧 ASG 小约 93%。不同录制的结果会随画面变化频率而变化。

体积优化没有依赖不可维护的字符串后处理，而是在模型层完成：

- 只为真正改变屏幕的 output/resize 建帧；input/marker/exit 只推进时间
- 相同视觉状态去重
- FPS 是“合并时间窗”，不再复制整屏制造固定帧
- 重复行注册到 `<defs>`，每帧用 `<use>` 引用
- 文本样式共用 CSS class
- 所有帧横向排列，仅用一个离散 CSS reel 动画切换
- SVG 直接紧凑编码，不再生成大量空 `<g>` 和格式化空白

## 模块边界

- `asciicast.rs`：v3 严格解析与元数据校验
- `terminal.rs`：`avt` 的最小适配层
- `timeline.rs`：时间变换、resize、视觉去重、FPS、range/at
- `renderer.rs`：紧凑 SVG、行/样式注册表、reel 动画
- `input.rs`：本地/stdin/HTTP、zstd 自动识别、输出
- `lib.rs`：库级编排和主题优先级
- `main.rs`：仅负责 CLI

主题优先级为：CLI/API 显式主题 > v3 header 主题 > svg-term 默认主题。

resize 会在事件发生时真实作用于终端并执行重排。由于 SVG 根画布不能在动画中改变固有尺寸，最终画布取录制过程中出现过的最大终端尺寸；`--cols` / `--rows` 可以固定对应轴。

## 验证

```bash
cargo test --workspace
cargo clippy --tests --all-features --all-targets --workspace -- -D warnings
cargo build -p asg --target wasm32-wasip2 --release
```

## 许可证

Apache-2.0；`avt` 同样使用 Apache-2.0。
