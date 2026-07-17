use std::str::FromStr;

use anyhow::{Context, Result};
use asg::renderer::{DEFAULT_FONT_FAMILY, DEFAULT_FONT_SIZE, DEFAULT_LINE_HEIGHT, RenderOptions};
use asg::theme::Theme;
use asg::timeline::TimelineOptions;
use asg::{Config, generate};
use clap::{ArgAction, Parser};

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Convert asciicast v3 recordings to compact animated SVG"
)]
struct Cli {
    /// v3 .cast path, URL, asciinema.org ID, or - for stdin
    #[arg(default_value = "-")]
    input: String,

    /// SVG output path, or - for stdout
    #[arg(default_value = "-")]
    output: String,

    /// Named theme or bg,fg,16-color comma-separated palette
    #[arg(long, value_parser = parse_theme)]
    theme: Option<Theme>,

    /// Playback speed multiplier
    #[arg(short, long, default_value_t = 1.0)]
    speed: f64,

    /// Maximum number of visual frames per second
    #[arg(long, default_value_t = 30)]
    fps: u16,

    /// CSS font stack
    #[arg(
        long,
        default_value = DEFAULT_FONT_FAMILY
    )]
    font_family: String,

    /// Font size in output pixels
    #[arg(long, default_value_t = DEFAULT_FONT_SIZE)]
    font_size: f64,

    /// Line-height multiplier
    #[arg(long, default_value_t = DEFAULT_LINE_HEIGHT)]
    line_height: f64,

    /// Maximum idle gap in seconds (overrides the v3 header)
    #[arg(short = 'i', long)]
    idle_time_limit: Option<f64>,

    /// Pin terminal columns (alias: --width)
    #[arg(long, alias = "width")]
    cols: Option<u16>,

    /// Pin terminal rows (alias: --height)
    #[arg(long, alias = "height")]
    rows: Option<u16>,

    /// Render one static frame at this output-timeline time in seconds
    #[arg(long)]
    at: Option<f64>,

    /// Start animation at this output-timeline time in seconds
    #[arg(long)]
    from: Option<f64>,

    /// End animation at this output-timeline time in seconds
    #[arg(long)]
    to: Option<f64>,

    /// Hide the terminal cursor
    #[arg(long)]
    no_cursor: bool,

    /// Stop after one animation iteration
    #[arg(long)]
    no_loop: bool,

    /// Add svg-term compatible macOS-style window decorations
    #[arg(long)]
    window: bool,

    /// Padding on both axes in output pixels
    #[arg(long, default_value_t = 0.0)]
    padding: f64,

    /// Override horizontal padding in output pixels
    #[arg(long)]
    padding_x: Option<f64>,

    /// Override vertical padding in output pixels
    #[arg(long)]
    padding_y: Option<f64>,

    /// Increase diagnostic logging
    #[arg(short, long, action = ArgAction::Count)]
    verbose: u8,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_logging(cli.verbose);

    let config = Config {
        timeline: TimelineOptions {
            speed: cli.speed,
            fps: cli.fps,
            idle_time_limit: cli.idle_time_limit,
            cols: cli.cols,
            rows: cli.rows,
            cursor: !cli.no_cursor,
            at: cli.at,
            from: cli.from,
            to: cli.to,
        },
        render: RenderOptions {
            font_size: cli.font_size,
            line_height: cli.line_height,
            font_family: cli.font_family,
            padding_x: cli.padding_x.unwrap_or(cli.padding),
            padding_y: cli.padding_y.unwrap_or(cli.padding),
            window: cli.window,
            loop_animation: !cli.no_loop,
            ..RenderOptions::default()
        },
        theme: cli.theme,
    };

    let reader = asg::input::open(&cli.input)?;
    let generated = generate(reader, &config).context("failed to generate SVG")?;
    let path = asg::input::write(&cli.output, &generated.data)?;

    if let Some(path) = path {
        eprintln!(
            "wrote {} ({} bytes, {} frames, {:.3}s, {}x{} cells)",
            path.display(),
            generated.data.len(),
            generated.frames,
            generated.duration,
            generated.cols,
            generated.rows
        );
    }

    Ok(())
}

fn parse_theme(value: &str) -> Result<Theme, String> {
    if value.contains(',') {
        Theme::from_str(value).map_err(|error| error.to_string())
    } else {
        Theme::named(value).map_err(|error| error.to_string())
    }
}

fn init_logging(verbose: u8) {
    let level = match verbose {
        0 => log::LevelFilter::Warn,
        1 => log::LevelFilter::Info,
        _ => log::LevelFilter::Debug,
    };
    let _ = env_logger::Builder::from_default_env()
        .filter_level(level)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_svg_term_dimension_aliases() {
        let cli = Cli::try_parse_from([
            "asg", "in.cast", "out.svg", "--width", "80", "--height", "24",
        ])
        .unwrap();

        assert_eq!(cli.cols, Some(80));
        assert_eq!(cli.rows, Some(24));
    }

    #[test]
    fn defaults_use_pixel_native_geometry() {
        let cli = Cli::try_parse_from(["asg", "in.cast", "out.svg"]).unwrap();

        assert_eq!(cli.font_size, DEFAULT_FONT_SIZE);
        assert_eq!(cli.line_height, DEFAULT_LINE_HEIGHT);
        assert_eq!(cli.font_family, DEFAULT_FONT_FAMILY);
        assert_eq!(cli.padding, 0.0);
    }
}
