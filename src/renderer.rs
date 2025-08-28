use crate::terminal::Frame;
use crate::theme::Theme;
use anyhow::Result;
use std::collections::HashMap;
use svg::node::element::{Circle, Definitions, Group, Rectangle, Style};
use svg::Document;

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
    
    pub fn render(&self, frames: &[Frame], _total_duration: f64) -> Result<Document> {
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
        let frame_duration = 0.1; // Default frame duration
        let (styles, text_elements) = self.generate_styles_and_segments(frames, frame_duration);
        
        // Create definitions with styles
        let defs = Definitions::new()
            .add(Style::new(styles));
        doc = doc.add(defs);
        
        // Add background
        let bg = Rectangle::new()
            .set("width", "100%")
            .set("height", "100%")
            .set("fill", format!("#{:02x}{:02x}{:02x}", self.theme.bg.r, self.theme.bg.g, self.theme.bg.b));
        doc = doc.add(bg);
        
        // Add window decorations if enabled
        if self.window {
            let window_group = self.create_window_decorations(width, window_bar_height);
            doc = doc.add(window_group);
        }
        
        // Create frames group with proper offset
        let y_offset = self.padding_y as f32 + window_bar_height;
        let mut frames_group = Group::new()
            .set("transform", format!("translate({}, {})", self.padding_x, y_offset));
        
        // Add text elements
        for element in text_elements {
            frames_group = frames_group.add(element);
        }
        
        doc = doc.add(frames_group);
        
        Ok(doc)
    }
    
    fn generate_styles_and_segments(&self, frames: &[Frame], frame_duration: f64) -> (String, Vec<Group>) {
        let mut css = String::new();
        let mut text_elements = Vec::new();
        let style_map: HashMap<String, String> = HashMap::new();
        let style_counter = 0;
        
        // Basic styles
        css.push_str(&format!(
            r#"
text {{ 
    white-space: pre; 
    font-family: monospace; 
    font-size: {}px;
    fill: #{:02x}{:02x}{:02x};
}}
"#,
            self.font_size, self.theme.fg.r, self.theme.fg.g, self.theme.fg.b
        ));
        
        // Unused variables for now - we'll use simple rendering
        let _ = style_map;
        let _ = style_counter;
        let _ = frame_duration;
        
        // Calculate dimensions for positioning
        let _char_width = self.font_size as f32 * 0.6;
        let line_height_px = self.font_size as f32 * self.line_height;
        
        // Simple static rendering for now
        if frames.is_empty() {
            return (css, text_elements);
        }
        
        let frame = &frames[frames.len() / 2]; // Use middle frame for static rendering
        
        for row in 0..frame.height {
            let mut line_content = String::new();
            for col in 0..frame.width {
                if let Some(cell) = frame.get_cell(row, col) {
                    line_content.push(cell.ch);
                } else {
                    line_content.push(' ');
                }
            }
            
            // Trim trailing whitespace
            let trimmed = line_content.trim_end();
            if !trimmed.is_empty() {
                // Create text element group with positioned text node
                let text_group = Group::new()
                    .set("transform", format!("translate(0, {})", row as f32 * line_height_px + self.font_size as f32))
                    .add(svg::node::Text::new(trimmed));
                text_elements.push(text_group);
            }
            
        }
        
        (css, text_elements)
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
            .set("transform", format!("translate({}, {})", width / 2.0, button_y + 5.0))
            .add(svg::node::Text::new("Terminal"));
        group = group.add(title_group);
        
        group
    }
}
