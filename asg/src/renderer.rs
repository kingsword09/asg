use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use anyhow::{Result, bail};
use avt::{Cell, Line, Pen};

use crate::theme::{Rgb, Theme};
use crate::timeline::Timeline;

const MONOSPACE_WIDTH_RATIO: f64 = 0.6;
pub const DEFAULT_FONT_SIZE: f64 = 16.0;
pub const DEFAULT_LINE_HEIGHT: f64 = 1.4;
pub const DEFAULT_FONT_FAMILY: &str = "'JetBrains Mono','Fira Code','SF Mono',Menlo,Consolas,'DejaVu Sans Mono','Liberation Mono','Symbols Nerd Font Mono','Symbols Nerd Font','Powerline Symbols','Apple Symbols','Segoe UI Symbol','Noto Sans Symbols 2','Noto Sans Symbols','Apple Color Emoji','Segoe UI Emoji','Noto Color Emoji',monospace";

#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// CSS font size in output pixels. The renderer snaps it to the nearest
    /// whole pixel so browser font hinting can operate at a stable size.
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
            font_size: DEFAULT_FONT_SIZE,
            line_height: DEFAULT_LINE_HEIGHT,
            font_family: DEFAULT_FONT_FAMILY.to_owned(),
            padding_x: 0.0,
            padding_y: 0.0,
            window: false,
            loop_animation: true,
            theme: Theme::default(),
        }
    }
}

/// Pixel-native terminal geometry. agg gets its clarity from integer font
/// sizes and pixel-rounded cell boundaries; using the same invariant also
/// prevents browsers from scaling an animated SVG text layer from a tiny
/// internal view box.
#[derive(Debug, Clone, Copy)]
struct Geometry {
    font_size: usize,
    cell_width: usize,
    row_height: usize,
    content_width: usize,
    content_height: usize,
    padding_x: usize,
    padding_y: usize,
}

impl Geometry {
    fn new(timeline: &Timeline, options: &RenderOptions) -> Result<Self> {
        let font_size = snap_pixel(options.font_size, "font size", false)?;
        let cell_width = ((font_size as f64 * MONOSPACE_WIDTH_RATIO).round() as usize).max(1);
        let row_height = ((font_size as f64 * options.line_height).round() as usize).max(1);
        let content_width = timeline
            .cols
            .checked_mul(cell_width)
            .ok_or_else(|| anyhow::anyhow!("SVG width is too large"))?;
        let content_height = timeline
            .rows
            .checked_mul(row_height)
            .ok_or_else(|| anyhow::anyhow!("SVG height is too large"))?;

        Ok(Self {
            font_size,
            cell_width,
            row_height,
            content_width,
            content_height,
            padding_x: snap_pixel(options.padding_x, "horizontal padding", true)?,
            padding_y: snap_pixel(options.padding_y, "vertical padding", true)?,
        })
    }

    fn letter_spacing(self) -> f64 {
        self.cell_width as f64 - self.font_size as f64 * MONOSPACE_WIDTH_RATIO
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum GraphicKind {
    Lines {
        up: bool,
        right: bool,
        down: bool,
        left: bool,
    },
    FullBlock,
    UpperHalfBlock,
    LowerHalfBlock,
}

impl GraphicKind {
    fn mergeable(self) -> bool {
        matches!(
            self,
            Self::Lines {
                up: false,
                right: true,
                down: false,
                left: true,
            } | Self::FullBlock
                | Self::UpperHalfBlock
                | Self::LowerHalfBlock
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct GraphicRun {
    col: usize,
    width: usize,
    kind: GraphicKind,
    style: TextStyle,
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
    graphics: Vec<GraphicRun>,
    text: Vec<TextRun>,
}

impl RenderedLine {
    fn is_empty(&self) -> bool {
        self.backgrounds.is_empty() && self.graphics.is_empty() && self.text.is_empty()
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

    let geometry = Geometry::new(timeline, options)?;

    let (width, height, content_x, content_y, radius) = if options.window {
        (
            geometry.content_width + (geometry.padding_x + 20) * 2,
            geometry.content_height + geometry.padding_y * 2 + 60,
            geometry.padding_x + 15,
            geometry.padding_y + 50,
            5,
        )
    } else {
        (
            geometry.content_width + geometry.padding_x * 2,
            geometry.content_height + geometry.padding_y * 2,
            geometry.padding_x,
            geometry.padding_y,
            0,
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
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
        width, height, width, height
    )?;
    write!(
        svg,
        "<rect width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{}\"/>",
        width, height, radius, options.theme.background
    )?;

    if options.window {
        svg.push_str(
            "<circle cx=\"20\" cy=\"20\" r=\"6\" fill=\"#ff5f58\"/><circle cx=\"40\" cy=\"20\" r=\"6\" fill=\"#ffbd2e\"/><circle cx=\"60\" cy=\"20\" r=\"6\" fill=\"#18c132\"/>",
        );
    }

    write!(
        svg,
        "<svg x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" overflow=\"hidden\">",
        content_x,
        content_y,
        geometry.content_width,
        geometry.content_height,
        geometry.content_width,
        geometry.content_height
    )?;

    write_styles(
        &mut svg,
        timeline,
        options,
        &style_classes,
        rendered_frames.len(),
        geometry,
    )?;
    write!(
        svg,
        "<g font-family=\"{}\" font-size=\"{}\" fill=\"{}\" shape-rendering=\"crispEdges\">",
        escape_attribute(&options.font_family),
        geometry.font_size,
        options.theme.foreground
    )?;

    svg.push_str("<defs>");
    write!(
        svg,
        "<g id=\"c\"><rect width=\"{}\" height=\"{}\" fill=\"{}\"/></g>",
        geometry.cell_width, geometry.row_height, options.theme.cursor
    )?;
    for (line, id) in &registry {
        write!(svg, "<g id=\"{id}\">")?;
        write_line(
            &mut svg,
            line,
            geometry.cell_width,
            geometry.font_size,
            geometry.row_height,
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
            frame_index * geometry.content_width
        )?;

        if let Some((col, row)) = frame.snapshot.cursor
            && col <= timeline.cols
            && row < timeline.rows
        {
            let x = col * geometry.cell_width;
            let y = row * geometry.row_height;
            write!(svg, "<use href=\"#c\" x=\"{}\" y=\"{}\"/>", x, y)?;
        }

        for (row, line) in lines.iter().enumerate() {
            if line.is_empty() {
                continue;
            }

            if let Some(id) = references.get(line) {
                write!(
                    svg,
                    "<use href=\"#{id}\" y=\"{}\"/>",
                    row * geometry.row_height
                )?;
            } else {
                write!(
                    svg,
                    "<g transform=\"translate(0 {})\">",
                    row * geometry.row_height
                )?;
                write_line(
                    &mut svg,
                    line,
                    geometry.cell_width,
                    geometry.font_size,
                    geometry.row_height,
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

fn snap_pixel(value: f64, name: &str, allow_zero: bool) -> Result<usize> {
    let rounded = value.round();
    if rounded > u32::MAX as f64 {
        bail!("{name} is too large");
    }

    let pixels = rounded as usize;
    Ok(if allow_zero { pixels } else { pixels.max(1) })
}

fn render_line(line: &Line, cols: usize, theme: &Theme) -> RenderedLine {
    let cells = &line.cells()[..line.cells().len().min(cols)];
    RenderedLine {
        backgrounds: background_runs(cells, theme),
        graphics: graphic_runs(cells, theme),
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

fn graphic_runs(cells: &[Cell], theme: &Theme) -> Vec<GraphicRun> {
    let mut output = Vec::new();
    let mut current: Option<GraphicRun> = None;

    for (col, cell) in cells.iter().enumerate() {
        let width = usize::from(cell.width());
        if width == 0 {
            continue;
        }

        let style = style(cell.pen(), theme);
        let Some(kind) = graphic_kind(cell.char()).filter(|_| supports_graphic_style(style)) else {
            if let Some(run) = current.take() {
                output.push(run);
            }
            continue;
        };

        match &mut current {
            Some(run)
                if kind.mergeable()
                    && run.kind == kind
                    && run.style == style
                    && run.col + run.width == col =>
            {
                run.width += width;
            }
            Some(_) => {
                output.push(
                    current
                        .replace(GraphicRun {
                            col,
                            width,
                            kind,
                            style,
                        })
                        .expect("graphic run exists"),
                );
            }
            None => {
                current = Some(GraphicRun {
                    col,
                    width,
                    kind,
                    style,
                });
            }
        }
    }
    if let Some(run) = current {
        output.push(run);
    }

    output
}

fn supports_graphic_style(style: TextStyle) -> bool {
    !style.italic && !style.underline && !style.strikethrough
}

fn graphic_kind(ch: char) -> Option<GraphicKind> {
    use GraphicKind::{FullBlock, Lines, LowerHalfBlock, UpperHalfBlock};

    let lines = |up, right, down, left| Lines {
        up,
        right,
        down,
        left,
    };

    Some(match ch {
        '─' => lines(false, true, false, true),
        '│' => lines(true, false, true, false),
        '┌' => lines(false, true, true, false),
        '┐' => lines(false, false, true, true),
        '└' => lines(true, true, false, false),
        '┘' => lines(true, false, false, true),
        '├' => lines(true, true, true, false),
        '┤' => lines(true, false, true, true),
        '┬' => lines(false, true, true, true),
        '┴' => lines(true, true, false, true),
        '┼' => lines(true, true, true, true),
        '█' => FullBlock,
        '▀' => UpperHalfBlock,
        '▄' => LowerHalfBlock,
        _ => return None,
    })
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

        if graphic_kind(cell.char()).is_some() && supports_graphic_style(style) {
            flush(&mut output, pending.take());
            continue;
        }

        // A browser fallback font may use a different advance for CJK/emoji.
        // Keep wide glyphs isolated so the next run always starts at the
        // terminal's integer cell boundary instead of inheriting that advance.
        if width > 1 {
            flush(&mut output, pending.take());
            if cell.char() != ' ' {
                output.push(TextRun {
                    col,
                    text: cell.char().to_string(),
                    style,
                });
            }
            continue;
        }

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
        .flat_map(|line| {
            line.text
                .iter()
                .map(|run| run.style)
                .chain(line.graphics.iter().map(|run| run.style))
        })
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
    geometry: Geometry,
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
                index * geometry.content_width
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
                (frame_count - 1) * geometry.content_width
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
    write!(
        output,
        "text{{white-space:pre;font-kerning:none;font-variant-ligatures:none;font-synthesis:none;font-optical-sizing:none;-webkit-font-smoothing:antialiased;text-rendering:optimizeLegibility;text-decoration-skip-ink:none;text-decoration-thickness:1px;text-underline-offset:2px;letter-spacing:{}px}}</style>",
        number(geometry.letter_spacing())
    )?;

    Ok(())
}

fn write_line(
    output: &mut String,
    line: &RenderedLine,
    cell_width: usize,
    font_size: usize,
    row_height: usize,
    classes: &BTreeMap<TextStyle, String>,
) -> std::fmt::Result {
    for run in &line.backgrounds {
        write!(
            output,
            "<rect x=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
            run.col * cell_width,
            run.width * cell_width,
            row_height,
            run.color
        )?;
    }
    let mut graphics = line.graphics.iter().peekable();
    while let Some(run) = graphics.next() {
        output.push_str("<path d=\"");
        write_graphic_path(output, run, cell_width, font_size, row_height)?;
        while graphics.peek().is_some_and(|next| next.style == run.style) {
            write_graphic_path(
                output,
                graphics.next().expect("peeked graphic exists"),
                cell_width,
                font_size,
                row_height,
            )?;
        }
        output.push('"');
        if let Some(class) = classes.get(&run.style) {
            write!(output, " class=\"{class}\"")?;
        }
        output.push_str("/>");
    }
    for run in &line.text {
        output.push_str("<text");
        if run.col > 0 {
            write!(output, " x=\"{}\"", run.col * cell_width)?;
        }
        write!(output, " y=\"{}\"", font_size)?;
        if let Some(class) = classes.get(&run.style) {
            write!(output, " class=\"{class}\"")?;
        }
        output.push('>');
        output.push_str(&escape_text(&run.text));
        output.push_str("</text>");
    }

    Ok(())
}

fn write_graphic_path(
    output: &mut String,
    run: &GraphicRun,
    cell_width: usize,
    font_size: usize,
    row_height: usize,
) -> std::fmt::Result {
    let x = run.col * cell_width;
    let width = run.width * cell_width;
    let thickness = (font_size.saturating_add(8) / 16)
        .max(1)
        .min(cell_width)
        .min(row_height);
    let center_x = (cell_width - thickness) / 2;
    let center_y = (row_height - thickness) / 2;
    match run.kind {
        GraphicKind::Lines {
            up,
            right,
            down,
            left,
        } => {
            if left || right {
                let left_x = if left { x } else { x + center_x };
                let right_x = if right {
                    x + width
                } else {
                    x + center_x + thickness
                };
                write_rect_path(output, left_x, center_y, right_x - left_x, thickness)?;
            }
            if up || down {
                let top_y = if up { 0 } else { center_y };
                let bottom_y = if down {
                    row_height
                } else {
                    center_y + thickness
                };
                write_rect_path(output, x + center_x, top_y, thickness, bottom_y - top_y)?;
            }
        }
        GraphicKind::FullBlock => {
            write_rect_path(output, x, 0, width, row_height)?;
        }
        GraphicKind::UpperHalfBlock => {
            write_rect_path(output, x, 0, width, row_height / 2)?;
        }
        GraphicKind::LowerHalfBlock => {
            let top = row_height / 2;
            write_rect_path(output, x, top, width, row_height - top)?;
        }
    }

    Ok(())
}

fn write_rect_path(
    output: &mut String,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> std::fmt::Result {
    write!(output, "M{x} {y}h{width}v{height}H{x}")
}

fn estimate_capacity(timeline: &Timeline, frames: &[Vec<RenderedLine>]) -> usize {
    let text_bytes = frames
        .iter()
        .flatten()
        .flat_map(|line| &line.text)
        .map(|run| run.text.len())
        .sum::<usize>();
    let graphic_runs = frames
        .iter()
        .flatten()
        .map(|line| line.graphics.len())
        .sum::<usize>();
    1_024 + timeline.frames.len() * 96 + text_bytes * 2 + graphic_runs * 48
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
    escape_xml(value, false, true)
}

fn escape_attribute(value: &str) -> String {
    escape_xml(value, true, false)
}

fn escape_xml(value: &str, attribute: bool, prefer_text_symbols: bool) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
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
        if prefer_text_symbols
            && prefers_text_presentation(ch)
            && !matches!(chars.peek(), Some('\u{fe0e}' | '\u{fe0f}'))
        {
            output.push('\u{fe0e}');
        }
    }
    output
}

/// Emoji-capable characters whose Unicode default is emoji but which also
/// define a standardized VS15 text presentation. agg resolves these through
/// its bundled symbol font before color emoji; requesting the text variant
/// gives browser SVG renderers the same terminal-style result without
/// embedding a multi-megabyte font. Source: Unicode 16.0 emoji-data.txt ∩
/// emoji-variation-sequences.txt.
fn prefers_text_presentation(ch: char) -> bool {
    matches!(
        ch,
        '\u{231a}'..='\u{231b}'
            | '\u{23e9}'..='\u{23ec}'
            | '\u{23f0}'
            | '\u{23f3}'
            | '\u{25fd}'..='\u{25fe}'
            | '\u{2614}'..='\u{2615}'
            | '\u{2648}'..='\u{2653}'
            | '\u{267f}'
            | '\u{2693}'
            | '\u{26a1}'
            | '\u{26aa}'..='\u{26ab}'
            | '\u{26bd}'..='\u{26be}'
            | '\u{26c4}'..='\u{26c5}'
            | '\u{26ce}'
            | '\u{26d4}'
            | '\u{26ea}'
            | '\u{26f2}'..='\u{26f3}'
            | '\u{26f5}'
            | '\u{26fa}'
            | '\u{26fd}'
            | '\u{2705}'
            | '\u{270a}'..='\u{270b}'
            | '\u{2728}'
            | '\u{274c}'
            | '\u{274e}'
            | '\u{2753}'..='\u{2755}'
            | '\u{2757}'
            | '\u{2795}'..='\u{2797}'
            | '\u{27b0}'
            | '\u{27bf}'
            | '\u{2b1b}'..='\u{2b1c}'
            | '\u{2b50}'
            | '\u{2b55}'
            | '\u{1f004}'
            | '\u{1f21a}'
            | '\u{1f22f}'
            | '\u{1f30d}'..='\u{1f30f}'
            | '\u{1f315}'
            | '\u{1f31c}'
            | '\u{1f378}'
            | '\u{1f393}'
            | '\u{1f3a7}'
            | '\u{1f3ac}'..='\u{1f3ae}'
            | '\u{1f3c2}'
            | '\u{1f3c4}'
            | '\u{1f3c6}'
            | '\u{1f3ca}'
            | '\u{1f3e0}'
            | '\u{1f3ed}'
            | '\u{1f408}'
            | '\u{1f415}'
            | '\u{1f41f}'
            | '\u{1f426}'
            | '\u{1f442}'
            | '\u{1f446}'..='\u{1f449}'
            | '\u{1f44d}'..='\u{1f44e}'
            | '\u{1f453}'
            | '\u{1f46a}'
            | '\u{1f47d}'
            | '\u{1f4a3}'
            | '\u{1f4b0}'
            | '\u{1f4b3}'
            | '\u{1f4bb}'
            | '\u{1f4bf}'
            | '\u{1f4cb}'
            | '\u{1f4da}'
            | '\u{1f4df}'
            | '\u{1f4e4}'..='\u{1f4e6}'
            | '\u{1f4ea}'..='\u{1f4ed}'
            | '\u{1f4f7}'
            | '\u{1f4f9}'..='\u{1f4fb}'
            | '\u{1f508}'
            | '\u{1f50d}'
            | '\u{1f512}'..='\u{1f513}'
            | '\u{1f550}'..='\u{1f567}'
            | '\u{1f610}'
            | '\u{1f687}'
            | '\u{1f68d}'
            | '\u{1f691}'
            | '\u{1f694}'
            | '\u{1f698}'
            | '\u{1f6ad}'
            | '\u{1f6b2}'
            | '\u{1f6b9}'..='\u{1f6ba}'
            | '\u{1f6bc}'
    )
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
    fn uses_pixel_native_default_canvas_geometry() {
        let svg = render(
            &timeline("[0,\"o\",\"Hello\"]\n", 10, 3),
            &RenderOptions::default(),
        )
        .unwrap();
        let document = roxmltree::Document::parse(&svg).unwrap();
        let root = document.root_element();

        assert_eq!(root.attribute("width"), Some("100"));
        assert_eq!(root.attribute("height"), Some("66"));
        assert_eq!(root.attribute("viewBox"), Some("0 0 100 66"));
        assert!(svg.contains("width=\"100\" height=\"66\" viewBox=\"0 0 100 66\""));
        assert!(svg.contains("font-size=\"16\""));
        assert!(svg.contains("shape-rendering=\"crispEdges\""));
        assert!(svg.contains("font-kerning:none"));
        assert!(svg.contains("font-variant-ligatures:none"));
        assert!(svg.contains("-webkit-font-smoothing:antialiased"));
        assert!(svg.contains("text-rendering:optimizeLegibility"));
        assert!(svg.contains("letter-spacing:0.4px"));
        assert!(svg.contains("<text y=\"16\">Hello</text>"));
    }

    #[test]
    fn uses_snapped_window_and_padding_geometry() {
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
        assert_eq!(root.attribute("height"), Some("140"));
        assert!(svg.contains("<svg x=\"20\" y=\"57\" width=\"100\" height=\"66\""));
    }

    #[test]
    fn animation_and_text_stay_on_the_physical_pixel_grid() {
        let svg = render(
            &timeline("[0,\"o\",\"A\"]\n[1,\"o\",\"\\r  B\"]\n", 10, 3),
            &RenderOptions::default(),
        )
        .unwrap();

        assert!(svg.contains("transform=\"translate(100 0)\""));
        assert!(svg.contains("transform:translateX(-100px)"));
        assert!(svg.contains("<text x=\"20\" y=\"16\">B</text>"));
        assert!(!svg.contains("x=\"20."));
        assert!(!svg.contains("y=\"16."));
    }

    #[test]
    fn isolates_wide_glyphs_at_terminal_cell_boundaries() {
        let svg = render(
            &timeline("[0,\"o\",\"界A\"]\n", 4, 1),
            &RenderOptions::default(),
        )
        .unwrap();

        assert!(svg.contains("<text y=\"16\">界</text><text x=\"20\" y=\"16\">A</text>"));
    }

    #[test]
    fn renders_terminal_graphics_as_crisp_pixel_geometry() {
        let svg = render(
            &timeline("[0,\"o\",\"┌──┐\\r\\n│█▀▄│\\r\\n└──┘\"]\n", 5, 3),
            &RenderOptions::default(),
        )
        .unwrap();

        assert!(!svg.contains(">─</text>"));
        assert!(!svg.contains(">█</text>"));
        assert_eq!(svg.matches("<path ").count(), 3);
        assert!(svg.contains("M10 10h20v1H10"));
        assert!(svg.contains("M10 0h10v22H10"));
        assert!(svg.contains("M20 0h10v11H20"));
        assert!(svg.contains("M30 11h10v11H30"));
    }

    #[test]
    fn derives_pixel_geometry_from_a_custom_font_size() {
        let svg = render(
            &timeline("[0,\"o\",\"A\"]\n", 10, 3),
            &RenderOptions {
                font_size: 20.0,
                line_height: 1.5,
                ..RenderOptions::default()
            },
        )
        .unwrap();
        let document = roxmltree::Document::parse(&svg).unwrap();
        let root = document.root_element();

        assert_eq!(root.attribute("width"), Some("120"));
        assert_eq!(root.attribute("height"), Some("90"));
        assert!(svg.contains("font-size=\"20\""));
        assert!(svg.contains("letter-spacing:0px"));
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
    fn requests_text_presentation_for_terminal_symbols() {
        assert_eq!(escape_text("⚡ 😀 ⚡️"), "⚡︎ 😀 ⚡️");
        assert_eq!(escape_attribute("⚡"), "⚡");
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
