# asg-cli (Asciinema SVG Generator)

Convert asciinema .cast recordings into animated SVGs.

This is the npm distribution of ASG. It provides the `asg` CLI command via Node.js (WASI component under the hood).

## Installation

- Global install:
  - npm: `npm i -g asg-cli`
  - pnpm: `pnpm add -g asg-cli`
  - yarn: `yarn global add asg-cli`

- One-off usage (no global install):
  - npx: `npx -p asg-cli asg --help`

Note: The package name is `asg-cli`, but the installed command is `asg` (from the `bin` field).

## Usage

Same as described in the root README. Quick reference:

```
Usage: asg [OPTIONS] <INPUT> <OUTPUT>

Arguments:
  <INPUT>   Input .cast file or URL
  <OUTPUT>  Output file path (.svg)

Options:
      --cols <COLS>                Fixed terminal width (columns)
      --rows <ROWS>                Fixed terminal height (rows)
      --font-family <FONT>         Font family override
      --font-size <PX>             Font size [default: 14]
      --theme <NAME>               Theme name (e.g., asciinema, nord, dracula)
      --bg <COLOR>                 Background color, e.g. #000, transparent
      --fps <FPS>                  Frames per second [default: 30]
      --speed <MULTIPLIER>         Speed multiplier [default: 1.0]
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

Examples:

- Convert a local .cast to SVG:

```
asg examples/demo.cast examples/demo.svg
```

- Render a single frame at 2.5s to a static SVG:

```
asg --at 2.5 examples/demo.cast examples/still.svg
```

- Faster playback and different theme:

```
asg --speed 1.5 --theme dracula examples/demo.cast examples/fast.svg
```

## Notes

- On Node.js, `asg` uses a WASI component under the hood. Paths should be provided as normal host paths; the CLI maps them internally as needed.
- If another global package also provides an `asg` binary, whichever is installed last will win. You can uninstall the conflicting package or use `npx -p asg-cli asg ...`.
