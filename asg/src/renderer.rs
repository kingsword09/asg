use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use anyhow::{Result, bail};
use avt::{Cell, Line, Pen};

use crate::theme::{Rgb, Theme};
use crate::timeline::Timeline;

const SVG_TERM_COLUMN_PX: f64 = 10.0;

#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// CSS font size in output pixels. svg-term uses 1.67 view-box units at a
    /// 10x scale, which is 16.7 output pixels.
    pub font_size: f64,
    pub line_height: f64,
    pub font_family: String,
    pub padding_x: f64,
    pub padding_y: f64,
    pub window: bool,
    pub loop_animation: bool,
    pub theme: Theme,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            font_size: 16.7,
            line_height: 1.3,
            font_family:
                "Monaco,Consolas,Menlo,'Bitstream Vera Sans Mono','Powerline Symbols',monospace"
                    .to_owned(),
            padding_x: 0.0,
            padding_y: 0.0,
            window: false,
            loop_animation: true,
            theme: Theme::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct TextStyle {
    foreground: Rgb,
    bold: bool,
    faint: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    blink: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BackgroundRun {
    col: usize,
    width: usize,
    color: Rgb,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TextRun {
    col: usize,
    text: String,
    style: TextStyle,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
struct RenderedLine {
    backgrounds: Vec<BackgroundRun>,
    text: Vec<TextRun>,
}

impl RenderedLine {
    fn is_empty(&self) -> bool {
        self.backgrounds.is_empty() && self.text.is_empty()
    }
}

pub fn render(timeline: &Timeline, options: &RenderOptions) -> Result<String> {
    validate(options)?;
    if timeline.cols == 0 || timeline.rows == 0 {
        bail!("terminal dimensions must be greater than zero");
    }
    if timeline.frames.is_empty() {
        bail!("timeline must contain at least one frame");
    }

    let font_size = options.font_size / SVG_TERM_COLUMN_PX;
    let row_height = font_size * options.line_height;
    let cell_width = font_size * 0.6;
    let content_width_px = timeline.cols as f64 * SVG_TERM_COLUMN_PX;
    let content_height_units = timeline.rows as f64 * row_height;
    let content_height_px = content_height_units * SVG_TERM_COLUMN_PX;

    let (width, height, content_x, content_y, radius) = if options.window {
        (
            content_width_px + (options.padding_x + 20.0) * 2.0,
            content_height_px + options.padding_y * 2.0 + 60.0,
            options.padding_x + 15.0,
            options.padding_y + 50.0,
            5.0,
        )
    } else {
        (
            content_width_px + options.padding_x * 2.0,
            content_height_px + options.padding_y * 2.0,
            options.padding_x,
            options.padding_y,
            0.0,
        )
    };

    let rendered_frames = timeline
        .frames
        .iter()
        .map(|frame| {
            frame
                .snapshot
                .lines
                .iter()
                .take(timeline.rows)
                .map(|line| render_line(line, timeline.cols, &options.theme))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let (registry, references) = build_registry(&rendered_frames);
    let styles = collect_styles(&rendered_frames);
    let style_classes = styles
        .into_iter()
        .filter(|style| *style != default_style(&options.theme))
        .enumerate()
        .map(|(index, style)| (style, format!("s{index}")))
        .collect::<BTreeMap<_, _>>();

    let mut svg = String::with_capacity(estimate_capacity(timeline, &rendered_frames));
    write!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\">",
        number(width),
        number(height)
    )?;
    write!(
        svg,
        "<rect width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{}\"/>",
        number(width),
        number(height),
        number(radius),
        options.theme.background
    )?;

    if options.window {
        svg.push_str(
            "<circle cx=\"20\" cy=\"20\" r=\"6\" fill=\"#ff5f58\"/><circle cx=\"40\" cy=\"20\" r=\"6\" fill=\"#ffbd2e\"/><circle cx=\"60\" cy=\"20\" r=\"6\" fill=\"#18c132\"/>",
        );
    }

    write!(
        svg,
        "<svg x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" overflow=\"hidden\">",
        number(content_x),
        number(content_y),
        number(content_width_px),
        number(content_height_px),
        timeline.cols,
        number(content_height_units)
    )?;

    write_styles(
        &mut svg,
        timeline,
        options,
        &style_classes,
        rendered_frames.len(),
    )?;
    write!(
        svg,
        "<g font-family=\"{}\" font-size=\"{}\" fill=\"{}\">",
        escape_attribute(&options.font_family),
        number(font_size),
        options.theme.foreground
    )?;

    svg.push_str("<defs>");
    let cursor_width = font_size * 0.66;
    write!(
        svg,
        "<g id=\"c\"><rect width=\"{}\" height=\"{}\" fill=\"{}\"/></g>",
        number(cursor_width),
        number(row_height),
        options.theme.cursor
    )?;
    for (line, id) in &registry {
        write!(svg, "<g id=\"{id}\">")?;
        write_line(
            &mut svg,
            line,
            cell_width,
            font_size,
            row_height,
            &style_classes,
        )?;
        svg.push_str("</g>");
    }
    svg.push_str("</defs>");

    svg.push_str("<g class=\"r\">");
    for (frame_index, (frame, lines)) in timeline
        .frames
        .iter()
        .zip(rendered_frames.iter())
        .enumerate()
    {
        write!(
            svg,
            "<g transform=\"translate({} 0)\">",
            frame_index * timeline.cols
        )?;

        if let Some((col, row)) = frame.snapshot.cursor
            && col <= timeline.cols
            && row < timeline.rows
        {
            let x = col as f64 + 2.0 - font_size * 1.2;
            let y = if row == 0 {
                0.0
            } else {
                row as f64 * row_height - 1.0 + options.line_height * 0.75
            };
            write!(
                svg,
                "<use href=\"#c\" x=\"{}\" y=\"{}\"/>",
                number(x),
                number(y)
            )?;
        }

        for (row, line) in lines.iter().enumerate() {
            if line.is_empty() {
                continue;
            }

            if let Some(id) = references.get(line) {
                write!(
                    svg,
                    "<use href=\"#{id}\" y=\"{}\"/>",
                    number(row as f64 * row_height)
                )?;
            } else {
                write!(
                    svg,
                    "<g transform=\"translate(0 {})\">",
                    number(row as f64 * row_height)
                )?;
                write_line(
                    &mut svg,
                    line,
                    cell_width,
                    font_size,
                    row_height,
                    &style_classes,
                )?;
                svg.push_str("</g>");
            }
        }

        svg.push_str("</g>");
    }
    svg.push_str("</g></g></svg></svg>");

    Ok(svg)
}

fn validate(options: &RenderOptions) -> Result<()> {
    for (name, value) in [
        ("font size", options.font_size),
        ("line height", options.line_height),
    ] {
        if !value.is_finite() || value <= 0.0 {
            bail!("{name} must be a finite number greater than zero");
        }
    }
    for (name, value) in [
        ("horizontal padding", options.padding_x),
        ("vertical padding", options.padding_y),
    ] {
        if !value.is_finite() || value < 0.0 {
            bail!("{name} must be a finite, non-negative number");
        }
    }
    if options.font_family.trim().is_empty() {
        bail!("font family cannot be empty");
    }

    Ok(())
}

fn render_line(line: &Line, cols: usize, theme: &Theme) -> RenderedLine {
    let cells = &line.cells()[..line.cells().len().min(cols)];
    RenderedLine {
        backgrounds: background_runs(cells, theme),
        text: text_runs(cells, theme),
    }
}

fn background_runs(cells: &[Cell], theme: &Theme) -> Vec<BackgroundRun> {
    let mut output = Vec::new();
    let mut current: Option<BackgroundRun> = None;

    for (col, cell) in cells.iter().enumerate() {
        let (_, background) = effective_colors(cell.pen(), theme);
        if background == theme.background {
            if let Some(run) = current.take() {
                output.push(run);
            }
            continue;
        }

        match &mut current {
            Some(run) if run.color == background && run.col + run.width == col => run.width += 1,
            Some(_) => {
                output.push(
                    current
                        .replace(BackgroundRun {
                            col,
                            width: 1,
                            color: background,
                        })
                        .expect("background run exists"),
                );
            }
            None => {
                current = Some(BackgroundRun {
                    col,
                    width: 1,
                    color: background,
                });
            }
        }
    }
    if let Some(run) = current {
        output.push(run);
    }

    output
}

fn text_runs(cells: &[Cell], theme: &Theme) -> Vec<TextRun> {
    struct Pending {
        col: usize,
        end: usize,
        text: String,
        style: TextStyle,
    }

    fn flush(output: &mut Vec<TextRun>, pending: Option<Pending>) {
        let Some(mut pending) = pending else {
            return;
        };
        let leading = pending.text.chars().take_while(|ch| *ch == ' ').count();
        let text = pending.text.trim_end_matches(' ').to_owned();
        let text = text.chars().skip(leading).collect::<String>();
        if !text.is_empty() {
            pending.col += leading;
            output.push(TextRun {
                col: pending.col,
                text,
                style: pending.style,
            });
        }
    }

    let mut output = Vec::new();
    let mut pending: Option<Pending> = None;

    for (col, cell) in cells.iter().enumerate() {
        let width = usize::from(cell.width());
        if width == 0 {
            continue;
        }
        let style = style(cell.pen(), theme);

        match &mut pending {
            Some(run) if run.style == style && run.end == col => {
                run.text.push(cell.char());
                run.end = col + width;
            }
            _ => {
                flush(&mut output, pending.take());
                pending = Some(Pending {
                    col,
                    end: col + width,
                    text: cell.char().to_string(),
                    style,
                });
            }
        }
    }
    flush(&mut output, pending);

    output
}

fn style(pen: &Pen, theme: &Theme) -> TextStyle {
    let (foreground, _) = effective_colors(pen, theme);
    TextStyle {
        foreground,
        bold: pen.is_bold(),
        faint: pen.is_faint(),
        italic: pen.is_italic(),
        underline: pen.is_underline(),
        strikethrough: pen.is_strikethrough(),
        blink: pen.is_blink(),
    }
}

fn default_style(theme: &Theme) -> TextStyle {
    TextStyle {
        foreground: theme.foreground,
        bold: false,
        faint: false,
        italic: false,
        underline: false,
        strikethrough: false,
        blink: false,
    }
}

fn effective_colors(pen: &Pen, theme: &Theme) -> (Rgb, Rgb) {
    let fallback = if pen.is_bold() {
        theme.bold
    } else {
        theme.foreground
    };
    let mut foreground = theme.resolve(pen.foreground(), fallback);
    let mut background = theme.resolve(pen.background(), theme.background);
    if pen.is_inverse() {
        std::mem::swap(&mut foreground, &mut background);
    }
    (foreground, background)
}

fn collect_styles(frames: &[Vec<RenderedLine>]) -> BTreeSet<TextStyle> {
    frames
        .iter()
        .flatten()
        .flat_map(|line| line.text.iter().map(|run| run.style))
        .collect()
}

fn build_registry(
    frames: &[Vec<RenderedLine>],
) -> (Vec<(RenderedLine, String)>, HashMap<RenderedLine, String>) {
    let mut counts = HashMap::<RenderedLine, usize>::new();
    for line in frames.iter().flatten().filter(|line| !line.is_empty()) {
        *counts.entry(line.clone()).or_default() += 1;
    }

    let mut registry = Vec::new();
    let mut references = HashMap::new();
    for line in frames.iter().flatten().filter(|line| !line.is_empty()) {
        if counts.get(line).copied().unwrap_or_default() < 2 || references.contains_key(line) {
            continue;
        }

        let id = format!("l{}", registry.len());
        registry.push((line.clone(), id.clone()));
        references.insert(line.clone(), id);
    }

    (registry, references)
}

fn write_styles(
    output: &mut String,
    timeline: &Timeline,
    options: &RenderOptions,
    classes: &BTreeMap<TextStyle, String>,
    frame_count: usize,
) -> std::fmt::Result {
    output.push_str("<style>");
    if classes.keys().any(|style| style.blink) {
        output.push_str("@keyframes k{50%{opacity:0}}");
    }

    let animated = timeline.duration > 0.0 && frame_count > 1;
    if animated {
        output.push_str("@keyframes a{");
        for (index, frame) in timeline.frames.iter().enumerate() {
            let percentage = (frame.time / timeline.duration * 100.0).clamp(0.0, 100.0);
            write!(
                output,
                "{}%{{transform:translateX(-{}px)}}",
                number(percentage),
                index * timeline.cols
            )?;
        }
        if timeline
            .frames
            .last()
            .is_some_and(|frame| frame.time < timeline.duration)
        {
            write!(
                output,
                "100%{{transform:translateX(-{}px)}}",
                (frame_count - 1) * timeline.cols
            )?;
        }
        output.push('}');
        write!(
            output,
            ".r{{animation:a {}s steps(1,end) {}{}}}",
            number(timeline.duration),
            if options.loop_animation {
                "infinite"
            } else {
                "1"
            },
            if options.loop_animation {
                ""
            } else {
                " forwards"
            }
        )?;
    }

    for (style, class) in classes {
        write!(output, ".{class}{{fill:{}", style.foreground)?;
        if style.bold {
            output.push_str(";font-weight:700");
        }
        if style.faint {
            output.push_str(";opacity:.5");
        }
        if style.italic {
            output.push_str(";font-style:italic");
        }
        match (style.underline, style.strikethrough) {
            (true, true) => output.push_str(";text-decoration:underline line-through"),
            (true, false) => output.push_str(";text-decoration:underline"),
            (false, true) => output.push_str(";text-decoration:line-through"),
            (false, false) => {}
        }
        if style.blink {
            output.push_str(";animation:k 1s step-end infinite");
        }
        output.push('}');
    }
    output.push_str("text{white-space:pre}</style>");

    Ok(())
}

fn write_line(
    output: &mut String,
    line: &RenderedLine,
    cell_width: f64,
    font_size: f64,
    row_height: f64,
    classes: &BTreeMap<TextStyle, String>,
) -> std::fmt::Result {
    for run in &line.backgrounds {
        write!(
            output,
            "<rect x=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
            number(run.col as f64 * cell_width),
            number(run.width as f64 * cell_width),
            number(row_height),
            run.color
        )?;
    }
    for run in &line.text {
        output.push_str("<text");
        if run.col > 0 {
            write!(output, " x=\"{}\"", number(run.col as f64 * cell_width))?;
        }
        write!(output, " y=\"{}\"", number(font_size))?;
        if let Some(class) = classes.get(&run.style) {
            write!(output, " class=\"{class}\"")?;
        }
        output.push('>');
        output.push_str(&escape_text(&run.text));
        output.push_str("</text>");
    }

    Ok(())
}

fn estimate_capacity(timeline: &Timeline, frames: &[Vec<RenderedLine>]) -> usize {
    let text_bytes = frames
        .iter()
        .flatten()
        .flat_map(|line| &line.text)
        .map(|run| run.text.len())
        .sum::<usize>();
    1_024 + timeline.frames.len() * 96 + text_bytes * 2
}

fn number(value: f64) -> String {
    if value.abs() < 0.000_000_5 {
        return "0".to_owned();
    }
    if (value.round() - value).abs() < 0.000_000_5 {
        return format!("{:.0}", value);
    }

    let mut value = format!("{value:.6}");
    while value.ends_with('0') {
        value.pop();
    }
    if value.ends_with('.') {
        value.pop();
    }
    value
}

fn escape_text(value: &str) -> String {
    escape_xml(value, false)
}

fn escape_attribute(value: &str) -> String {
    escape_xml(value, true)
}

fn escape_xml(value: &str, attribute: bool) -> String {
    let mut output = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' if attribute => output.push_str("&quot;"),
            '\'' if attribute => output.push_str("&apos;"),
            '\u{9}'
            | '\u{a}'
            | '\u{d}'
            | '\u{20}'..='\u{d7ff}'
            | '\u{e000}'..='\u{fffd}'
            | '\u{10000}'..='\u{10ffff}' => output.push(ch),
            _ => output.push('\u{fffd}'),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::asciicast::Asciicast;
    use crate::timeline::{TimelineOptions, build};

    use super::*;

    fn timeline(events: &str, cols: u16, rows: u16) -> Timeline {
        let input =
            format!("{{\"version\":3,\"term\":{{\"cols\":{cols},\"rows\":{rows}}}}}\n{events}");
        let cast = Asciicast::parse(Cursor::new(input)).unwrap();
        build(&cast, &TimelineOptions::default()).unwrap()
    }

    #[test]
    fn matches_svg_term_default_canvas_geometry() {
        let svg = render(
            &timeline("[0,\"o\",\"Hello\"]\n", 10, 3),
            &RenderOptions::default(),
        )
        .unwrap();
        let document = roxmltree::Document::parse(&svg).unwrap();
        let root = document.root_element();

        assert_eq!(root.attribute("width"), Some("100"));
        assert_eq!(root.attribute("height"), Some("65.13"));
        assert!(svg.contains("viewBox=\"0 0 10 6.513\""));
    }

    #[test]
    fn matches_svg_term_window_and_padding_geometry() {
        let options = RenderOptions {
            padding_x: 5.0,
            padding_y: 7.0,
            window: true,
            ..RenderOptions::default()
        };
        let svg = render(&timeline("", 10, 3), &options).unwrap();
        let document = roxmltree::Document::parse(&svg).unwrap();
        let root = document.root_element();

        assert_eq!(root.attribute("width"), Some("150"));
        assert_eq!(root.attribute("height"), Some("139.13"));
    }

    #[test]
    fn emits_valid_escaped_xml() {
        let svg = render(
            &timeline("[0,\"o\",\"<&>\"]\n", 8, 1),
            &RenderOptions {
                font_family: "A & \"B\"".to_owned(),
                ..RenderOptions::default()
            },
        )
        .unwrap();

        roxmltree::Document::parse(&svg).unwrap();
        assert!(svg.contains("&lt;&amp;&gt;"));
        assert!(svg.contains("A &amp; &quot;B&quot;"));
    }

    #[test]
    fn reuses_identical_lines_through_defs() {
        let svg = render(
            &timeline(
                "[0,\"o\",\"same\\r\\n\"]\n[1,\"o\",\"next\"]\n[1,\"o\",\"!\"]\n",
                10,
                3,
            ),
            &RenderOptions::default(),
        )
        .unwrap();

        assert!(svg.contains("id=\"l0\""));
        assert!(svg.matches("href=\"#l0\"").count() >= 2);
    }

    #[test]
    fn static_output_has_no_reel_keyframes() {
        let input = "{\"version\":3,\"term\":{\"cols\":4,\"rows\":1}}\n[1,\"o\",\"x\"]\n";
        let cast = Asciicast::parse(Cursor::new(input)).unwrap();
        let timeline = build(
            &cast,
            &TimelineOptions {
                at: Some(1.0),
                ..TimelineOptions::default()
            },
        )
        .unwrap();
        let svg = render(&timeline, &RenderOptions::default()).unwrap();

        assert!(!svg.contains("@keyframes a"));
    }

    #[test]
    fn renders_terminal_colors_and_sgr_styles() {
        let svg = render(
            &timeline("[0,\"o\",\"\\u001b[1;3;4;5;9;31;44mX\\u001b[2mY\"]\n", 4, 1),
            &RenderOptions::default(),
        )
        .unwrap();

        assert!(svg.contains("fill=\"#71bef2\""));
        assert!(svg.contains("fill:#e88388"));
        assert!(svg.contains("font-weight:700"));
        assert!(svg.contains("opacity:.5"));
        assert!(svg.contains("font-style:italic"));
        assert!(svg.contains("text-decoration:underline line-through"));
        assert!(svg.contains("animation:k 1s step-end infinite"));
    }

    #[test]
    fn no_loop_keeps_the_last_frame() {
        let svg = render(
            &timeline("[0,\"o\",\"a\"]\n[1,\"o\",\"b\"]\n", 4, 1),
            &RenderOptions {
                loop_animation: false,
                ..RenderOptions::default()
            },
        )
        .unwrap();

        assert!(svg.contains("steps(1,end) 1 forwards"));
    }

    #[test]
    fn replaces_xml_forbidden_characters() {
        assert_eq!(escape_text("a\u{ffff}b"), "a\u{fffd}b");
    }

    #[test]
    fn output_stays_compact_for_many_incremental_events() {
        let mut events = String::new();
        for _ in 0..100 {
            events.push_str("[0.04,\"o\",\"x\"]\n");
        }
        let svg = render(&timeline(&events, 120, 8), &RenderOptions::default()).unwrap();

        assert!(svg.len() < 100_000, "unexpected SVG size: {}", svg.len());
    }
}
