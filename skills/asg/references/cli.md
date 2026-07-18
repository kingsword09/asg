# ASG CLI Reference

Use `asg --help` as the authoritative interface. This reference covers the stable ASG 2.x workflow and the decisions most often needed by an agent.

## Command shape

```text
asg [OPTIONS] [INPUT] [OUTPUT]
```

Both positional arguments default to `-`, meaning stdin and stdout. Prefer explicit paths for generated documentation assets.

Supported inputs:

- asciicast v3 `.cast` files
- zstd-compressed `.cast.zst` files
- stdin via `-`
- HTTP(S) URLs
- asciinema.org recording IDs

ASG intentionally rejects asciicast v1 and v2.

## Option selection

| Goal | Option | Notes |
|---|---|---|
| Add terminal chrome | `--window` | macOS-style, svg-term-compatible decorations |
| Render one state | `--at <seconds>` | Produces a static SVG without reel animation |
| Start later | `--from <seconds>` | Rebases the animation to the selected state |
| Stop earlier | `--to <seconds>` | Combine with `--from` for an excerpt |
| Change playback rate | `--speed <number>` | Values above 1 play faster |
| Limit visual frames | `--fps <number>` | Default is 30; lower values may reduce size |
| Cap long pauses | `--idle-time-limit <seconds>` | Overrides the v3 header value |
| Fix terminal width | `--cols <number>` | `--width` is an alias |
| Fix terminal height | `--rows <number>` | `--height` is an alias |
| Hide the cursor | `--no-cursor` | Useful for polished documentation |
| Stop after one pass | `--no-loop` | Leaves the final frame after playback |
| Select a theme | `--theme <name-or-palette>` | Named theme or 18 comma-separated colors |
| Select fonts | `--font-family <css-stack>` | Quote stacks containing spaces or commas |
| Change type size | `--font-size <pixels>` | Default is 16 |
| Change row spacing | `--line-height <number>` | Default is 1.4 |
| Add padding | `--padding <pixels>` | Override axes with `--padding-x` or `--padding-y` |
| Show diagnostics | `-v`, `-vv` | Increase verbosity when troubleshooting |

Named themes:

```text
svg-term, atom-one, asciinema, dracula, github-dark, github-light,
monokai, solarized-dark, solarized-light
```

A custom theme is `background,foreground` followed by 16 ANSI colors, all comma-separated.

## Recipes

### README animation

```bash
asg recording.cast docs/terminal-demo.svg --window --no-cursor
```

If a static preview service captures a blank opening state, rerender after inspecting the first events:

```bash
asg recording.cast docs/terminal-demo.svg --window --no-cursor --from 0.1
```

### Compact excerpt

```bash
asg recording.cast docs/terminal-demo.svg --window --from 2 --to 14 --fps 20
```

### Light documentation theme

```bash
asg recording.cast docs/terminal-demo.svg --window --theme github-light
```

### Static social preview

```bash
asg recording.cast docs/terminal-preview.svg --window --at 8.5 --no-cursor
```

### Stream conversion

```bash
cat recording.cast | asg - - > recording.svg
```

Keep diagnostics on stderr when writing SVG to stdout. Do not mix log output into the redirected SVG stream.

## Troubleshooting

### Unsupported cast version

ASG is v3-only. Use `asciinema convert` to create a v3 copy when the user wants migration; do not mutate the original recording.

### npm or WASI invocation fails

1. Check `node --version`; prefer Node.js 22 or newer.
2. Invoke the current package with `npx --yes @kingsword/asg@latest --version`.
3. Ensure the input exists and the output parent is writable.
4. Retry with an explicit input and output path inside the current workspace.
5. Use the native Rust package when the WASI host cannot provide a required platform capability.

### Output looks soft

- View at the SVG's native aspect ratio and avoid screenshotting or rasterizing it.
- Avoid CSS dimensions that distort the aspect ratio.
- Keep the default pixel-native geometry unless the destination requires different sizing.
- Remember that font rasterization is controlled by the viewer; select a suitable monospace `--font-family` when the destination environment is known.

### Animation appears blank or static

- Open the SVG directly to distinguish a host limitation from a rendering problem.
- Use `--from 0.1` only when the first recorded state is blank.
- Use `--at <seconds>` when the destination intentionally does not animate SVG images.
- Confirm the hosting service serves the file as an SVG image and does not sanitize its CSS animation.

### Output is too large

- Trim irrelevant time with `--from` and `--to`.
- Lower `--fps` after checking that fast terminal changes remain readable.
- Use `--idle-time-limit` to shorten long pauses.
- Do not reduce size by manually minifying generated internals before comparing the supported timing controls.
