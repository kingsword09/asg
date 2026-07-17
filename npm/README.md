# @kingsword/asg

Rust/WASI CLI for converting **asciicast v3 only** recordings to compact animated SVG.

## Install

```bash
npm install -g @kingsword/asg
# or
npx -p @kingsword/asg asg --help
```

## Use

```bash
asg recording.cast recording.svg
asg recording.cast.zst recording.svg
asg recording.cast still.svg --at 4.5
asg recording.cast excerpt.svg --from 3 --to 12
asg recording.cast window.svg --window --no-cursor
```

Input can be a local path, `-` for stdin, an HTTP(S) URL, or an asciinema.org recording ID. Output can be a path or `-` for stdout. Timeline options use seconds.

ASG supports v3 relative timing, terminal resize, v3 header themes, all event codes, zstd input, ANSI/DEC terminal behavior, static/range selection, idle limiting, FPS capping, and sharp pixel-native default geometry.

v1/v2 are intentionally rejected. Convert old recordings first:

```bash
asciinema convert old.cast recording-v3.cast
```

Default geometry uses a 16px font, 10px cells, 22px rows, a 1:1 physical-pixel viewBox, and zero padding. Unicode text-symbol selection and native SVG paths for common terminal graphics provide agg-like clarity, while line/style reuse keeps the SVG compact.

See the repository [README](https://github.com/kingsword09/asg#readme) for all options and architecture details.

The package runs the same Rust binary as a `wasm32-wasip2` component through Node.js.
