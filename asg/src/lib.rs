pub mod asciicast;
pub mod input;
pub mod renderer;
pub mod terminal;
pub mod theme;
pub mod timeline;

use std::io::BufRead;

use anyhow::Result;

use crate::asciicast::Asciicast;
use crate::renderer::RenderOptions;
use crate::theme::Theme;
use crate::timeline::TimelineOptions;

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub timeline: TimelineOptions,
    pub render: RenderOptions,
    /// Explicit CLI/API theme. When absent, the v3 header theme wins, followed
    /// by svg-term's default theme.
    pub theme: Option<Theme>,
}

pub struct GeneratedSvg {
    pub data: String,
    pub frames: usize,
    pub duration: f64,
    pub cols: usize,
    pub rows: usize,
}

pub fn generate<R: BufRead>(reader: R, config: &Config) -> Result<GeneratedSvg> {
    let cast = Asciicast::parse(reader)?;
    let timeline = timeline::build(&cast, &config.timeline)?;
    let mut render_options = config.render.clone();
    render_options.theme = config
        .theme
        .clone()
        .or_else(|| cast.header.term.theme.clone())
        .unwrap_or_default();
    let data = renderer::render(&timeline, &render_options)?;

    Ok(GeneratedSvg {
        data,
        frames: timeline.frames.len(),
        duration: timeline.duration,
        cols: timeline.cols,
        rows: timeline.rows,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn cast_theme_is_used_when_no_explicit_theme_is_set() {
        let input = r##"{"version":3,"term":{"cols":2,"rows":1,"theme":{"fg":"#123456","bg":"#010203","palette":"#000000:#111111:#222222:#333333:#444444:#555555:#666666:#777777"}}}
[0,"o","x"]
"##;
        let result = generate(Cursor::new(input), &Config::default()).unwrap();

        assert!(result.data.contains("fill=\"#010203\""));
        assert!(result.data.contains("fill=\"#123456\""));
    }

    #[test]
    fn repository_demo_matches_geometry_and_stays_below_svg_term_size() {
        let mut config = Config::default();
        config.timeline.cursor = false;
        let result = generate(
            Cursor::new(include_str!("../../examples/demo.cast")),
            &config,
        )
        .unwrap();

        assert_eq!((result.cols, result.rows), (80, 16));
        assert!(result.data.starts_with(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"800\" height=\"347.36\""
        ));
        assert!(
            result.data.len() <= 791_872,
            "SVG exceeded svg-term-cli reference size: {} bytes",
            result.data.len()
        );
    }
}
