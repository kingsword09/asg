# ASG — asciicast v3 to SVG

ASG is a Rust CLI and library that converts **asciicast v3 only** recordings into compact, self-contained animated SVG files. It is a from-scratch replacement for the previous v2 implementation and targets the output geometry and compact reel model of `svg-term-cli` while supporting asciinema 3 recordings.

Chinese documentation: [README_ZH.md](README_ZH.md). Design details and measured trade-offs: [docs/architecture.md](docs/architecture.md).

## What is supported

- asciicast v3 headers, including nested `term` metadata and 8/16-color themes
- v3 relative event intervals (accumulated into an absolute playback timeline)
- output, input, resize, marker, exit, and unknown events
- plain `.cast` and zstd-compressed `.cast.zst` input
- local files, stdin, HTTP(S) URLs, and asciinema.org recording IDs
- ANSI/DEC terminal behavior via asciinema's `avt` virtual terminal
- 16/256/truecolor, inverse, bold, faint, italic, underline, strikethrough, blink, wide Unicode, alternate screen, cursor visibility, and terminal resize
- animated ranges, static frames, speed, idle limiting, FPS capping, themes, padding, and window decorations
- native Rust and `wasm32-wasip2` builds

ASG intentionally rejects asciicast v1 and v2. Convert old recordings first:

```bash
asciinema convert old.cast recording-v3.cast
```

## Install and build

```bash
cargo build --release -p asg
```

The npm/WASI package installs the same `asg` command:

```bash
npm install -g @kingsword/asg
```

## Usage

```bash
# Local v3 recording
asg recording.cast recording.svg

# zstd-compressed v3 recording
asg recording.cast.zst recording.svg

# stdin/stdout
cat recording.cast | asg - - > recording.svg

# Static frame and animated range (times are seconds)
asg recording.cast still.svg --at 4.5
asg recording.cast excerpt.svg --from 3 --to 12

# svg-term-compatible dimensions with decorations
asg recording.cast window.svg --window --no-cursor
```

Run `asg --help` for the complete option list. Useful options include:

```text
--speed <N>                  playback speed multiplier
--fps <N>                    maximum visual frames per second (default 30)
--idle-time-limit <SECONDS>  override the v3 header idle limit
--cols/--width <N>           pin terminal columns
--rows/--height <N>          pin terminal rows
--font-size <PX>             output font size (default 16.7)
--line-height <N>            line-height multiplier (default 1.3)
--padding[-x|-y] <PX>        output padding (default 0)
--theme <NAME|COLORS>        named or custom 18-color theme
--no-cursor --no-loop --window
```

Named themes are `svg-term`, `atom-one`, `asciinema`, `dracula`, `github-dark`, `github-light`, `monokai`, `solarized-dark`, and `solarized-light`.

## svg-term geometry and size

The defaults deliberately use svg-term's coordinate system:

- width = `columns × 10 px`
- height = `rows × 16.7 px × 1.3`
- default padding = `0 px`
- default unthemed palette = svg-term's Atom One palette

For the repository's equivalent 80×16 demo recording:

| Generator | SVG bytes | Canvas |
|---|---:|---:|
| Previous ASG implementation | 5,309,409 | mismatched defaults |
| `svg-term-cli` reference | 791,872 | 800×347.36 |
| Rewritten ASG | 364,504 | 800×347.36 |

The new encoder is about 54% smaller than the reference and 93% smaller than the old ASG output for this sample. Results vary with terminal activity.

The reduction comes from emitting only visual changes, capping rather than duplicating frames, reusing identical lines through `<defs>/<use>`, sharing style classes, and moving one horizontal SVG reel with a single discrete CSS animation.

## Architecture

- `asciicast.rs` — strict, v3-only parser and metadata validation
- `terminal.rs` — small boundary around `avt`
- `timeline.rs` — timing transforms, resize handling, visual deduplication, FPS cap, range/static selection
- `renderer.rs` — compact SVG model, line registry, style registry, and reel animation
- `input.rs` — file/stdin/HTTP input, zstd detection, and output writing
- `lib.rs` — library orchestration and theme precedence
- `main.rs` — CLI only

Theme precedence is CLI/API override, then the v3 header theme, then the svg-term default.

For resize events, terminal reflow is applied at the event time. Since an SVG root cannot change intrinsic dimensions during playback, the output canvas uses the largest observed terminal size; `--cols` and `--rows` pin either axis.

## Verify

```bash
cargo test --workspace
cargo clippy --tests --all-features --all-targets --workspace -- -D warnings
cargo build -p asg --target wasm32-wasip2 --release
```

## License

Apache-2.0. The `avt` dependency is also Apache-2.0.
