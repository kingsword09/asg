use anyhow::Result;
use asg::{self, Config, Theme};
use clap::Parser;
use clap::ValueEnum; // for Theme::value_variants()
use std::io::Write;
use std::path::Path;
// use std::time::Duration; // unused

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
            clap::value_parser!(asg::Theme)
                .parse_ref(cmd, arg, value)
        }
    }

    fn possible_values(
        &self,
    ) -> Option<Box<dyn Iterator<Item = clap::builder::PossibleValue> + '_>> {
        Some(Box::new(
            asg::Theme::value_variants()
                .iter()
                .filter_map(|v| v.to_possible_value())
                .chain(std::iter::once(clap::builder::PossibleValue::new("custom")))
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
    #[clap(short, long, value_parser = ThemeParser, default_value = "asciinema")]
    theme: Theme,

    /// Adjust playback speed
    #[clap(short, long, default_value = "1.0")]
    speed: f64,

    /// Set FPS (frames per second)
    #[clap(long, default_value = "30")]
    fps: u8,

    /// Specify font family to use
    #[clap(long, default_value = "JetBrains Mono,Monaco,Consolas,Liberation Mono,Menlo,monospace")]
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

    /// Verbose mode (-v, -vv, -vvv, etc.)
    #[clap(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
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
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level))
        .init();

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
    };

    // Read the input
    let reader = asg::input::get_reader(&cli.input)?;
    let cast = asg::asciicast::Asciicast::parse(reader)?;

    // Get terminal dimensions
    let cols = config.cols.unwrap_or(cast.header.width as u16);
    let rows = config.rows.unwrap_or(cast.header.height as u16);

    // Process events through terminal emulator
    let mut emulator = asg::terminal::Emulator::new(cols as usize, rows as usize);
    let mut frames = Vec::new();
    let mut last_time = 0.0;
    let fps = config.fps as f64;
    let frame_duration = 1.0 / fps;

    // Filter events by time range if specified
    let events: Vec<_> = cast.events.into_iter()
        .filter(|e| {
            if let Some(from) = config.from {
                if e.time < from { return false; }
            }
            if let Some(to) = config.to {
                if e.time > to { return false; }
            }
            true
        })
        .collect();

    // Handle --at option for single frame
    if let Some(at_time) = config.at {
        // Process events up to the specified time
        for event in &events {
            if event.time <= at_time {
                if let asg::asciicast::EventType::Output = event.event_type {
                    emulator.process(event.data.as_bytes());
                }
            }
        }
        frames.push(emulator.get_frame());
    } else {
        // Normal animation processing
        for event in events {
            let mut duration = event.time - last_time;
            
            // Apply speed factor
            duration /= config.speed;
            
            // Apply idle time limit if specified
            if let Some(limit) = config.idle_time_limit {
                duration = duration.min(limit);
            }
            
            // Generate frames for this duration
            let frame_count = (duration * fps).ceil() as usize;
            if frame_count > 0 {
                // Add frames showing the current state
                for _ in 0..frame_count {
                    frames.push(emulator.get_frame());
                }
            }
            
            // Process the event
            if let asg::asciicast::EventType::Output = event.event_type {
                emulator.process(event.data.as_bytes());
            }
            
            last_time = event.time;
        }
        
        // Add final frame
        frames.push(emulator.get_frame());
    }

    // Convert theme (clone to avoid moving from config)
    let theme: asg::theme::Theme = config.theme.clone().try_into()?;

    // Render to SVG
    let renderer = asg::renderer::SvgRenderer::new(cols as usize, rows as usize)
        .with_font_size(config.font_size)
        .with_line_height(config.line_height)
        .with_theme(theme)
        .with_loop_enable(config.loop_enable)
        .with_cursor_visible(!config.no_cursor)
        .with_window(config.window)
        .with_padding(config.effective_padding_x(), config.effective_padding_y());
    
    let svg = renderer.render(&frames, frame_duration)?;

    // Write output
    let output_path = Path::new(&cli.output);
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    let mut file = std::fs::File::create(output_path)?;
    let svg_str = svg.to_string();
    file.write_all(svg_str.as_bytes())?;
    
    println!("✨ SVG animation saved to: {}", cli.output);
    if config.at.is_some() {
        println!("🖼️  Static frame at {:.2}s", config.at.unwrap());
    } else {
        println!("📊 Total frames: {}", frames.len());
        println!("⏱️  Duration: {:.2}s", frames.len() as f64 * frame_duration);
    }
    
    Ok(())
}
