# ASG v3 重写设计与验收报告

本文记录本次从零重写的依据、边界和可量化验收项。目标不是继续修补旧实现，而是建立一条只面向 asciicast v3、行为明确、输出紧凑的转换流水线。

## 1. 旧实现的根因分析

旧实现的问题不是单个 renderer bug，而是数据模型、终端状态和输出模型同时不适合 v3。

### 1.1 格式与时间轴错误

- header 固定读取 v2 的顶层 `width` / `height`，而 v3 使用 `term.cols` / `term.rows`。
- 旧解析器只允许 `version == 2`。
- v2 事件时间是绝对时间；v3 第一项是与上一事件之间的增量。仅修改版本判断会把 v3 时间重复当成绝对时间，动画顺序和时长都会错误。
- v3 的 `m` marker、`x` exit 和未知事件没有完整领域模型。
- 未知事件被当成 output，会把不应送入终端的数据画到屏幕。
- 单行解析失败只记录 warning 后继续，损坏的录制会悄悄生成“看起来成功”的错误 SVG。
- header 自带 `idle_time_limit` 没有进入时间变换。

### 1.2 终端行为不完整

- 旧代码在 `vte` parser 之上自行实现网格和控制序列语义，光标、滚动、插入/删除、备用屏、宽字符、DEC 模式和 resize 重排都需要自行维护。
- resize 虽然能被解析，却没有真正改变终端状态。
- 为过滤 OSC 而在 main 中删除整个 output 事件，会连带删除同一事件中有效的提示符或控制序列；系统控制消息应由终端 parser 消费，而不是在录制层猜测并删除。
- UTF-8 清理逻辑按字节转 `char`，会破坏非 ASCII 字符。

### 1.3 SVG 体积失控

- 每个事件复制一次完整屏幕，即使事件是 input、marker 或无视觉变化的控制序列。
- fixed FPS 通过重复完整 frame 实现，帧数随时长线性膨胀。
- 每帧、每行都包含多层空 `<g>`。
- 颜色和样式逐元素重复写入。
- 每帧各自包含一个 SMIL `<animate>`，形成长串依赖。
- 输出库默认保留大量格式化换行与属性冗余。

仓库 demo 的 58,091 字节 v2 输入曾生成 5,309,409 字节 SVG，说明主要开销来自输出模型，而不是输入本身。

### 1.4 小数坐标与缩放导致文字发糊

旧 renderer 为了复刻 svg-term，把 1080px 物理画布映射到 108 单位的内部 `viewBox`。16.7px 字号变成 1.67 内部单位，行高变成 2.171，大量文本起点和基线落在小数像素上。浏览器先放大内部文本层，预览器或页面再按容器缩放，边缘会经过多次插值。回退字体的实际 advance 与固定 0.6em 列宽不完全一致，也会让长文本逐渐偏离单元格。

## 2. 本次范围与非目标

本次明确实现：

- 只接受 asciicast v3。
- 正确解析和累计 v3 相对时间。
- 支持 plain 与 zstd 输入。
- 使用可靠终端状态机处理 output 和 resize。
- input、marker、exit、unknown 参与时间轴但不伪造画面。
- 支持静态 seek、范围、speed、idle limit 和 FPS cap。
- 默认 10px 列宽保持与 svg-term-cli 相近的 intrinsic width，并使用 1:1 像素原生几何。
- 输出体积不高于 svg-term-cli 同等样例。
- 原生和 WASI 构建保持可用。

刻意不实现：

- v1/v2 兼容层；旧文件交给 `asciinema convert`。
- GIF、视频、音频或浏览器播放器。
- marker UI；marker 只作为时间轴事件保留。
- 字体文件扫描/嵌入。
- 实时流式 SVG 更新。SVG 生成前需要知道完整时间轴、最大 resize 画布和可复用行。

这些非目标遵循 YAGNI：它们不是“正确转换 v3 为紧凑 SVG”的必要条件。

## 3. 新数据流

```text
file / stdin / HTTP / recording ID
                │
                ▼
       plain / zstd 自动识别
                │
                ▼
      v3 header + 增量事件严格解析
                │
                ▼
       speed / idle 时间轴变换
                │
                ▼
       avt 终端 feed / resize
                │
                ▼
     视觉快照去重 + FPS 时间窗合并
                │
                ▼
       at / from / to 状态选择
                │
                ▼
   行注册表 + 样式注册表 + 横向 reel
                │
                ▼
            compact SVG
```

每一层只有一个主要职责，解析器不关心 SVG，终端不关心时间范围，renderer 不重新解释 ANSI。

## 4. v3 语义

### 4.1 Header

必需字段：

```json
{"version":3,"term":{"cols":80,"rows":24}}
```

同时读取 `term.type`、`term.version`、`term.theme`、`timestamp`、`idle_time_limit`、`command`、`title`、`env`。主题 palette 必须包含 8 或 16 个 `#RRGGBB`；8 色按 asciinema 行为复制成 16 色。

零尺寸、负/非有限 idle limit、错误颜色和错误 palette 均直接失败。

### 4.2 Event

v3 行格式仍为三元 JSON array，但第一项是 delta：

```text
absolute_time[n] = absolute_time[n-1] + delta[n]
```

| code | 解析结果 | 是否进入终端 | 是否可能建视觉帧 |
|---|---|---:|---:|
| `o` | Output | 是 | 是 |
| `i` | Input | 否 | 否 |
| `r` | Resize(COLS, ROWS) | 是 | 是 |
| `m` | Marker | 否 | 否 |
| `x` | Exit(status) | 否 | 否 |
| 其他 | Other(code, data) | 否 | 否 |

所有事件都会推进 duration。这样最后一个 output 到 exit 之间的停留时间不会丢失。

损坏事件会携带具体行号中止，不再跳过后继续。

## 5. 终端模型

终端层使用 asciinema 生态的 `avt` 0.18（Apache-2.0）。ASG 只保留三个操作：

- `feed(data)`
- `resize(cols, rows)`
- `snapshot(cursor_enabled)`

这样复用已经经过性质测试的 ANSI/DEC、备用屏、滚动、Unicode 宽度和 resize 语义，删除旧代码中大量重复且不完整的控制序列实现，体现 DRY 与 KISS。

resize 会真实触发 reflow。SVG 根尺寸不能在动画期间改变，所以画布使用实际出现过的最大列/行；若提供 `--cols` 或 `--rows`，对应轴始终固定。

## 6. 时间线模型

时间变换顺序固定为：

1. 计算当前 v3 event 与上一 event 的源时间间隔。
2. 使用 CLI idle limit；未提供时使用 header idle limit。
3. 对间隔应用 idle cap。
4. 除以 speed。
5. 累计为输出时间。
6. 应用事件到终端。
7. 仅对 output/resize 获取快照。

同一时刻的多个视觉事件只保留最后状态；与上一帧视觉完全相同的状态不输出。

FPS 不是固定采样器。每个 `1/fps` 窗口只保留最新视觉状态，从而限制事件爆发时的帧数，但不会为静止区间制造重复帧。

`--at` 在 FPS 合并前选择精确状态，避免 10ms 后的画面因 30 FPS 量化被错误提前到 0ms。range 会在起点合成当时已有状态并把时间重置为 0。

## 7. SVG 编码模型

### 7.1 几何

默认值参考 agg 的整数像素策略：

```text
font_px         = round(16) = 16
column_px       = round(font_px × 0.6) = 10
line_height     = 1.4
row_px          = round(font_px × line_height) = 22
content_width   = cols × column_px
content_height  = rows × row_px
```

根 SVG 和裁剪层各自使用与物理尺寸相同的 `viewBox`，不再做 10 倍嵌套缩放。文本基线、行偏移、背景、光标和 frame offset 都落在整数像素。字体栈参考 agg，关闭 kerning、ligature、optical sizing 和 font synthesis，并在 macOS 浏览器请求灰阶 antialiasing；宽字符单独输出，使后续 run 从准确终端列重新开始。padding 会舍入到整数像素，window decoration 保持原有布局。

浏览器可能在首个可用正文字体缺少符号时直接跳到彩色 emoji，而 agg 会先尝试内置 Symbols Nerd Font。renderer 因此把系统 symbol family 放在 emoji family 前，并对 Unicode 16.0 同时具有 text/emoji 标准变体、默认呈现为 emoji 的字符补充 VS15（显式 VS16 会原样保留）。这样 `⚡`、时钟等终端状态符号与 agg 一样优先单色显示，真正的彩色 emoji 仍可回退到系统 emoji 字体。

实际 demo 高频使用 `─│┌┐└┘├┤┬┴┼` 与 `█▀▄`。这些字符不再作为相邻字体 glyph 输出，而是转换为按单元格合并的 SVG path；同一行同一样式只使用一个 path，既消除缩放后的线段接缝，也控制额外体积。

该策略能保留矢量 SVG 的紧凑性并显著改善常见浏览器的清晰度，但不会承诺跨设备逐像素一致：本地字体缺失时浏览器仍会使用 fallback。完全复刻 agg 需要嵌入或栅格化字体，超出当前体积目标，遵循 YAGNI 暂不实现。

### 7.2 Reel

所有 frame 横向排列，第 `n` 帧位于 `x = n × content_width`。内层 SVG 负责裁剪，唯一的 `.r` group 通过 `steps(1,end)` CSS keyframes 按整画布像素离散平移。

相较逐帧 opacity + SMIL：

- 只有一个 animation declaration。
- keyframe 只记录时间百分比与横向 offset。
- 不需要为每帧生成 ID、begin 链和 animate 元素。

### 7.3 行与样式复用

renderer 先把终端行转换成与位置无关的结构：背景 run 与文本 run。相同行出现两次以上时只在 `<defs>` 写一次，frame 使用 `<use href="#lN" y="...">`。

文本样式由颜色和 SGR 属性组成；默认样式继承父 `<g>`，非默认组合注册为短 CSS class。背景相邻同色 cell 合并为单个 rect。

该策略保持 svg-term 的核心 DRY 思路，同时避免 React/Emotion/SVGO 中间层，直接生成合法紧凑 XML。

## 8. 量化结果

基准使用仓库当前的真实 108×32 v3 demo。`svg-term-cli` 2.1.1 无法读取 v3，因此不能对该输入做直接同源转换；这里同时报告清晰度改造前后的 ASG 结果。

| 项目 | bytes | 帧数 | intrinsic size |
|---|---:|---:|---:|
| v3 cast 输入 | 720,855 | - | 108×32 cells |
| 小数几何 SVG | 252,731 | 218 | 1080×694.72 |
| 像素原生 SVG | 290,902 | 218 | 1080×704 |

像素原生 SVG / cast 输入 = 40.36%。相对小数几何版本增加 15.10%，主要是宽字符隔离和终端图形的确定性 path；帧数与时间轴未变化。与仅做整数几何的上一版 283,941 bytes 相比，新增符号/box-drawing 质量修复只增加 2.45%。

质量验收使用本机 Menlo 16px，在 demo 的 10s、60s、100s、120s 分别生成 ASG 静态 SVG 与 agg 1.9.0 Swash 单帧。普通文本字框分别约为 159×14 与 153×12；ASG 保留浏览器完整灰阶，agg GIF 默认量化为 5 个有效边缘灰阶。原生尺寸与缩放到约 74% 的 Chrome 预览均人工检查了 Latin、CJK、emoji、block 与 box-drawing 连续性。

官方 asciinema `full-v3.cast`：

- plain 与 zstd 输入输出逐字节一致；
- 4 visual frames；
- duration 13.400002s；
- 100×50 canvas；
- SVG 1,164 bytes。

## 9. 验证矩阵

自动测试覆盖：

- header、relative time、comment、null metadata、unknown event、错误行号、v2 拒绝
- 8 色 palette 扩展、256 色 cube/grayscale、自定义主题错误
- ANSI style、宽字符、隐藏光标、resize
- header idle、speed、FPS、range、at、维度 override
- XML escaping、静态 SVG、动画 keyframes、line registry、体积预算
- 1:1 viewBox、整数文字/动画坐标、自定义字号、宽字符边界、Unicode text-symbol、box/block path、window/padding 尺寸

发布前命令：

```bash
cargo test --workspace
cargo clippy --tests --all-features --all-targets --workspace -- -D warnings
cargo build -p asg --target wasm32-wasip2 --release
```

另外使用系统 SVG renderer 对官方 v3 和 demo 静态时刻进行了栅格化检查，确认 XML、裁剪、文本基线、Unicode 与背景可被实际渲染。

## 10. 原则落地

- **KISS**：一条 v3-only 路径；直接 compact XML；删除 v2 分支、fixed-frame 复制和手写 ANSI 语义。
- **YAGNI**：不保留“未来也许支持”的 v1/v2、字体扫描、视频等接口。
- **SRP**：input、parser、terminal、timeline、renderer、CLI 各自独立。
- **OCP/DIP**：时间线依赖稳定的事件/快照模型，renderer 不依赖 parser 细节；终端行为依赖 `avt` 抽象边界。
- **LSP/ISP**：公开库入口只要求 `BufRead`，文件、网络、zstd 和内存输入均可替换；终端适配只暴露所需三个操作。
- **DRY**：重复行、样式、背景 run 复用；颜色解析和主题解析集中；CLI 与 renderer 共用默认字体常量，不重复时间或默认配置逻辑。

## 11. 后续建议

当前实现已满足 v3-to-SVG 主目标。后续迭代应由真实需求驱动，建议优先级如下：

1. 建立更多公开 v3 录制的视觉 golden corpus，固定静态时间点做像素差异阈值测试。
2. 在 CI 中增加 Node 版本矩阵和 npm CLI smoke test；WASI release build 已纳入当前 CI。
3. 对超长录制增加 benchmark，观察 line registry 和 snapshot 内存峰值；只有出现真实瓶颈时再引入 interning/流式外存。
4. 如需要完全复刻 svg-term-cli 的毫秒 CLI，可单独增加兼容子命令，避免模糊当前以秒为单位的 API。
