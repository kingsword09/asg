use anyhow::Result;
use svg::Document;
use svg::node::element::{
    Animate, Circle, Definitions, Group, Rectangle, Style, Text as TextElement,
};

use crate::terminal::Frame;
use crate::theme::Theme;

pub struct SvgRenderer {
    cols: usize,
    rows: usize,
    font_size: u8,
    line_height: f32,
    theme: Theme,
    loop_enable: bool,
    cursor_visible: bool,
    window: bool,
    padding_x: u16,
    padding_y: u16,
}

impl SvgRenderer {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols,
            rows,
            font_size: 14,
            line_height: 1.4,
            theme: Theme::default(),
            loop_enable: true,
            cursor_visible: true,
            window: false,
            padding_x: 10,
            padding_y: 10,
        }
    }

    pub fn with_font_size(mut self, font_size: u8) -> Self {
        self.font_size = font_size;
        self
    }

    pub fn with_line_height(mut self, line_height: f32) -> Self {
        self.line_height = line_height;
        self
    }

    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    pub fn with_loop_enable(mut self, loop_enable: bool) -> Self {
        self.loop_enable = loop_enable;
        self
    }

    pub fn with_cursor_visible(mut self, cursor_visible: bool) -> Self {
        self.cursor_visible = cursor_visible;
        self
    }

    pub fn with_window(mut self, window: bool) -> Self {
        self.window = window;
        self
    }

    pub fn with_padding(mut self, padding_x: u16, padding_y: u16) -> Self {
        self.padding_x = padding_x;
        self.padding_y = padding_y;
        self
    }

    pub fn render(&self, frames: &[Frame], durations: &[f64]) -> Result<Document> {
        let char_width = self.font_size as f32 * 0.6;
        let line_height_px = self.font_size as f32 * self.line_height;

        let content_width = self.cols as f32 * char_width;
        let content_height = self.rows as f32 * line_height_px;

        let width = content_width + self.padding_x as f32 * 2.0;
        let mut height = content_height + self.padding_y as f32 * 2.0;

        // Add space for window decorations if enabled
        let window_bar_height = if self.window { 30.0 } else { 0.0 };
        if self.window {
            height += window_bar_height;
        }

        // Create SVG document
        let mut doc = Document::new()
            .set("width", width)
            .set("height", height)
            .set("viewBox", (0, 0, width as i32, height as i32))
            .set("xmlns", "http://www.w3.org/2000/svg");

        // Title will be added via metadata if needed

        // Generate styles and text for all frames
        let (styles, text_elements) = self.generate_styles_and_segments(frames, durations);

        // Create definitions with styles
        let defs = Definitions::new().add(Style::new(styles));
        doc = doc.add(defs);

        // Add background
        let bg = Rectangle::new()
            .set("width", "100%")
            .set("height", "100%")
            .set(
                "fill",
                format!(
                    "#{:02x}{:02x}{:02x}",
                    self.theme.bg.r, self.theme.bg.g, self.theme.bg.b
                ),
            );
        doc = doc.add(bg);

        // Add window decorations if enabled
        if self.window {
            let window_group = self.create_window_decorations(width, window_bar_height);
            doc = doc.add(window_group);
        }

        // Create frames group with proper offset
        let y_offset = self.padding_y as f32 + window_bar_height;
        let mut frames_group = Group::new().set(
            "transform",
            format!("translate({}, {})", self.padding_x, y_offset),
        );

        // Add text elements
        for element in text_elements {
            frames_group = frames_group.add(element);
        }

        doc = doc.add(frames_group);

        Ok(doc)
    }

    fn generate_styles_and_segments(
        &self,
        frames: &[Frame],
        durations: &[f64],
    ) -> (String, Vec<Group>) {
        let mut css = String::new();
        let mut frame_groups = Vec::new();

        // Basic styles
        css.push_str(&format!(
            r#"
text {{
    white-space: pre;
    font-family: monospace;
    font-size: {}px;
}}
.frame {{ opacity: 0; }}
"#,
            self.font_size
        ));

        // Calculate dimensions for positioning
        let line_height_px = self.font_size as f32 * self.line_height;
        let char_width = self.font_size as f32 * 0.6;

        if frames.is_empty() {
            return (css, frame_groups);
        }

        // Chain animations using previous frame's end; first frame also restarts after last
        let last_anim_id = format!("f{}", frames.len() - 1);
        for (i, frame) in frames.iter().enumerate() {
            // Each frame is a group with class 'frame' so default opacity is 0
            let mut frame_group = Group::new().set("class", "frame");

            for row in 0..frame.height {
                // Find last non-space character to avoid rendering trailing whitespace
                let mut last_col_opt: Option<usize> = None;
                for col in (0..frame.width).rev() {
                    if let Some(cell) = frame.get_cell(row, col)
                        && cell.ch != ' '
                    {
                        last_col_opt = Some(col);
                        break;
                    }
                }

                if let Some(last_col) = last_col_opt {
                    // Create a row group positioned at the top of the row box
                    let mut row_group = Group::new().set(
                        "transform",
                        format!("translate(0, {})", row as f32 * line_height_px),
                    );

                    // 1) Background rectangles grouped by bg color runs
                    let mut bg_group = Group::new();
                    let theme_bg = (self.theme.bg.r, self.theme.bg.g, self.theme.bg.b);
                    let mut bg_run_start: usize = 0;
                    let mut bg_run_color: Option<(u8, u8, u8)> = None;

                    // (helper removed) we'll flush bg runs inline to avoid borrow issues

                    for col in 0..=last_col {
                        if let Some(cell) = frame.get_cell(row, col) {
                            let bg_tuple = (cell.bg.r, cell.bg.g, cell.bg.b);
                            // Skip painting backgrounds that match the global background or pure black default
                            let should_paint = bg_tuple != theme_bg && bg_tuple != (0, 0, 0);
                            match (bg_run_color, should_paint) {
                                (None, true) => {
                                    bg_run_color = Some(bg_tuple);
                                    bg_run_start = col;
                                }
                                (Some(current), true) if current == bg_tuple => {
                                    // continue run
                                }
                                (Some(current), _) => {
                                    // flush and stop run
                                    if col > bg_run_start {
                                        let width = (col - bg_run_start) as f32 * char_width;
                                        let x = bg_run_start as f32 * char_width;
                                        let (r, g, b) = current;
                                        let fill = format!("#{:02x}{:02x}{:02x}", r, g, b);
                                        let rect = Rectangle::new()
                                            .set("x", x)
                                            .set("y", 0.0)
                                            .set("width", width)
                                            .set("height", line_height_px)
                                            .set("fill", fill);
                                        bg_group = bg_group.add(rect);
                                    }
                                    bg_run_color = None;
                                }
                                (None, false) => {
                                    // no-op
                                }
                            }
                        }
                    }
                    // Flush final bg run
                    if let Some(color) = bg_run_color {
                        let end = last_col + 1;
                        if end > bg_run_start {
                            let width = (end - bg_run_start) as f32 * char_width;
                            let x = bg_run_start as f32 * char_width;
                            let (r, g, b) = color;
                            let fill = format!("#{:02x}{:02x}{:02x}", r, g, b);
                            let rect = Rectangle::new()
                                .set("x", x)
                                .set("y", 0.0)
                                .set("width", width)
                                .set("height", line_height_px)
                                .set("fill", fill);
                            bg_group = bg_group.add(rect);
                        }
                    }

                    row_group = row_group.add(bg_group);

                    // 2) Foreground text grouped by (fg color + styles)
                    let mut text_group = Group::new().set(
                        "transform",
                        format!("translate(0, {})", self.font_size as f32),
                    );

                    #[derive(Clone, Copy, PartialEq, Eq)]
                    struct StyleKey {
                        fg: (u8, u8, u8),
                        bold: bool,
                        italic: bool,
                        underline: bool,
                    }

                    let mut run_text = String::new();
                    let mut run_start_col: usize = 0;
                    let mut run_key: Option<StyleKey> = None;

                    // (helper removed) we'll flush text runs inline

                    for col in 0..=last_col {
                        if let Some(cell) = frame.get_cell(row, col) {
                            let key = StyleKey {
                                fg: (cell.fg.r, cell.fg.g, cell.fg.b),
                                bold: cell.bold,
                                italic: cell.italic,
                                underline: cell.underline,
                            };
                            match run_key {
                                None => {
                                    run_key = Some(key);
                                    run_start_col = col;
                                    run_text.push(cell.ch);
                                }
                                Some(current) if current == key => {
                                    run_text.push(cell.ch);
                                }
                                Some(current) => {
                                    // flush previous
                                    if !run_text.is_empty() {
                                        let x = run_start_col as f32 * char_width;
                                        let (r, g, b) = current.fg;
                                        let fill = format!("#{:02x}{:02x}{:02x}", r, g, b);
                                        let mut el = TextElement::new(run_text.clone())
                                            .set("x", x)
                                            .set("fill", fill);
                                        if current.bold {
                                            el = el.set("font-weight", "bold");
                                        }
                                        if current.italic {
                                            el = el.set("font-style", "italic");
                                        }
                                        if current.underline {
                                            el = el.set("text-decoration", "underline");
                                        }
                                        text_group = text_group.add(el);
                                    }
                                    // start new
                                    run_key = Some(key);
                                    run_start_col = col;
                                    run_text.clear();
                                    run_text.push(cell.ch);
                                }
                            }
                        }
                    }

                    if let Some(current) = run_key
                        && !run_text.is_empty()
                    {
                        let x = run_start_col as f32 * char_width;
                        let (r, g, b) = current.fg;
                        let fill = format!("#{:02x}{:02x}{:02x}", r, g, b);
                        let mut el = TextElement::new(run_text).set("x", x).set("fill", fill);
                        if current.bold {
                            el = el.set("font-weight", "bold");
                        }
                        if current.italic {
                            el = el.set("font-style", "italic");
                        }
                        if current.underline {
                            el = el.set("text-decoration", "underline");
                        }
                        text_group = text_group.add(el);
                    }

                    row_group = row_group.add(text_group);
                    frame_group = frame_group.add(row_group);
                }
            }

            // Animate opacity for this frame's time slice; chain to previous frame's end
            let begin_attr = if i == 0 {
                if self.loop_enable {
                    format!("0s;{}.end", last_anim_id)
                } else {
                    "0s".to_string()
                }
            } else {
                format!("f{}.end", i - 1)
            };
            let anim_id = format!("f{}", i);
            let dur = durations.get(i).copied().unwrap_or(0.0).max(0.000_001);
            let anim = Animate::new()
                .set("id", anim_id.clone())
                .set("attributeName", "opacity")
                .set("begin", begin_attr)
                .set("dur", format!("{:.6}s", dur))
                // Keep frame visible for the whole duration, then revert to base (0)
                .set("values", "1;1")
                .set("keyTimes", "0;1")
                .set("calcMode", "discrete");
            frame_group = frame_group.add(anim);
            frame_groups.push(frame_group);
        }

        (css, frame_groups)
    }

    fn create_window_decorations(&self, width: f32, _height: f32) -> Group {
        let mut group = Group::new();

        // Window bar
        let bar = Rectangle::new()
            .set("width", width)
            .set("height", 30)
            .set("fill", "#2d2d2d")
            .set("rx", "5")
            .set("ry", "5");
        group = group.add(bar);

        // Window buttons (close, minimize, maximize)
        let button_y = 15.0;
        let button_radius = 6.0;
        let button_spacing = 20.0;
        let button_start_x = 20.0;

        // Close button (red)
        let close = Circle::new()
            .set("cx", button_start_x)
            .set("cy", button_y)
            .set("r", button_radius)
            .set("fill", "#ff5f57");
        group = group.add(close);

        // Minimize button (yellow)
        let minimize = Circle::new()
            .set("cx", button_start_x + button_spacing)
            .set("cy", button_y)
            .set("r", button_radius)
            .set("fill", "#ffbd2e");
        group = group.add(minimize);

        // Maximize button (green)
        let maximize = Circle::new()
            .set("cx", button_start_x + button_spacing * 2.0)
            .set("cy", button_y)
            .set("r", button_radius)
            .set("fill", "#28ca42");
        group = group.add(maximize);

        // Window title - use Group with text
        let title_group = Group::new()
            .set(
                "transform",
                format!("translate({}, {})", width / 2.0, button_y + 5.0),
            )
            .add(svg::node::Text::new("Terminal"));
        group = group.add(title_group);

        group
    }
}
