# ASG — asciicast v3 to SVG

ASG is a Rust CLI and library that converts **asciicast v3 only** recordings into compact, self-contained animated SVG files. It is a from-scratch replacement for the previous v2 implementation. It keeps the compact reel model and 10px default column width familiar from `svg-term-cli`, but uses pixel-native geometry for sharper browser rendering.

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

# Pixel-native terminal with decorations
asg recording.cast window.svg --window --no-cursor
```

Run `asg --help` for the complete option list. Useful options include:

```text
--speed <N>                  playback speed multiplier
--fps <N>                    maximum visual frames per second (default 30)
--idle-time-limit <SECONDS>  override the v3 header idle limit
--cols/--width <N>           pin terminal columns
--rows/--height <N>          pin terminal rows
--font-size <PX>             output font size (default 16)
--line-height <N>            line-height multiplier (default 1.4)
--padding[-x|-y] <PX>        output padding (default 0)
--theme <NAME|COLORS>        named or custom 18-color theme
--no-cursor --no-loop --window
```

Named themes are `svg-term`, `atom-one`, `asciinema`, `dracula`, `github-dark`, `github-light`, `monokai`, `solarized-dark`, and `solarized-light`.

## Rendering clarity and size

The defaults deliberately keep every terminal boundary on the physical pixel grid:

- font size = `16 px`
- cell width = `round(font size × 0.6) = 10 px`
- row height = `round(font size × 1.4) = 22 px`
- width = `columns × 10 px`; height = `rows × 22 px`
- default padding = `0 px`
- default unthemed palette = svg-term's Atom One palette

The root and inner `viewBox` values equal their physical canvases, text baselines and frame offsets are integers, and kerning/ligatures are disabled. An agg-inspired fallback stack starts with JetBrains Mono, Fira Code, and SF Mono, resolves terminal symbols before color emoji, and requests Unicode text presentation where agg would use its symbol font. Common box-drawing and block glyphs are emitted as crisp SVG paths rather than font glyphs. This avoids both the fractional 10× scaling and the small seams commonly seen between terminal line characters.

For the repository's actual 108×32 v3 demo:

| Artifact | Bytes | Canvas |
|---|---:|---:|
| v3 cast input | 720,855 | 108×32 cells |
| Previous fractional SVG | 252,731 | 1080×694.72 |
| Pixel-native SVG | 290,902 | 1080×704 |

The complete clarity pass adds 15.10% to this SVG, mainly from isolating wide glyphs and encoding terminal graphics independently from fonts, but the result remains only 40.36% of the cast input. `svg-term-cli` cannot parse this v3 recording, so a direct conversion of the repository demo is not possible.

The reduction comes from emitting only visual changes, capping rather than duplicating frames, reusing identical lines through `<defs>/<use>`, sharing style classes, and moving one horizontal SVG reel with a single discrete CSS animation.

SVG text still uses fonts installed on the viewer's system. Exact cross-device pixels would require embedding or rasterizing a font, which would substantially increase output size; ASG intentionally keeps the output vector-based and compact. At native size and at a 74% browser preview scale, the repository demo was visually checked against agg 1.9.0 at the same 10s, 60s, 100s, and 120s positions.

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
