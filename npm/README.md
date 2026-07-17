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

ASG supports v3 relative timing, terminal resize, v3 header themes, all event codes, zstd input, ANSI/DEC terminal behavior, static/range selection, idle limiting, FPS capping, and svg-term-compatible default geometry.

v1/v2 are intentionally rejected. Convert old recordings first:

```bash
asciinema convert old.cast recording-v3.cast
```

Default canvas geometry matches svg-term-cli (`cols × 10px`, `rows × 16.7px × 1.3`, zero padding), while line/style reuse generally produces a smaller SVG.

See the repository [README](https://github.com/kingsword09/asg#readme) for all options and architecture details.

The package runs the same Rust binary as a `wasm32-wasip2` component through Node.js.
