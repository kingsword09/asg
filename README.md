# ASG - Asciinema SVG Generator

A Rust CLI that converts Asciinema cast files (`.cast`) into animated SVG.

Chinese documentation: [README_ZH.md](README_ZH.md)

## Features

- 🎬 Supports Asciicast v2
- 🌐 Works with local files, URLs, and remote recording IDs
- 🎨 Accurate per-cell ANSI colors and text styles (bold, italic, underline) with background rectangles
- ⚡ Fast terminal emulation powered by `vte`
- 📦 Produces a self-contained animated SVG file
- 🔧 Customizable font family, font size, line height, theme, and padding

## Installation

### Install via npm (recommended)

ASG is distributed as an npm package named `@kingsword/asg`, which installs the `asg` command.

- Global install:

```bash
npm i -g @kingsword/asg
# then
asg --help
```

- One-off usage (without global install):

```bash
npx -p @kingsword/asg asg --help
```

Note:
- The package name is `@kingsword/asg`, but the installed command is `asg` (from the `bin` field).
- There is currently no prebuilt dynamic/static Rust binary published to crates.io for `cargo install asg`. If you want a native binary, build from source (below).

### Build from source

```bash
# Clone the repository
git clone https://github.com/kingsword09/asg.git
cd asg

# Build release
cargo build --release

# Install to system (optional)
cargo install --path .
```

### Requirements

- Rust stable toolchain
- Cargo package manager

## Usage

### Basic usage

```bash
# Convert a local .cast file
asg demo.cast demo.svg

# Specify output file name (second positional arg)
asg demo.cast output.svg

# Download from asciinema.org by recording ID and convert
asg 113643 output.svg

# Use a custom font stack
asg demo.cast output.svg --font-family "JetBrains Mono,Monaco,Consolas,Liberation Mono,Menlo,monospace"
```

### CLI options

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

## Architecture

ASG uses a modular architecture consisting of these primary modules:

### Core modules

- `src/input.rs` — Input layer for local files, URLs, and remote IDs
- `src/asciicast.rs` — Asciicast v2 parser
- `src/terminal.rs` — VTE-based terminal emulator (parses ANSI/SGR, produces frames)
- `src/renderer.rs` — SVG generation and CSS animation
- `src/main.rs` — CLI entrypoint and orchestration

### Data flow

```
Input (file/URL/ID)
    ↓
Input reader
    ↓
.cast NDJSON stream
    ↓
Asciicast parser
    ↓
Terminal emulator (VTE)
    ↓
Frame sequence (cells with fg/bg and styles)
    ↓
SVG renderer
    ↓
Animated SVG file
```

## Technical highlights

### Terminal emulation

- High-performance and spec-compliant ANSI sequence parsing via `vte`
- Full SGR support: foreground/background (standard/bright/256-color/truecolor), bold, italic, underline
- Cursor movement, screen clearing, and other control sequences

### SVG generation

- Smooth animation using CSS keyframes with per-frame opacity transitions
- Row-level grouping ensures background rectangles render beneath text
- Backgrounds are merged into contiguous runs of `<rect>` to reduce element count
- Text is grouped by (foreground-color + style) and applies `font-weight`/`font-style`/`text-decoration`
- Configurable font family, font size, line height, and padding

### Performance

- Streaming NDJSON parsing without loading the whole file
- Tracks only state changes to minimize memory and output
- Merges background/text runs to keep SVG DOM small

## Examples

### Local file

```bash
# Record a terminal session
asciinema rec demo.cast

# Convert to SVG
asg demo.cast demo.svg

# Open in a browser
open demo.svg
```

### Remote recording

```bash
# Fetch from asciinema.org and convert
asg 113643 terminal-demo.svg
```

## Contributing

Issues and pull requests are welcome!

### Development

```bash
# Run tests
cargo test

# Lint code
cargo clippy

# Format code
cargo fmt
```

## Related projects

- [asciinema](https://github.com/asciinema/asciinema) — Terminal recorder
- [svg-term-cli](https://github.com/marionebl/svg-term-cli) — JavaScript implementation
- [agg](https://github.com/asciinema/agg) — Asciinema GIF generator

## License

Apache-2.0 License

## Author

Kingsword <kingsword09@gmail.com>
