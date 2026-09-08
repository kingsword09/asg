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
    /// Font files to subset and embed as data-URI @font-face rules.
    /// Requires the `embed-fonts` feature; ignored with a warning otherwise.
    pub font_files: Vec<std::path::PathBuf>,
    pub padding_x: f64,
    pub padding_y: f64,
    pub window: bool,
    pub loop_animation: bool,
    /// Draw box-drawing and block elements as crisp cell-aligned SVG paths.
    /// Set to false to render those characters with the font's own glyphs.
    pub synthetic_symbols: bool,
    pub theme: Theme,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            font_size: DEFAULT_FONT_SIZE,
            line_height: DEFAULT_LINE_HEIGHT,
            font_family: DEFAULT_FONT_FAMILY.to_owned(),
            font_files: Vec::new(),
            padding_x: 0.0,
            padding_y: 0.0,
            window: false,
            loop_animation: true,
            synthetic_symbols: true,
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
    advance_ratio: f64,
    cell_width: usize,
    row_height: usize,
    content_width: usize,
    content_height: usize,
    padding_x: usize,
    padding_y: usize,
}

/// Measure the cell width from the supplied font files before layout. The
/// first face reporting a usable advance wins, matching the CSS font stack
/// where the first supplied face is the primary text face.
#[cfg(feature = "embed-fonts")]
fn measure_advance_ratio(options: &RenderOptions) -> Option<f64> {
    for path in &options.font_files {
        let Ok(data) = std::fs::read(path) else {
            // A missing or unreadable file keeps failing the render later,
            // when collect_embedded_faces loads it for embedding.
            continue;
        };
        match crate::fonts::advance_ratio(&data) {
            Ok(Some(ratio)) => return Some(ratio),
            Ok(None) => log::warn!(
                "cannot measure an advance from {}; falling back to the next font",
                path.display()
            ),
            Err(error) => log::warn!("cannot measure an advance from {}: {error}", path.display()),
        }
    }
    None
}

#[cfg(not(feature = "embed-fonts"))]
fn measure_advance_ratio(_options: &RenderOptions) -> Option<f64> {
    None
}

impl Geometry {
    fn new(
        timeline: &Timeline,
        options: &RenderOptions,
        advance_ratio: Option<f64>,
    ) -> Result<Self> {
        let font_size = snap_pixel(options.font_size, "font size", false)?;
        // A measured advance reproduces the primary face's metrics exactly on
        // every viewer; 0.6em keeps the classic svg-term geometry otherwise.
        let ratio = advance_ratio
            .unwrap_or(MONOSPACE_WIDTH_RATIO)
            .clamp(0.0, 1.0);
        let cell_width = ((font_size as f64 * ratio).round() as usize).max(1);
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
            advance_ratio: ratio,
            cell_width,
            row_height,
            content_width,
            content_height,
            padding_x: snap_pixel(options.padding_x, "horizontal padding", true)?,
            padding_y: snap_pixel(options.padding_y, "vertical padding", true)?,
        })
    }

    fn letter_spacing(self) -> f64 {
        // Bridge the font's fractional-pixel advance to the integer cell grid.
        self.cell_width as f64 - self.font_size as f64 * self.advance_ratio
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
    BoxDrawing(char),
    Block(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Thickness {
    Light,
    Heavy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LineStyle {
    Single(Thickness),
    Double,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Orientation {
    Horizontal,
    Vertical,
}

impl Orientation {
    fn swap(self) -> Self {
        match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Half {
    First,
    Last,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LinePosition {
    Before,
    Middle,
    After,
}

impl LinePosition {
    fn opposite(self) -> Self {
        match self {
            Self::Before => Self::After,
            Self::Middle => Self::Middle,
            Self::After => Self::Before,
        }
    }

    fn half(self) -> Half {
        match self {
            Self::Before => Half::First,
            Self::Middle => Half::Both,
            Self::After => Half::Last,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Corner {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

type BoxJoint = (
    Option<Thickness>,
    Option<Thickness>,
    Option<Thickness>,
    Option<Thickness>,
);

impl GraphicKind {
    fn mergeable(self) -> bool {
        match self {
            Self::BoxDrawing(ch) => matches!(ch, '─' | '━' | '═'),
            Self::Block(ch) => matches!(ch, '▀'..='█' | '▔'),
        }
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

    let advance_ratio = measure_advance_ratio(options);
    let geometry = Geometry::new(timeline, options, advance_ratio)?;

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
                .map(|line| {
                    render_line(
                        line,
                        timeline.cols,
                        &options.theme,
                        options.synthetic_symbols,
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let embedded_faces = collect_embedded_faces(&rendered_frames, options)?;

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
        &embedded_faces,
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

/// Include rendered text and the space and text-presentation selector used
/// during output. Characters rendered as paths need no font glyphs.
#[cfg(feature = "embed-fonts")]
fn collect_codepoints(frames: &[Vec<RenderedLine>]) -> BTreeSet<char> {
    let mut codepoints: BTreeSet<char> = frames
        .iter()
        .flatten()
        .flat_map(|line| line.text.iter().flat_map(|run| run.text.chars()))
        .map(xml_character)
        .collect();
    codepoints.insert(' ');
    codepoints.insert('\u{fe0e}');
    codepoints
}

/// Parse, subset, and encode every supplied font file. Warns when an embedded
/// family is not reachable through the font-family stack, since the browser
/// would never use the embedded face in that case.
#[cfg(feature = "embed-fonts")]
fn collect_embedded_faces(
    frames: &[Vec<RenderedLine>],
    options: &RenderOptions,
) -> Result<Vec<crate::fonts::EmbeddedFace>> {
    if options.font_files.is_empty() {
        return Ok(Vec::new());
    }

    let faces = crate::fonts::load_faces(&options.font_files, &collect_codepoints(frames))?;
    let stack = options.font_family.to_ascii_lowercase();
    for face in &faces {
        if !stack.contains(&face.family.to_ascii_lowercase()) {
            log::warn!(
                "embedded family '{}' is not listed in --font-family; the browser will never use it",
                face.family
            );
        }
    }
    Ok(faces)
}

#[cfg(not(feature = "embed-fonts"))]
fn collect_embedded_faces(
    _frames: &[Vec<RenderedLine>],
    options: &RenderOptions,
) -> Result<EmbeddedFaces> {
    if !options.font_files.is_empty() {
        log::warn!("--font-file requires the `embed-fonts` feature; ignoring font files");
    }
    Ok(Vec::new())
}

fn snap_pixel(value: f64, name: &str, allow_zero: bool) -> Result<usize> {
    let rounded = value.round();
    if rounded > u32::MAX as f64 {
        bail!("{name} is too large");
    }

    let pixels = rounded as usize;
    Ok(if allow_zero { pixels } else { pixels.max(1) })
}

fn render_line(line: &Line, cols: usize, theme: &Theme, synthetic_symbols: bool) -> RenderedLine {
    let cells = &line.cells()[..line.cells().len().min(cols)];
    RenderedLine {
        backgrounds: background_runs(cells, theme),
        graphics: if synthetic_symbols {
            graphic_runs(cells, theme)
        } else {
            Vec::new()
        },
        text: text_runs(cells, theme, synthetic_symbols),
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
    match ch {
        '\u{2500}'..='\u{257f}' => Some(GraphicKind::BoxDrawing(ch)),
        '\u{2580}'..='\u{2590}' | '\u{2594}'..='\u{259f}' => Some(GraphicKind::Block(ch)),
        // Solid path geometry cannot represent the density of the shade glyphs
        // without substantially increasing SVG size, so retain the font forms.
        '\u{2591}'..='\u{2593}' => None,
        _ => None,
    }
}

fn text_runs(cells: &[Cell], theme: &Theme, synthetic_symbols: bool) -> Vec<TextRun> {
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

        if synthetic_symbols && graphic_kind(cell.char()).is_some() && supports_graphic_style(style)
        {
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

/// A face ready to embed, or a no-op placeholder when font embedding is
/// compiled out.
#[cfg(feature = "embed-fonts")]
type EmbeddedFaces = Vec<crate::fonts::EmbeddedFace>;
#[cfg(not(feature = "embed-fonts"))]
type EmbeddedFaces = Vec<std::convert::Infallible>;

fn write_styles(
    output: &mut String,
    timeline: &Timeline,
    options: &RenderOptions,
    classes: &BTreeMap<TextStyle, String>,
    frame_count: usize,
    geometry: Geometry,
    embedded_faces: &EmbeddedFaces,
) -> std::fmt::Result {
    output.push_str("<style>");
    #[cfg(feature = "embed-fonts")]
    crate::fonts::write_font_faces(output, embedded_faces)?;
    #[cfg(not(feature = "embed-fonts"))]
    let _ = embedded_faces;
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
    let thin = (font_size.saturating_add(8) / 16)
        .max(1)
        .min(cell_width)
        .min(row_height);
    match run.kind {
        GraphicKind::BoxDrawing(ch) => {
            write_box_drawing_path(output, ch, x, width, row_height, thin)?
        }
        GraphicKind::Block(ch) => write_block_path(output, ch, x, width, row_height)?,
    }

    Ok(())
}

fn write_box_drawing_path(
    output: &mut String,
    ch: char,
    x: usize,
    width: usize,
    height: usize,
    thin: usize,
) -> std::fmt::Result {
    use Half::{Both, First, Last};
    use Orientation::{Horizontal, Vertical};

    let heavy = thin.saturating_mul(2).min(width).min(height).max(1);
    let offset = thin.saturating_mul(2).max(1);

    if let Some((corner, horizontal, vertical)) = box_corner(ch) {
        return write_corner_path(
            output, x, width, height, thin, heavy, offset, corner, horizontal, vertical,
        );
    }
    if let Some((up, right, down, left)) = box_joint(ch) {
        return write_joint_path(
            output, x, width, height, thin, heavy, offset, up, right, down, left,
        );
    }

    let single = |output: &mut String, orientation, half, thickness, target_thickness| {
        write_line_segment(
            output,
            x,
            width,
            height,
            offset,
            orientation,
            half,
            LinePosition::Middle,
            LinePosition::Middle,
            thickness,
            target_thickness,
        )
    };

    match ch {
        '─' => single(output, Horizontal, Both, thin, thin),
        '━' => single(output, Horizontal, Both, heavy, heavy),
        '│' => single(output, Vertical, Both, thin, thin),
        '┃' => single(output, Vertical, Both, heavy, heavy),
        '┄' => write_dashed_line(output, x, width, height, Horizontal, thin, 3),
        '┅' => write_dashed_line(output, x, width, height, Horizontal, heavy, 3),
        '┆' => write_dashed_line(output, x, width, height, Vertical, thin, 3),
        '┇' => write_dashed_line(output, x, width, height, Vertical, heavy, 3),
        '┈' => write_dashed_line(output, x, width, height, Horizontal, thin, 4),
        '┉' => write_dashed_line(output, x, width, height, Horizontal, heavy, 4),
        '┊' => write_dashed_line(output, x, width, height, Vertical, thin, 4),
        '┋' => write_dashed_line(output, x, width, height, Vertical, heavy, 4),
        '╌' => write_dashed_line(output, x, width, height, Horizontal, thin, 2),
        '╍' => write_dashed_line(output, x, width, height, Horizontal, heavy, 2),
        '╎' => write_dashed_line(output, x, width, height, Vertical, thin, 2),
        '╏' => write_dashed_line(output, x, width, height, Vertical, heavy, 2),
        '═' => write_double_line(output, x, width, height, thin, offset, Horizontal, Both),
        '║' => write_double_line(output, x, width, height, thin, offset, Vertical, Both),
        '╞' => write_double_to_single_t(output, x, width, height, thin, offset, Vertical, Last),
        '╟' => write_single_to_double_t(
            output,
            x,
            width,
            height,
            thin,
            offset,
            Vertical,
            Last,
            LinePosition::After,
        ),
        '╠' => write_double_t(
            output,
            x,
            width,
            height,
            thin,
            offset,
            Vertical,
            LinePosition::After,
        ),
        '╡' => write_double_to_single_t(output, x, width, height, thin, offset, Vertical, First),
        '╢' => write_single_to_double_t(
            output,
            x,
            width,
            height,
            thin,
            offset,
            Vertical,
            First,
            LinePosition::Before,
        ),
        '╣' => write_double_t(
            output,
            x,
            width,
            height,
            thin,
            offset,
            Vertical,
            LinePosition::Before,
        ),
        '╤' => write_single_to_double_t(
            output,
            x,
            width,
            height,
            thin,
            offset,
            Horizontal,
            Last,
            LinePosition::After,
        ),
        '╥' => write_double_to_single_t(output, x, width, height, thin, offset, Horizontal, Last),
        '╦' => write_double_t(
            output,
            x,
            width,
            height,
            thin,
            offset,
            Horizontal,
            LinePosition::After,
        ),
        '╧' => write_single_to_double_t(
            output,
            x,
            width,
            height,
            thin,
            offset,
            Horizontal,
            First,
            LinePosition::Before,
        ),
        '╨' => {
            write_double_to_single_t(output, x, width, height, thin, offset, Horizontal, First)
        }
        '╩' => write_double_t(
            output,
            x,
            width,
            height,
            thin,
            offset,
            Horizontal,
            LinePosition::Before,
        ),
        '╪' => write_single_double_cross(output, x, width, height, thin, offset, Horizontal),
        '╫' => write_single_double_cross(output, x, width, height, thin, offset, Vertical),
        '╬' => write_double_cross(output, x, width, height, thin, offset),
        '╭' => write_rounded_corner_path(output, x, width, height, thin, Corner::TopLeft),
        '╮' => write_rounded_corner_path(output, x, width, height, thin, Corner::TopRight),
        '╯' => write_rounded_corner_path(output, x, width, height, thin, Corner::BottomRight),
        '╰' => write_rounded_corner_path(output, x, width, height, thin, Corner::BottomLeft),
        '╱' => write_diagonal_path(output, x, width, height, thin, true),
        '╲' => write_diagonal_path(output, x, width, height, thin, false),
        '╳' => {
            write_diagonal_path(output, x, width, height, thin, true)?;
            write_diagonal_path(output, x, width, height, thin, false)
        }
        '╴' => single(output, Horizontal, First, thin, thin),
        '╵' => single(output, Vertical, First, thin, thin),
        '╶' => single(output, Horizontal, Last, thin, thin),
        '╷' => single(output, Vertical, Last, thin, thin),
        '╸' => single(output, Horizontal, First, heavy, heavy),
        '╹' => single(output, Vertical, First, heavy, heavy),
        '╺' => single(output, Horizontal, Last, heavy, heavy),
        '╻' => single(output, Vertical, Last, heavy, heavy),
        '╼' => {
            single(output, Horizontal, First, thin, heavy)?;
            single(output, Horizontal, Last, heavy, thin)
        }
        '╽' => {
            single(output, Vertical, First, thin, heavy)?;
            single(output, Vertical, Last, heavy, thin)
        }
        '╾' => {
            single(output, Horizontal, First, heavy, thin)?;
            single(output, Horizontal, Last, thin, heavy)
        }
        '╿' => {
            single(output, Vertical, First, heavy, thin)?;
            single(output, Vertical, Last, thin, heavy)
        }
        _ => Ok(()),
    }
}

fn box_corner(ch: char) -> Option<(Corner, LineStyle, LineStyle)> {
    use Corner::{BottomLeft, BottomRight, TopLeft, TopRight};
    use LineStyle::{Double, Single};
    use Thickness::{Heavy, Light};

    Some(match ch {
        '┌' => (TopLeft, Single(Light), Single(Light)),
        '┍' => (TopLeft, Single(Heavy), Single(Light)),
        '┎' => (TopLeft, Single(Light), Single(Heavy)),
        '┏' => (TopLeft, Single(Heavy), Single(Heavy)),
        '╒' => (TopLeft, Double, Single(Light)),
        '╓' => (TopLeft, Single(Light), Double),
        '╔' => (TopLeft, Double, Double),
        '┐' => (TopRight, Single(Light), Single(Light)),
        '┑' => (TopRight, Single(Heavy), Single(Light)),
        '┒' => (TopRight, Single(Light), Single(Heavy)),
        '┓' => (TopRight, Single(Heavy), Single(Heavy)),
        '╕' => (TopRight, Double, Single(Light)),
        '╖' => (TopRight, Single(Light), Double),
        '╗' => (TopRight, Double, Double),
        '└' => (BottomLeft, Single(Light), Single(Light)),
        '┕' => (BottomLeft, Single(Heavy), Single(Light)),
        '┖' => (BottomLeft, Single(Light), Single(Heavy)),
        '┗' => (BottomLeft, Single(Heavy), Single(Heavy)),
        '╘' => (BottomLeft, Double, Single(Light)),
        '╙' => (BottomLeft, Single(Light), Double),
        '╚' => (BottomLeft, Double, Double),
        '┘' => (BottomRight, Single(Light), Single(Light)),
        '┙' => (BottomRight, Single(Heavy), Single(Light)),
        '┚' => (BottomRight, Single(Light), Single(Heavy)),
        '┛' => (BottomRight, Single(Heavy), Single(Heavy)),
        '╛' => (BottomRight, Double, Single(Light)),
        '╜' => (BottomRight, Single(Light), Double),
        '╝' => (BottomRight, Double, Double),
        _ => return None,
    })
}

fn box_joint(ch: char) -> Option<BoxJoint> {
    use Thickness::{Heavy as H, Light as L};

    Some(match ch {
        '├' => (Some(L), Some(L), Some(L), None),
        '┝' => (Some(L), Some(H), Some(L), None),
        '┞' => (Some(H), Some(L), Some(L), None),
        '┟' => (Some(L), Some(L), Some(H), None),
        '┠' => (Some(H), Some(L), Some(H), None),
        '┡' => (Some(H), Some(H), Some(L), None),
        '┢' => (Some(L), Some(H), Some(H), None),
        '┣' => (Some(H), Some(H), Some(H), None),
        '┤' => (Some(L), None, Some(L), Some(L)),
        '┥' => (Some(L), None, Some(L), Some(H)),
        '┦' => (Some(H), None, Some(L), Some(L)),
        '┧' => (Some(L), None, Some(H), Some(L)),
        '┨' => (Some(H), None, Some(H), Some(L)),
        '┩' => (Some(H), None, Some(L), Some(H)),
        '┪' => (Some(L), None, Some(H), Some(H)),
        '┫' => (Some(H), None, Some(H), Some(H)),
        '┬' => (None, Some(L), Some(L), Some(L)),
        '┭' => (None, Some(L), Some(L), Some(H)),
        '┮' => (None, Some(H), Some(L), Some(L)),
        '┯' => (None, Some(H), Some(L), Some(H)),
        '┰' => (None, Some(L), Some(H), Some(L)),
        '┱' => (None, Some(L), Some(H), Some(H)),
        '┲' => (None, Some(H), Some(H), Some(L)),
        '┳' => (None, Some(H), Some(H), Some(H)),
        '┴' => (Some(L), Some(L), None, Some(L)),
        '┵' => (Some(L), Some(L), None, Some(H)),
        '┶' => (Some(L), Some(H), None, Some(L)),
        '┷' => (Some(L), Some(H), None, Some(H)),
        '┸' => (Some(H), Some(L), None, Some(L)),
        '┹' => (Some(H), Some(L), None, Some(H)),
        '┺' => (Some(H), Some(H), None, Some(L)),
        '┻' => (Some(H), Some(H), None, Some(H)),
        '┼' => (Some(L), Some(L), Some(L), Some(L)),
        '┽' => (Some(L), Some(L), Some(L), Some(H)),
        '┾' => (Some(L), Some(H), Some(L), Some(L)),
        '┿' => (Some(L), Some(H), Some(L), Some(H)),
        '╀' => (Some(H), Some(L), Some(L), Some(L)),
        '╁' => (Some(L), Some(L), Some(H), Some(L)),
        '╂' => (Some(H), Some(L), Some(H), Some(L)),
        '╃' => (Some(H), Some(L), Some(L), Some(H)),
        '╄' => (Some(H), Some(H), Some(L), Some(L)),
        '╅' => (Some(L), Some(L), Some(H), Some(H)),
        '╆' => (Some(L), Some(H), Some(H), Some(L)),
        '╇' => (Some(H), Some(H), Some(L), Some(H)),
        '╈' => (Some(L), Some(H), Some(H), Some(H)),
        '╉' => (Some(H), Some(L), Some(H), Some(H)),
        '╊' => (Some(H), Some(H), Some(H), Some(L)),
        '╋' => (Some(H), Some(H), Some(H), Some(H)),
        _ => return None,
    })
}

#[allow(clippy::too_many_arguments)]
fn write_line_segment(
    output: &mut String,
    x: usize,
    width: usize,
    height: usize,
    offset: usize,
    orientation: Orientation,
    half: Half,
    position: LinePosition,
    target_position: LinePosition,
    thickness: usize,
    target_thickness: usize,
) -> std::fmt::Result {
    let (axis_length, cross_length) = match orientation {
        Orientation::Horizontal => (width, height),
        Orientation::Vertical => (height, width),
    };
    let (cross_start, cross_end) = line_band(cross_length, thickness, position, offset);
    let (target_start, target_end) =
        line_band(axis_length, target_thickness, target_position, offset);
    let (axis_start, axis_end) = match half {
        Half::First => (0, target_end),
        Half::Last => (target_start, axis_length),
        Half::Both => (0, axis_length),
    };

    match orientation {
        Orientation::Horizontal => write_rect_path(
            output,
            x + axis_start,
            cross_start,
            axis_end - axis_start,
            cross_end - cross_start,
        ),
        Orientation::Vertical => write_rect_path(
            output,
            x + cross_start,
            axis_start,
            cross_end - cross_start,
            axis_end - axis_start,
        ),
    }
}

fn line_band(
    length: usize,
    thickness: usize,
    position: LinePosition,
    offset: usize,
) -> (usize, usize) {
    let thickness = thickness.max(1).min(length);
    let middle = (length - thickness) / 2;
    let start = match position {
        LinePosition::Before => middle.saturating_sub(offset),
        LinePosition::Middle => middle,
        LinePosition::After => (middle + offset).min(length - thickness),
    };
    (start, start + thickness)
}

fn thickness_px(thickness: Thickness, thin: usize, heavy: usize) -> usize {
    match thickness {
        Thickness::Light => thin,
        Thickness::Heavy => heavy,
    }
}

fn line_style_px(style: LineStyle, thin: usize, heavy: usize) -> usize {
    match style {
        LineStyle::Single(thickness) => thickness_px(thickness, thin, heavy),
        LineStyle::Double => thin,
    }
}

#[allow(clippy::too_many_arguments)]
fn write_corner_path(
    output: &mut String,
    x: usize,
    width: usize,
    height: usize,
    thin: usize,
    heavy: usize,
    offset: usize,
    corner: Corner,
    horizontal: LineStyle,
    vertical: LineStyle,
) -> std::fmt::Result {
    let horizontal_thickness = line_style_px(horizontal, thin, heavy);
    let vertical_thickness = line_style_px(vertical, thin, heavy);
    let outer_horizontal = match (corner, horizontal) {
        (_, LineStyle::Single(_)) => LinePosition::Middle,
        (Corner::TopLeft | Corner::TopRight, LineStyle::Double) => LinePosition::Before,
        (Corner::BottomLeft | Corner::BottomRight, LineStyle::Double) => LinePosition::After,
    };
    let outer_vertical = match (corner, vertical) {
        (_, LineStyle::Single(_)) => LinePosition::Middle,
        (Corner::TopLeft | Corner::BottomLeft, LineStyle::Double) => LinePosition::Before,
        (Corner::TopRight | Corner::BottomRight, LineStyle::Double) => LinePosition::After,
    };
    let inner_horizontal = outer_horizontal.opposite();
    let inner_vertical = outer_vertical.opposite();
    let horizontal_half = match corner {
        Corner::TopLeft | Corner::BottomLeft => Half::Last,
        Corner::TopRight | Corner::BottomRight => Half::First,
    };
    let vertical_half = match corner {
        Corner::TopLeft | Corner::TopRight => Half::Last,
        Corner::BottomLeft | Corner::BottomRight => Half::First,
    };

    write_line_segment(
        output,
        x,
        width,
        height,
        offset,
        Orientation::Horizontal,
        horizontal_half,
        outer_horizontal,
        outer_vertical,
        horizontal_thickness,
        vertical_thickness,
    )?;
    write_line_segment(
        output,
        x,
        width,
        height,
        offset,
        Orientation::Vertical,
        vertical_half,
        outer_vertical,
        outer_horizontal,
        vertical_thickness,
        horizontal_thickness,
    )?;
    if horizontal == LineStyle::Double {
        write_line_segment(
            output,
            x,
            width,
            height,
            offset,
            Orientation::Horizontal,
            horizontal_half,
            inner_horizontal,
            inner_vertical,
            horizontal_thickness,
            vertical_thickness,
        )?;
    }
    if vertical == LineStyle::Double {
        write_line_segment(
            output,
            x,
            width,
            height,
            offset,
            Orientation::Vertical,
            vertical_half,
            inner_vertical,
            inner_horizontal,
            vertical_thickness,
            horizontal_thickness,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_joint_path(
    output: &mut String,
    x: usize,
    width: usize,
    height: usize,
    thin: usize,
    heavy: usize,
    offset: usize,
    up: Option<Thickness>,
    right: Option<Thickness>,
    down: Option<Thickness>,
    left: Option<Thickness>,
) -> std::fmt::Result {
    let joint = [up, right, down, left]
        .into_iter()
        .flatten()
        .map(|thickness| thickness_px(thickness, thin, heavy))
        .max()
        .unwrap_or(thin);
    for (thickness, orientation, half) in [
        (up, Orientation::Vertical, Half::First),
        (right, Orientation::Horizontal, Half::Last),
        (down, Orientation::Vertical, Half::Last),
        (left, Orientation::Horizontal, Half::First),
    ] {
        if let Some(thickness) = thickness {
            write_line_segment(
                output,
                x,
                width,
                height,
                offset,
                orientation,
                half,
                LinePosition::Middle,
                LinePosition::Middle,
                thickness_px(thickness, thin, heavy),
                joint,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_double_line(
    output: &mut String,
    x: usize,
    width: usize,
    height: usize,
    thin: usize,
    offset: usize,
    orientation: Orientation,
    half: Half,
) -> std::fmt::Result {
    for position in [LinePosition::Before, LinePosition::After] {
        write_line_segment(
            output,
            x,
            width,
            height,
            offset,
            orientation,
            half,
            position,
            LinePosition::Middle,
            thin,
            thin,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_double_to_single_t(
    output: &mut String,
    x: usize,
    width: usize,
    height: usize,
    thin: usize,
    offset: usize,
    single_orientation: Orientation,
    double_half: Half,
) -> std::fmt::Result {
    write_line_segment(
        output,
        x,
        width,
        height,
        offset,
        single_orientation,
        Half::Both,
        LinePosition::Middle,
        LinePosition::Middle,
        thin,
        thin,
    )?;
    write_double_line(
        output,
        x,
        width,
        height,
        thin,
        offset,
        single_orientation.swap(),
        double_half,
    )
}

#[allow(clippy::too_many_arguments)]
fn write_single_to_double_t(
    output: &mut String,
    x: usize,
    width: usize,
    height: usize,
    thin: usize,
    offset: usize,
    double_orientation: Orientation,
    single_half: Half,
    target: LinePosition,
) -> std::fmt::Result {
    write_double_line(
        output,
        x,
        width,
        height,
        thin,
        offset,
        double_orientation,
        Half::Both,
    )?;
    write_line_segment(
        output,
        x,
        width,
        height,
        offset,
        double_orientation.swap(),
        single_half,
        LinePosition::Middle,
        target,
        thin,
        thin,
    )
}

#[allow(clippy::too_many_arguments)]
fn write_double_t(
    output: &mut String,
    x: usize,
    width: usize,
    height: usize,
    thin: usize,
    offset: usize,
    orientation: Orientation,
    side: LinePosition,
) -> std::fmt::Result {
    write_line_segment(
        output,
        x,
        width,
        height,
        offset,
        orientation,
        Half::Both,
        side.opposite(),
        LinePosition::Middle,
        thin,
        thin,
    )?;
    write_line_segment(
        output,
        x,
        width,
        height,
        offset,
        orientation,
        Half::First,
        side,
        LinePosition::Before,
        thin,
        thin,
    )?;
    write_line_segment(
        output,
        x,
        width,
        height,
        offset,
        orientation,
        Half::Last,
        side,
        LinePosition::After,
        thin,
        thin,
    )?;
    for position in [LinePosition::Before, LinePosition::After] {
        write_line_segment(
            output,
            x,
            width,
            height,
            offset,
            orientation.swap(),
            side.half(),
            position,
            side,
            thin,
            thin,
        )?;
    }
    Ok(())
}

fn write_single_double_cross(
    output: &mut String,
    x: usize,
    width: usize,
    height: usize,
    thin: usize,
    offset: usize,
    double_orientation: Orientation,
) -> std::fmt::Result {
    write_double_line(
        output,
        x,
        width,
        height,
        thin,
        offset,
        double_orientation,
        Half::Both,
    )?;
    let single_orientation = double_orientation.swap();
    write_line_segment(
        output,
        x,
        width,
        height,
        offset,
        single_orientation,
        Half::First,
        LinePosition::Middle,
        LinePosition::Before,
        thin,
        thin,
    )?;
    write_line_segment(
        output,
        x,
        width,
        height,
        offset,
        single_orientation,
        Half::Last,
        LinePosition::Middle,
        LinePosition::After,
        thin,
        thin,
    )
}

fn write_double_cross(
    output: &mut String,
    x: usize,
    width: usize,
    height: usize,
    thin: usize,
    offset: usize,
) -> std::fmt::Result {
    for first in [LinePosition::Before, LinePosition::After] {
        for second in [LinePosition::Before, LinePosition::After] {
            write_line_segment(
                output,
                x,
                width,
                height,
                offset,
                Orientation::Horizontal,
                first.half(),
                second,
                first,
                thin,
                thin,
            )?;
            write_line_segment(
                output,
                x,
                width,
                height,
                offset,
                Orientation::Vertical,
                first.half(),
                second,
                first,
                thin,
                thin,
            )?;
        }
    }
    Ok(())
}

fn write_dashed_line(
    output: &mut String,
    x: usize,
    width: usize,
    height: usize,
    orientation: Orientation,
    thickness: usize,
    count: usize,
) -> std::fmt::Result {
    let axis_length = match orientation {
        Orientation::Horizontal => width,
        Orientation::Vertical => height,
    };
    let cross_length = match orientation {
        Orientation::Horizontal => height,
        Orientation::Vertical => width,
    };
    let gap = (width / (count * 2)).max(1);
    let (cross_start, cross_end) = line_band(cross_length, thickness, LinePosition::Middle, 0);
    for index in 0..count {
        let bin_start = index * axis_length / count;
        let bin_end = (index + 1) * axis_length / count;
        let mut start = bin_start + gap / 2;
        let mut end = bin_end.saturating_sub(gap - gap / 2);
        if end <= start {
            start = (bin_start + bin_end.saturating_sub(1)) / 2;
            end = (start + 1).min(axis_length);
        }
        match orientation {
            Orientation::Horizontal => write_rect_path(
                output,
                x + start,
                cross_start,
                end - start,
                cross_end - cross_start,
            )?,
            Orientation::Vertical => write_rect_path(
                output,
                x + cross_start,
                start,
                cross_end - cross_start,
                end - start,
            )?,
        }
    }
    Ok(())
}

fn write_rounded_corner_path(
    output: &mut String,
    x: usize,
    width: usize,
    height: usize,
    thin: usize,
    corner: Corner,
) -> std::fmt::Result {
    let (x0, x1) = line_band(width, thin, LinePosition::Middle, 0);
    let (y0, y1) = line_band(height, thin, LinePosition::Middle, 0);
    let x0 = x + x0;
    let x1 = x + x1;
    let right = x + width;
    match corner {
        Corner::TopLeft => write!(
            output,
            "M{right} {y0}Q{x0} {y0} {x0} {height}L{x1} {height}Q{x1} {y1} {right} {y1}Z"
        ),
        Corner::TopRight => write!(
            output,
            "M{x} {y0}Q{x1} {y0} {x1} {height}L{x0} {height}Q{x0} {y1} {x} {y1}Z"
        ),
        Corner::BottomRight => write!(
            output,
            "M{x} {y0}Q{x1} {y0} {x1} 0L{x0} 0Q{x0} {y1} {x} {y1}Z"
        ),
        Corner::BottomLeft => write!(
            output,
            "M{right} {y0}Q{x0} {y0} {x0} 0L{x1} 0Q{x1} {y1} {right} {y1}Z"
        ),
    }
}

fn write_diagonal_path(
    output: &mut String,
    x: usize,
    width: usize,
    height: usize,
    thickness: usize,
    rising: bool,
) -> std::fmt::Result {
    let thickness = thickness.min(width).min(height).max(1);
    let right = x + width;
    if rising {
        write!(
            output,
            "M{right} 0L{} 0L{x} {}L{x} {height}L{} {height}L{right} {thickness}Z",
            right - thickness,
            height - thickness,
            x + thickness,
        )
    } else {
        write!(
            output,
            "M{x} 0L{} 0L{right} {}L{right} {height}L{} {height}L{x} {thickness}Z",
            x + thickness,
            height - thickness,
            right - thickness,
        )
    }
}

fn write_block_path(
    output: &mut String,
    ch: char,
    x: usize,
    width: usize,
    height: usize,
) -> std::fmt::Result {
    match ch {
        '▀' => write_eighth_rect(output, x, width, height, Orientation::Horizontal, 0, 4),
        '▁' => write_eighth_rect(output, x, width, height, Orientation::Horizontal, 7, 8),
        '▂' => write_eighth_rect(output, x, width, height, Orientation::Horizontal, 6, 8),
        '▃' => write_eighth_rect(output, x, width, height, Orientation::Horizontal, 5, 8),
        '▄' => write_eighth_rect(output, x, width, height, Orientation::Horizontal, 4, 8),
        '▅' => write_eighth_rect(output, x, width, height, Orientation::Horizontal, 3, 8),
        '▆' => write_eighth_rect(output, x, width, height, Orientation::Horizontal, 2, 8),
        '▇' => write_eighth_rect(output, x, width, height, Orientation::Horizontal, 1, 8),
        '█' => write_rect_path(output, x, 0, width, height),
        '▉' => write_eighth_rect(output, x, width, height, Orientation::Vertical, 0, 7),
        '▊' => write_eighth_rect(output, x, width, height, Orientation::Vertical, 0, 6),
        '▋' => write_eighth_rect(output, x, width, height, Orientation::Vertical, 0, 5),
        '▌' => write_eighth_rect(output, x, width, height, Orientation::Vertical, 0, 4),
        '▍' => write_eighth_rect(output, x, width, height, Orientation::Vertical, 0, 3),
        '▎' => write_eighth_rect(output, x, width, height, Orientation::Vertical, 0, 2),
        '▏' => write_eighth_rect(output, x, width, height, Orientation::Vertical, 0, 1),
        '▐' => write_eighth_rect(output, x, width, height, Orientation::Vertical, 4, 8),
        '▔' => write_eighth_rect(output, x, width, height, Orientation::Horizontal, 0, 1),
        '▕' => write_eighth_rect(output, x, width, height, Orientation::Vertical, 7, 8),
        '▖' => write_quadrants(output, x, width, height, 0b0100),
        '▗' => write_quadrants(output, x, width, height, 0b1000),
        '▘' => write_quadrants(output, x, width, height, 0b0001),
        '▙' => write_quadrants(output, x, width, height, 0b1101),
        '▚' => write_quadrants(output, x, width, height, 0b1001),
        '▛' => write_quadrants(output, x, width, height, 0b0111),
        '▜' => write_quadrants(output, x, width, height, 0b1011),
        '▝' => write_quadrants(output, x, width, height, 0b0010),
        '▞' => write_quadrants(output, x, width, height, 0b0110),
        '▟' => write_quadrants(output, x, width, height, 0b1110),
        _ => Ok(()),
    }
}

fn write_eighth_rect(
    output: &mut String,
    x: usize,
    width: usize,
    height: usize,
    orientation: Orientation,
    start: usize,
    end: usize,
) -> std::fmt::Result {
    let eighth = |length: usize, index: usize| length / 8 * index + (length % 8 * index + 4) / 8;
    match orientation {
        Orientation::Horizontal => {
            let (top, bottom) =
                nonempty_interval(eighth(height, start), eighth(height, end), height);
            write_rect_path(output, x, top, width, bottom - top)
        }
        Orientation::Vertical => {
            let (left, right) = nonempty_interval(eighth(width, start), eighth(width, end), width);
            write_rect_path(output, x + left, 0, right - left, height)
        }
    }
}

fn nonempty_interval(start: usize, end: usize, limit: usize) -> (usize, usize) {
    if end > start {
        return (start, end);
    }
    if end < limit {
        (end, end + 1)
    } else {
        (limit.saturating_sub(1), limit)
    }
}

fn write_quadrants(
    output: &mut String,
    x: usize,
    width: usize,
    height: usize,
    mask: u8,
) -> std::fmt::Result {
    let left_end = width.div_ceil(2);
    let right_start = width / 2;
    let top_end = height.div_ceil(2);
    let bottom_start = height / 2;
    for (bit, left, top, quadrant_width, quadrant_height) in [
        (0b0001, 0, 0, left_end, top_end),
        (0b0010, right_start, 0, width - right_start, top_end),
        (0b0100, 0, bottom_start, left_end, height - bottom_start),
        (
            0b1000,
            right_start,
            bottom_start,
            width - right_start,
            height - bottom_start,
        ),
    ] {
        if mask & bit != 0 {
            write_rect_path(output, x + left, top, quadrant_width, quadrant_height)?;
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
    if width == 0 || height == 0 {
        return Ok(());
    }
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

fn xml_character(ch: char) -> char {
    match ch {
        '\u{9}'
        | '\u{a}'
        | '\u{d}'
        | '\u{20}'..='\u{d7ff}'
        | '\u{e000}'..='\u{fffd}'
        | '\u{10000}'..='\u{10ffff}' => ch,
        _ => '\u{fffd}',
    }
}

fn escape_xml(value: &str, attribute: bool, prefer_text_symbols: bool) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        let ch = xml_character(ch);
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' if attribute => output.push_str("&quot;"),
            '\'' if attribute => output.push_str("&apos;"),
            _ => output.push(ch),
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

    fn graphic_path(ch: char) -> String {
        graphic_path_at(ch, 10, 16, 22)
    }

    fn graphic_path_at(ch: char, cell_width: usize, font_size: usize, row_height: usize) -> String {
        let mut output = String::new();
        let run = GraphicRun {
            col: 0,
            width: 1,
            kind: graphic_kind(ch).expect("synthetic graphic"),
            style: default_style(&Theme::default()),
        };
        write_graphic_path(&mut output, &run, cell_width, font_size, row_height).unwrap();
        output
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
    fn renders_double_line_box_drawing_as_paired_paths() {
        let svg = render(
            &timeline("[0,\"o\",\"╔═╗\\r\\n║╬║\\r\\n╚═╝\"]\n", 5, 3),
            &RenderOptions::default(),
        )
        .unwrap();

        assert!(!svg.contains(">═</text>"));
        assert!(!svg.contains(">╬</text>"));
        // Horizontal double rules straddle the vertical center line y=10.
        assert!(svg.contains("M10 8h10v1H10"));
        assert!(svg.contains("M10 12h10v1H10"));
        // The vertical sides are paired full-height rules.
        assert!(svg.contains("M2 0h1v22H2"));
        assert!(svg.contains("M6 0h1v22H6"));
        // Both rules in the top-left corner join continuously at their bends.
        assert!(svg.contains("M2 8h8v1H2M2 8h1v14H2"));
        assert!(svg.contains("M6 12h4v1H6M6 12h1v10H6"));
    }

    #[test]
    fn renders_half_line_and_heavy_box_drawing_as_paths() {
        let svg = render(
            &timeline("[0,\"o\",\"╶─╴\\r\\n╵┃╷\"]\n", 5, 2),
            &RenderOptions::default(),
        )
        .unwrap();

        assert_eq!(svg.matches("<path ").count(), 2);
        // Half lines stop at the center line toward their open side.
        assert!(svg.contains("M4 10h6v1H4"));
        assert!(svg.contains("M20 10h5v1H20"));
        // Heavy strokes remain visibly wider than light strokes.
        assert!(svg.contains("M14 0h2v22H14"));
    }

    #[test]
    fn renders_partial_and_quadrant_blocks_as_paths() {
        let svg = render(
            &timeline("[0,\"o\",\"▁▅▌▐\\r\\n▖▝▘▗\"]\n", 8, 2),
            &RenderOptions::default(),
        )
        .unwrap();

        assert_eq!(svg.matches("<path ").count(), 2);
        // Eighths blocks fill from the bottom of the 22px cell.
        assert!(svg.contains("M0 19h10v3H0"));
        assert!(svg.contains("M10 8h10v14H10"));
        // Left/right half blocks split the cell width in half.
        assert!(svg.contains("M20 0h5v22H20"));
        assert!(svg.contains("M35 0h5v22H35"));
        // Quadrants fill one half-width by half-height rect each.
        assert!(svg.contains("M0 11h5v11H0"));
        assert!(svg.contains("M15 0h5v11H15"));
        assert!(svg.contains("M20 0h5v11H20"));
        assert!(svg.contains("M35 11h5v11H35"));
    }

    #[test]
    fn shade_blocks_keep_their_font_glyphs() {
        let svg = render(
            &timeline("[0,\"o\",\"░▒▓\"]\n", 5, 1),
            &RenderOptions::default(),
        )
        .unwrap();

        assert_eq!(svg.matches("<path ").count(), 0);
        assert!(svg.contains(">░▒▓</text>"));
    }

    #[test]
    fn covers_every_box_drawing_codepoint_with_nonempty_geometry() {
        let chars = ('\u{2500}'..='\u{257f}').collect::<String>();
        for ch in chars.chars() {
            assert!(
                !graphic_path(ch).is_empty(),
                "U+{:04X} produced no path geometry",
                u32::from(ch)
            );
        }

        let svg = render(
            &timeline(&format!("[0,\"o\",\"{chars}\"]\n"), 128, 1),
            &RenderOptions::default(),
        )
        .unwrap();
        roxmltree::Document::parse(&svg).unwrap();
        assert!(!svg.contains("<text"));
    }

    #[test]
    fn covers_every_non_shade_block_element_with_nonempty_geometry() {
        let chars = ('\u{2580}'..='\u{2590}')
            .chain('\u{2594}'..='\u{259f}')
            .collect::<String>();
        for ch in chars.chars() {
            assert!(
                !graphic_path(ch).is_empty(),
                "U+{:04X} produced no path geometry",
                u32::from(ch)
            );
        }

        let svg = render(
            &timeline(&format!("[0,\"o\",\"{chars}\"]\n"), 29, 1),
            &RenderOptions::default(),
        )
        .unwrap();
        roxmltree::Document::parse(&svg).unwrap();
        assert!(!svg.contains("<text"));
    }

    #[test]
    fn synthetic_graphics_degrade_to_nonempty_one_pixel_geometry() {
        for ch in ('\u{2500}'..='\u{2590}').chain('\u{2594}'..='\u{259f}') {
            assert!(
                !graphic_path_at(ch, 1, 1, 1).is_empty(),
                "U+{:04X} disappeared at the minimum cell size",
                u32::from(ch)
            );
        }
    }

    #[test]
    fn cell_local_graphics_are_not_merged_across_columns() {
        let svg = render(
            &timeline("[0,\"o\",\"┼┼▌▌▖▖\"]\n", 6, 1),
            &RenderOptions::default(),
        )
        .unwrap();

        assert!(svg.contains("M4 0h1v11H4"));
        assert!(svg.contains("M14 0h1v11H14"));
        assert!(svg.contains("M20 0h5v22H20"));
        assert!(svg.contains("M30 0h5v22H30"));
        assert!(svg.contains("M40 11h5v11H40"));
        assert!(svg.contains("M50 11h5v11H50"));
    }

    #[test]
    fn no_synthetic_symbols_renders_graphics_with_the_font() {
        let svg = render(
            &timeline("[0,\"o\",\"┌─┐\\r\\n│█▀▄│\\r\\n└─┘\"]\n", 5, 3),
            &RenderOptions {
                synthetic_symbols: false,
                ..RenderOptions::default()
            },
        )
        .unwrap();

        assert_eq!(svg.matches("<path ").count(), 0);
        assert!(svg.contains(">┌─┐</text>"));
        assert!(svg.contains(">│█▀▄│</text>"));
        assert!(svg.contains(">└─┘</text>"));
    }

    #[test]
    fn synthetic_symbols_default_stays_enabled() {
        let options = RenderOptions::default();
        assert!(options.synthetic_symbols);
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

    #[cfg(feature = "embed-fonts")]
    mod embed_fonts {
        use super::*;

        #[test]
        fn collects_text_glyphs_across_frames_without_path_only_graphics() {
            let timeline = timeline(
                "[0,\"o\",\"A░─\\u001b[3m│\\u001b[0m\\ufffe\"]\n[1,\"o\",\"\\u001b[2J\\u001b[HZ\"]\n",
                16,
                1,
            );
            let frames = timeline
                .frames
                .iter()
                .map(|frame| {
                    frame
                        .snapshot
                        .lines
                        .iter()
                        .map(|line| render_line(line, timeline.cols, &Theme::default(), true))
                        .collect()
                })
                .collect::<Vec<_>>();

            assert_eq!(
                collect_codepoints(&frames),
                BTreeSet::from([' ', 'A', 'Z', '░', '│', '\u{fe0e}', '\u{fffd}'])
            );
        }

        fn font_files() -> Vec<std::path::PathBuf> {
            let font_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fonts");
            vec![
                font_dir.join("AsgTestSans-Regular.ttf"),
                font_dir.join("AsgTestSans-Bold.ttf"),
            ]
        }

        fn first_subset(svg: &str) -> Vec<u8> {
            use base64::Engine as _;
            use base64::engine::general_purpose::STANDARD;
            let start = svg.find("base64,").expect("no base64 data") + "base64,".len();
            let end = svg[start..].find(')').expect("unterminated url") + start;
            STANDARD.decode(&svg[start..end]).unwrap()
        }

        #[test]
        fn embeds_font_faces_for_used_codepoints() {
            let options = RenderOptions {
                font_family: "'Asg Test Sans',monospace".to_owned(),
                font_files: font_files(),
                ..RenderOptions::default()
            };
            let svg = render(
                &timeline(
                    "[0,\"o\",\"A\"]\n[1,\"o\",\"\\u001b[1mB\\u001b[0m\"]\n",
                    10,
                    2,
                ),
                &options,
            )
            .unwrap();

            assert_eq!(svg.matches("@font-face{").count(), 2);
            assert!(svg.contains("font-family:'Asg Test Sans';font-style:normal;font-weight:400;"));
            assert!(svg.contains("font-style:normal;font-weight:700;"));
            assert!(svg.contains("format('woff2')}"));
            assert_eq!(&first_subset(&svg)[..4], b"wOF2");
        }

        #[test]
        fn missing_font_file_fails_the_render() {
            let options = RenderOptions {
                font_family: "'Asg Test Sans',monospace".to_owned(),
                font_files: vec!["/nonexistent/font.ttf".into()],
                ..RenderOptions::default()
            };
            assert!(render(&timeline("[0,\"o\",\"A\"]\n", 10, 2), &options).is_err());
        }

        #[test]
        fn without_font_files_no_font_faces_are_emitted() {
            let svg = render(
                &timeline("[0,\"o\",\"A\"]\n", 10, 2),
                &RenderOptions::default(),
            )
            .unwrap();

            assert!(!svg.contains("@font-face"));
        }

        #[test]
        fn subset_is_smaller_than_the_source_font() {
            let options = RenderOptions {
                font_family: "'Asg Test Sans',monospace".to_owned(),
                font_files: font_files(),
                ..RenderOptions::default()
            };
            let svg = render(&timeline("[0,\"o\",\"A\"]\n", 10, 2), &options).unwrap();

            let source = std::fs::read(&font_files()[0]).unwrap();
            assert!(first_subset(&svg).len() < source.len() / 2);
        }

        #[test]
        fn cell_width_is_measured_from_the_supplied_font_advance() {
            // 16px font at the fixture's measured 0.6em advance keeps the
            // classic 10px cell; a 0.55em face shrinks the cell to 9px.
            for (font, family, cell_width) in [
                ("AsgTestSans-Regular.ttf", "Asg Test Sans", 10),
                ("AsgTestNarrow-Regular.ttf", "Asg Test Narrow", 9),
            ] {
                let options = RenderOptions {
                    font_family: format!("'{family}',monospace"),
                    font_files: vec![font_dir().join(font)],
                    ..RenderOptions::default()
                };
                let svg = render(&timeline("[0,\"o\",\"A\"]\n", 10, 2), &options).unwrap();
                let document = roxmltree::Document::parse(&svg).unwrap();

                assert_eq!(
                    document.root_element().attribute("width"),
                    Some((cell_width * 10).to_string().as_str()),
                    "{font} should derive a {cell_width}px cell"
                );
                assert!(
                    svg.contains(&format!("width=\"{}\"", cell_width * 10)),
                    "{font} should size the content viewport at {cell_width}px cells"
                );
            }
        }

        #[test]
        fn measured_advance_survives_a_20px_font_size() {
            let options = RenderOptions {
                font_family: "'Asg Test Narrow',monospace".to_owned(),
                font_files: vec![font_dir().join("AsgTestNarrow-Regular.ttf")],
                font_size: 20.0,
                ..RenderOptions::default()
            };
            let svg = render(&timeline("[0,\"o\",\"A\"]\n", 10, 2), &options).unwrap();

            // round(20 * 0.55) = 11, rows * row_height = 2 * round(20 * 1.4) = 56
            assert!(svg.contains("width=\"110\""));
            assert!(svg.contains("<rect width=\"11\" height=\"28\""));
        }

        #[test]
        fn letter_spacing_compensates_the_measured_advance_rounding() {
            // 0.55em at 16px is an 8.8px advance; the 9px cell needs a 0.2px
            // letter-spacing correction so the viewer lands on the cell grid.
            let options = RenderOptions {
                font_family: "'Asg Test Narrow',monospace".to_owned(),
                font_files: vec![font_dir().join("AsgTestNarrow-Regular.ttf")],
                ..RenderOptions::default()
            };
            let svg = render(&timeline("[0,\"o\",\"A\"]\n", 10, 2), &options).unwrap();

            assert!(svg.contains("letter-spacing:0.2px"));
        }

        #[test]
        fn unreadable_advance_measures_fall_back_to_the_06em_geometry() {
            // A font file that measures no advance (here: no space glyph map)
            // must not change the classic 0.6em geometry.
            let options = RenderOptions {
                font_family: "'Asg Test Sans',monospace".to_owned(),
                font_files: vec![font_dir().join("AsgTestSerif-Regular.otf")],
                ..RenderOptions::default()
            };
            let svg = render(&timeline("[0,\"o\",\"A\"]\n", 10, 2), &options).unwrap();

            assert!(svg.contains("width=\"100\""));
        }

        fn font_dir() -> std::path::PathBuf {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fonts")
        }
    }
}
