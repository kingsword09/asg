use std::io::Write;
use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use asg::{self, Config, Theme};
use clap::Parser;
use clap::ValueEnum;
use rgb::RGB8; // for Theme::value_variants()

#[derive(Clone, Copy, Default)]
struct ThemeParser;

impl clap::builder::TypedValueParser for ThemeParser {
    type Value = asg::Theme;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let s = value.to_string_lossy();

        if s.contains(',') {
            Ok(asg::Theme::Custom(s.to_string()))
        } else {
            clap::value_parser!(asg::Theme).parse_ref(cmd, arg, value)
        }
    }

    fn possible_values(
        &self,
    ) -> Option<Box<dyn Iterator<Item = clap::builder::PossibleValue> + '_>> {
        Some(Box::new(
            asg::Theme::value_variants()
                .iter()
                .filter_map(|v| v.to_possible_value())
                .chain(std::iter::once(clap::builder::PossibleValue::new("custom"))),
        ))
    }
}

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Input file (.cast) or URL
    input: String,

    /// Output file (.svg)
    output: String,

    /// Select color theme (or provide comma-separated hex colors)
    /// If omitted, we will use header.theme from the cast file when available.
    #[clap(short, long, value_parser = ThemeParser)]
    theme: Option<Theme>,

    /// Adjust playback speed
    #[clap(short, long, default_value = "1.0")]
    speed: f64,

    /// Set FPS (frames per second)
    #[clap(long, default_value = "30")]
    fps: u8,

    /// Specify font family to use
    #[clap(
        long,
        default_value = "JetBrains Mono,Monaco,Consolas,Liberation Mono,Menlo,monospace"
    )]
    font_family: String,

    /// Set font size (in pixels)
    #[clap(long, default_value = "14")]
    font_size: u8,

    /// Set idle time limit (in seconds)
    #[clap(short = 'i', long)]
    idle_time_limit: Option<f64>,

    /// Override terminal width (number of columns)
    #[clap(long)]
    cols: Option<u16>,

    /// Override terminal height (number of rows)
    #[clap(long)]
    rows: Option<u16>,

    /// Path to a directory containing font files
    #[clap(long)]
    font_dir: Option<String>,

    /// Disable animation loop
    #[clap(long)]
    no_loop: bool,

    /// Set line height
    #[clap(long, default_value = "1.4")]
    line_height: f32,

    /// Timestamp of frame to render in seconds (for static image)
    #[clap(long)]
    at: Option<f64>,

    /// Lower range of timeline to render in seconds
    #[clap(long)]
    from: Option<f64>,

    /// Upper range of timeline to render in seconds
    #[clap(long)]
    to: Option<f64>,

    /// Disable cursor rendering
    #[clap(long)]
    no_cursor: bool,

    /// Render with window decorations
    #[clap(long)]
    window: bool,

    /// Distance between text and image bounds
    #[clap(long, default_value = "10")]
    padding: u16,

    /// Distance between text and image bounds on x axis
    #[clap(long)]
    padding_x: Option<u16>,

    /// Distance between text and image bounds on y axis
    #[clap(long)]
    padding_y: Option<u16>,

    /// Timeline mode: original (variable per-frame) or fixed (resampled to FPS)
    #[clap(long, value_enum, default_value_t = asg::Timeline::Original)]
    timeline: asg::Timeline,

    /// Verbose mode (-v, -vv, -vvv, etc.)
    #[clap(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Compress output SVG using zstd (writes .zst)
    #[clap(long)]
    zstd: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set up logging
    let log_level = match cli.verbose {
        0 => "error",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();

    // Build config
    let config = Config {
        theme: cli.theme,
        speed: cli.speed,
        fps: cli.fps,
        font_size: cli.font_size,
        font_family: cli.font_family,
        line_height: cli.line_height,
        cols: cli.cols,
        rows: cli.rows,
        idle_time_limit: cli.idle_time_limit,
        loop_enable: !cli.no_loop,
        at: cli.at,
        from: cli.from,
        to: cli.to,
        no_cursor: cli.no_cursor,
        window: cli.window,
        padding: cli.padding,
        padding_x: cli.padding_x,
        padding_y: cli.padding_y,
        timeline: cli.timeline,
    };

    // Read the input
    let reader = asg::input::get_reader(&cli.input)?;
    let cast = asg::asciicast::Asciicast::parse(reader)?;
    // Destructure to avoid moving issues and to reuse header later
    let asg::asciicast::Asciicast {
        header,
        events: all_events,
    } = cast;

    // Get terminal dimensions
    let cols = config.cols.unwrap_or(header.width as u16);
    let rows = config.rows.unwrap_or(header.height as u16);

    // Process events through terminal emulator
    let mut emulator = asg::terminal::Emulator::new(cols as usize, rows as usize);
    let mut frames: Vec<asg::terminal::Frame> = Vec::new();
    let mut durations: Vec<f64> = Vec::new();
    let mut last_time = config.from.unwrap_or(0.0);
    let fps = config.fps as f64;
    let trailing_default = 1.0 / fps; // show final state for at least one frame worth of time

    // Filter events by time range if specified
    let mut events: Vec<_> = all_events
        .into_iter()
        .filter(|e| {
            if let Some(from) = config.from
                && e.time < from
            {
                return false;
            }

            if let Some(to) = config.to
                && e.time > to
            {
                return false;
            }
            true
        })
        .collect();

    // Detect shell from header.env to scope certain filters (e.g., zsh temporary '%')
    let is_zsh = header
        .env
        .as_ref()
        .and_then(|m| m.get("SHELL"))
        .map(|s| s.contains("zsh"))
        .unwrap_or(false);

    // Strip system/OSC messages by default (window title, cwd, session footer, etc.)
    events.retain(|e| match e.event_type {
        asg::asciicast::EventType::Output => !should_strip_system_output(&e.data, is_zsh),
        _ => true,
    });

    // Handle --at option for single frame
    if let Some(at_time) = config.at {
        // Process events up to the specified time
        for event in &events {
            if event.time <= at_time
                && let asg::asciicast::EventType::Output = event.event_type
            {
                emulator.process(event.data.as_bytes());
            }
        }
        frames.push(emulator.get_frame());
        durations.push(trailing_default);
    } else {
        match config.timeline {
            asg::Timeline::Original => {
                // Variable-duration animation processing (original timing)
                for event in events {
                    let mut duration = event.time - last_time;
                    // Apply speed factor
                    duration /= config.speed;
                    // Apply idle time limit if specified
                    if let Some(limit) = config.idle_time_limit {
                        duration = duration.min(limit);
                    }
                    // Show current state for the duration until this event
                    frames.push(emulator.get_frame());
                    durations.push(duration.max(0.0));
                    // Process the event to update state
                    if let asg::asciicast::EventType::Output = event.event_type {
                        emulator.process(event.data.as_bytes());
                    }
                    last_time = event.time;
                }
                // Add final frame showing the last state for a short trailing duration
                frames.push(emulator.get_frame());
                durations.push(trailing_default);
            }
            asg::Timeline::Fixed => {
                // Resample to fixed FPS
                let fps = config.fps as f64;
                let fd = 1.0 / fps;
                for event in events {
                    let mut duration = event.time - last_time;
                    duration /= config.speed;
                    if let Some(limit) = config.idle_time_limit {
                        duration = duration.min(limit);
                    }
                    let frame_count = (duration * fps).ceil() as usize;
                    if frame_count > 0 {
                        for _ in 0..frame_count {
                            frames.push(emulator.get_frame());
                            durations.push(fd);
                        }
                    }
                    if let asg::asciicast::EventType::Output = event.event_type {
                        emulator.process(event.data.as_bytes());
                    }
                    last_time = event.time;
                }
                // Final frame
                frames.push(emulator.get_frame());
                durations.push(fd);
            }
        }
    }

    // Determine effective theme: CLI > header.theme > default
    let theme: asg::theme::Theme = if let Some(cli_theme) = config.theme.clone() {
        cli_theme.try_into()?
    } else if let Some(header_theme) = header.theme.clone() {
        theme_from_header(&header_theme)?
    } else {
        asg::theme::Theme::default()
    };

    // Render to SVG
    let renderer = asg::renderer::SvgRenderer::new(cols as usize, rows as usize)
        .with_font_size(config.font_size)
        .with_line_height(config.line_height)
        .with_theme(theme)
        .with_loop_enable(config.loop_enable)
        .with_cursor_visible(!config.no_cursor)
        .with_window(config.window)
        .with_padding(config.effective_padding_x(), config.effective_padding_y());

    let svg = renderer.render(&frames, &durations)?;

    // Write output (optionally compressed)
    let resolved_output_path = asg::input::resolve_output_path(&cli.output)?;
    let output_path = Path::new(&resolved_output_path);
    let svg_str = svg.to_string();

    if cli.zstd {
        #[cfg(not(target_family = "wasm"))]
        {
            use std::fs::File;
            // If output doesn't already end with .zst, append .zst to the extension
            let mut zpath = output_path.to_path_buf();
            if let Some(ext) = output_path.extension().and_then(|e| e.to_str()) {
                let new_ext = format!("{}.zst", ext);
                let _ = zpath.set_extension(new_ext);
            } else {
                let _ = zpath.set_extension("zst");
            }
            let mut f = File::create(&zpath)?;
            let mut enc = zstd::stream::Encoder::new(&mut f, 0)?; // 0 = default level
            enc.write_all(svg_str.as_bytes())?;
            enc.finish()?; // finalize
            println!("✨ SVG (zstd) saved to: {}", zpath.display());
        }
        #[cfg(target_family = "wasm")]
        {
            anyhow::bail!("zstd compression is not supported on wasm target");
        }
    } else {
        let mut file = std::fs::File::create(output_path)?;
        file.write_all(svg_str.as_bytes())?;
        println!("✨ SVG animation saved to: {}", resolved_output_path);
    }
    if let Some(at_time) = config.at {
        println!("🖼️  Static frame at {:.2}s", at_time);
    } else {
        let total: f64 = durations.iter().copied().sum();
        println!("📊 Total frames: {}", frames.len());
        println!("⏱️  Duration: {:.2}s", total);
    }

    Ok(())
}

fn theme_from_header(h: &asg::theme::Theme) -> anyhow::Result<asg::theme::Theme> {
    // Build a comma-separated list of 18 hex colors (bg, fg, then 16 palette colors)
    fn to_hex(c: RGB8) -> String {
        format!("{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    }

    let mut parts: Vec<String> = Vec::with_capacity(18);
    parts.push(to_hex(RGB8::from(h.bg)));
    parts.push(to_hex(RGB8::from(h.fg)));
    for rgb in &h.palette {
        parts.push(to_hex(RGB8::from(*rgb)));
    }
    let s = parts.join(",");
    asg::theme::Theme::from_str(&s)
}

fn strip_ansi(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    let mut out = String::new();
    while i < bytes.len() {
        let b = bytes[i];
        if b == 0x1b {
            // ESC
            if i + 1 < bytes.len() {
                let b1 = bytes[i + 1];
                match b1 {
                    b'[' => {
                        // CSI: ESC [ ... final (0x40..=0x7E)
                        i += 2;
                        while i < bytes.len() {
                            let bb = bytes[i];
                            if (0x40..=0x7e).contains(&bb) {
                                i += 1;
                                break;
                            }
                            i += 1;
                        }
                        continue;
                    }
                    b']' => {
                        // OSC: ESC ] ... BEL or ST (ESC \)
                        i += 2;
                        while i < bytes.len() {
                            if bytes[i] == 0x07 {
                                i += 1;
                                break;
                            } // BEL
                            if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                                i += 2;
                                break;
                            }
                            i += 1;
                        }
                        continue;
                    }
                    _ => {
                        // Other ESC sequences: skip ESC + next byte
                        i += 2;
                        continue;
                    }
                }
            } else {
                i += 1;
                continue;
            }
        }
        // Drop control chars except printable ASCII and UTF-8 continuation handled by from_utf8_lossy later
        if b == b'\r' || b == b'\n' || b == b'\t' || b < 0x20 {
            i += 1;
            continue;
        }
        // push utf8 chunk conservatively (single byte or start of multi-byte)
        // For simplicity, collect the remainder and let from_utf8_lossy handle
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn should_strip_system_output(s: &str, is_zsh: bool) -> bool {
    // Strip OSC sequences (Operating System Command): ESC ] ... (terminated by BEL or ST)
    if s.starts_with("\x1b]") {
        return true;
    }
    // Strip common session footer lines
    let t = s.trim();
    if t == "Saving session..." || t == "completed." {
        return true;
    }
    // Strip lone zsh default prompt lines that render just '%'
    if is_zsh {
        let visible = strip_ansi(s);
        if visible.trim() == "%" {
            return true;
        }
    }
    false
}
