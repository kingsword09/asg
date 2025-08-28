use rgb::RGB8;
use vte::{Params, Parser, Perform};

#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub fg: RGB8,
    pub bg: RGB8,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            fg: RGB8::new(204, 204, 204),
            bg: RGB8::new(0, 0, 0),
            bold: false,
            italic: false,
            underline: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    cells: Vec<Vec<Cell>>,
}

// Alias for backwards compatibility
pub type Grid = Frame;

impl Frame {
    pub fn new(width: usize, height: usize) -> Self {
        let cells = vec![vec![Cell::default(); width]; height];
        Frame {
            width,
            height,
            cells,
        }
    }
    
    pub fn get_cell(&self, row: usize, col: usize) -> Option<&Cell> {
        self.cells.get(row).and_then(|r| r.get(col))
    }
    
    pub fn get_cell_mut(&mut self, row: usize, col: usize) -> Option<&mut Cell> {
        self.cells.get_mut(row).and_then(|r| r.get_mut(col))
    }
    
    pub fn clear(&mut self) {
        for row in &mut self.cells {
            for cell in row {
                *cell = Cell::default();
            }
        }
    }
}

#[derive(Debug, Clone)]
struct Cursor {
    row: usize,
    col: usize,
}

impl Cursor {
    pub fn new() -> Self {
        Cursor { row: 0, col: 0 }
    }
}

pub struct Emulator {
    pub grid: Grid,
    cursor: Cursor,
    saved_cursor: Option<Cursor>,
    pub fg_color: RGB8,
    pub bg_color: RGB8,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    parser: Parser,
}

impl Emulator {
    pub fn new(width: usize, height: usize) -> Self {
        Emulator {
            grid: Grid::new(width, height),
            cursor: Cursor::new(),
            saved_cursor: None,
            fg_color: RGB8::new(204, 204, 204),
            bg_color: RGB8::new(0, 0, 0),
            bold: false,
            italic: false,
            underline: false,
            parser: Parser::new(),
        }
    }
    
    pub fn process(&mut self, input: &[u8]) {
        let mut parser = std::mem::take(&mut self.parser);
        for byte in input {
            parser.advance(self, *byte);
        }
        self.parser = parser;
    }
    
    pub fn process_string(&mut self, data: &str) {
        self.process(data.as_bytes());
    }
    
    pub fn get_frame(&self) -> Frame {
        self.grid.clone()
    }
    
    fn write_char(&mut self, ch: char) {
        if let Some(cell) = self.grid.get_cell_mut(self.cursor.row, self.cursor.col) {
            cell.ch = ch;
            cell.fg = self.fg_color;
            cell.bg = self.bg_color;
            cell.bold = self.bold;
            cell.italic = self.italic;
            cell.underline = self.underline;
        }
        
        self.cursor.col += 1;
        if self.cursor.col >= self.grid.width {
            self.cursor.col = 0;
            self.cursor.row += 1;
            if self.cursor.row >= self.grid.height {
                self.cursor.row = self.grid.height - 1;
                // Scroll up
                self.grid.cells.remove(0);
                self.grid.cells.push(vec![Cell::default(); self.grid.width]);
            }
        }
    }
    
    fn parse_sgr_params(&mut self, params: &Params) {
        for param in params {
            for value in param {
                match *value {
                    0 => {
                        // Reset all attributes
                        self.fg_color = RGB8::new(204, 204, 204);
                        self.bg_color = RGB8::new(0, 0, 0);
                        self.bold = false;
                        self.italic = false;
                        self.underline = false;
                    }
                    1 => self.bold = true,
                    3 => self.italic = true,
                    4 => self.underline = true,
                    22 => self.bold = false,
                    23 => self.italic = false,
                    24 => self.underline = false,
                    30..=37 => {
                        self.fg_color = ansi_color_to_rgb(value - 30, self.bold);
                    }
                    40..=47 => {
                        self.bg_color = ansi_color_to_rgb(value - 40, false);
                    }
                    38 => {
                        // Extended foreground color (not fully implemented)
                        // This would need more complex parsing for 256-color and RGB
                    }
                    48 => {
                        // Extended background color (not fully implemented)
                    }
                    90..=97 => {
                        // Bright foreground colors
                        self.fg_color = ansi_color_to_rgb(value - 90, true);
                    }
                    100..=107 => {
                        // Bright background colors
                        self.bg_color = ansi_color_to_rgb(value - 100, true);
                    }
                    _ => {
                        log::debug!("Unhandled SGR parameter: {}", value);
                    }
                }
            }
        }
    }
}

impl Perform for Emulator {
    fn print(&mut self, c: char) {
        self.write_char(c);
    }
    
    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => {
                self.cursor.row += 1;
                if self.cursor.row >= self.grid.height {
                    self.cursor.row = self.grid.height - 1;
                    // Scroll up
                    self.grid.cells.remove(0);
                    self.grid.cells.push(vec![Cell::default(); self.grid.width]);
                }
            }
            b'\r' => {
                self.cursor.col = 0;
            }
            b'\t' => {
                // Move to next tab stop (every 8 columns)
                let next_tab = ((self.cursor.col / 8) + 1) * 8;
                self.cursor.col = next_tab.min(self.grid.width - 1);
            }
            b'\x08' => {
                // Backspace
                if self.cursor.col > 0 {
                    self.cursor.col -= 1;
                }
            }
            _ => {
                log::debug!("Unhandled execute byte: {:02x}", byte);
            }
        }
    }
    
    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _c: char) {
        // Not needed for basic implementation
    }
    
    fn put(&mut self, _byte: u8) {
        // Not needed for basic implementation
    }
    
    fn unhook(&mut self) {
        // Not needed for basic implementation
    }
    
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {
        // OSC sequences (like setting window title) - not critical for SVG generation
    }
    
    fn csi_dispatch(&mut self, params: &Params, _intermediates: &[u8], _ignore: bool, c: char) {
        match c {
            'm' => {
                // SGR - Select Graphic Rendition
                self.parse_sgr_params(params);
            }
            'H' | 'f' => {
                // CUP - Cursor Position
                let row = params.iter().nth(0)
                    .and_then(|p| p.get(0))
                    .map(|&v| v.saturating_sub(1) as usize)
                    .unwrap_or(0);
                let col = params.iter().nth(1)
                    .and_then(|p| p.get(0))
                    .map(|&v| v.saturating_sub(1) as usize)
                    .unwrap_or(0);
                
                self.cursor.row = row.min(self.grid.height - 1);
                self.cursor.col = col.min(self.grid.width - 1);
            }
            'J' => {
                // ED - Erase Display
                let mode = params.iter().nth(0)
                    .and_then(|p| p.get(0))
                    .copied()
                    .unwrap_or(0);
                
                match mode {
                    0 => {
                        // Clear from cursor to end of screen
                        for row in self.cursor.row..self.grid.height {
                            let start_col = if row == self.cursor.row { self.cursor.col } else { 0 };
                            for col in start_col..self.grid.width {
                                if let Some(cell) = self.grid.get_cell_mut(row, col) {
                                    *cell = Cell::default();
                                }
                            }
                        }
                    }
                    1 => {
                        // Clear from beginning to cursor
                        for row in 0..=self.cursor.row {
                            let end_col = if row == self.cursor.row { self.cursor.col } else { self.grid.width - 1 };
                            for col in 0..=end_col {
                                if let Some(cell) = self.grid.get_cell_mut(row, col) {
                                    *cell = Cell::default();
                                }
                            }
                        }
                    }
                    2 => {
                        // Clear entire screen
                        self.grid.clear();
                    }
                    _ => {}
                }
            }
            'K' => {
                // EL - Erase Line
                let mode = params.iter().nth(0)
                    .and_then(|p| p.get(0))
                    .copied()
                    .unwrap_or(0);
                
                match mode {
                    0 => {
                        // Clear from cursor to end of line
                        for col in self.cursor.col..self.grid.width {
                            if let Some(cell) = self.grid.get_cell_mut(self.cursor.row, col) {
                                *cell = Cell::default();
                            }
                        }
                    }
                    1 => {
                        // Clear from beginning to cursor
                        for col in 0..=self.cursor.col {
                            if let Some(cell) = self.grid.get_cell_mut(self.cursor.row, col) {
                                *cell = Cell::default();
                            }
                        }
                    }
                    2 => {
                        // Clear entire line
                        for col in 0..self.grid.width {
                            if let Some(cell) = self.grid.get_cell_mut(self.cursor.row, col) {
                                *cell = Cell::default();
                            }
                        }
                    }
                    _ => {}
                }
            }
            'A' => {
                // CUU - Cursor Up
                let n = params.iter().nth(0)
                    .and_then(|p| p.get(0))
                    .copied()
                    .unwrap_or(1) as usize;
                self.cursor.row = self.cursor.row.saturating_sub(n);
            }
            'B' => {
                // CUD - Cursor Down
                let n = params.iter().nth(0)
                    .and_then(|p| p.get(0))
                    .copied()
                    .unwrap_or(1) as usize;
                self.cursor.row = (self.cursor.row + n).min(self.grid.height - 1);
            }
            'C' => {
                // CUF - Cursor Forward
                let n = params.iter().nth(0)
                    .and_then(|p| p.get(0))
                    .copied()
                    .unwrap_or(1) as usize;
                self.cursor.col = (self.cursor.col + n).min(self.grid.width - 1);
            }
            'D' => {
                // CUB - Cursor Back
                let n = params.iter().nth(0)
                    .and_then(|p| p.get(0))
                    .copied()
                    .unwrap_or(1) as usize;
                self.cursor.col = self.cursor.col.saturating_sub(n);
            }
            's' => {
                // Save cursor position
                self.saved_cursor = Some(Cursor {
                    row: self.cursor.row,
                    col: self.cursor.col,
                });
            }
            'u' => {
                // Restore cursor position
                if let Some(saved) = &self.saved_cursor {
                    self.cursor.row = saved.row;
                    self.cursor.col = saved.col;
                }
            }
            _ => {
                log::debug!("Unhandled CSI sequence: {}", c);
            }
        }
    }
    
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {
        // ESC sequences - not critical for basic implementation
    }
}

fn ansi_color_to_rgb(color: u16, bright: bool) -> RGB8 {
    let colors = if bright {
        // Bright colors
        [
            RGB8::new(127, 127, 127),  // Black (bright = gray)
            RGB8::new(255, 0, 0),      // Red
            RGB8::new(0, 255, 0),      // Green
            RGB8::new(255, 255, 0),    // Yellow
            RGB8::new(92, 92, 255),    // Blue
            RGB8::new(255, 0, 255),    // Magenta
            RGB8::new(0, 255, 255),    // Cyan
            RGB8::new(255, 255, 255),  // White
        ]
    } else {
        // Normal colors
        [
            RGB8::new(0, 0, 0),        // Black
            RGB8::new(205, 0, 0),      // Red
            RGB8::new(0, 205, 0),      // Green
            RGB8::new(205, 205, 0),    // Yellow
            RGB8::new(0, 0, 238),      // Blue
            RGB8::new(205, 0, 205),    // Magenta
            RGB8::new(0, 205, 205),    // Cyan
            RGB8::new(229, 229, 229),  // White
        ]
    };
    
    colors[color as usize % 8]
}

#[derive(Debug, Clone)]
pub struct StateSnapshot {
    pub time: f64,
    pub grid: Grid,
}

pub fn process_events(events: &[crate::asciicast::Event], width: usize, height: usize) -> Vec<StateSnapshot> {
    let mut emulator = Emulator::new(width, height);
    let mut snapshots = Vec::new();
    
    for event in events {
        if event.event_type == crate::asciicast::EventType::Output {
            emulator.process_string(&event.data);
            snapshots.push(StateSnapshot {
                time: event.time,
                grid: emulator.grid.clone(),
            });
        }
    }
    
    snapshots
}
