use avt::{Line, Vt};

/// Immutable terminal state attached to a visual frame.
#[derive(Clone, PartialEq)]
pub struct Snapshot {
    pub cols: usize,
    pub rows: usize,
    pub lines: Vec<Line>,
    pub cursor: Option<(usize, usize)>,
}

impl Snapshot {
    pub fn same_visual(&self, other: &Self) -> bool {
        self == other
    }
}

/// Thin boundary around asciinema's virtual terminal. ANSI/DEC parsing lives in
/// `avt`; this crate owns only recording-specific timeline and SVG concerns.
pub struct Terminal {
    vt: Vt,
}

impl Terminal {
    pub fn new(cols: usize, rows: usize) -> Self {
        let vt = Vt::builder().size(cols, rows).scrollback_limit(0).build();
        Self { vt }
    }

    pub fn feed(&mut self, data: &str) {
        self.vt.feed_str(data);
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.vt.resize(cols, rows);
    }

    pub fn snapshot(&self, cursor: bool) -> Snapshot {
        let (cols, rows) = self.vt.size();
        Snapshot {
            cols,
            rows,
            lines: self.vt.view().cloned().collect(),
            cursor: cursor.then(|| self.vt.cursor().into()).flatten(),
        }
    }
}

#[cfg(test)]
mod tests {
    use avt::Color;

    use super::*;

    #[test]
    fn delegates_ansi_styles_and_wide_cells_to_avt() {
        let mut terminal = Terminal::new(8, 2);
        terminal.feed("\x1b[1;3;4;9;31;44mX界");
        let snapshot = terminal.snapshot(true);
        let cells = snapshot.lines[0].cells();

        assert_eq!(cells[0].char(), 'X');
        assert_eq!(cells[0].pen().foreground(), Some(Color::Indexed(1)));
        assert_eq!(cells[0].pen().background(), Some(Color::Indexed(4)));
        assert!(cells[0].pen().is_bold());
        assert!(cells[0].pen().is_italic());
        assert!(cells[0].pen().is_underline());
        assert!(cells[0].pen().is_strikethrough());
        assert_eq!(cells[1].char(), '界');
        assert_eq!(cells[1].width(), 2);
        assert_eq!(cells[2].width(), 0);
    }

    #[test]
    fn handles_resize_and_hidden_cursor() {
        let mut terminal = Terminal::new(4, 2);
        terminal.feed("ab");
        terminal.resize(6, 3);
        let snapshot = terminal.snapshot(false);

        assert_eq!((snapshot.cols, snapshot.rows), (6, 3));
        assert_eq!(snapshot.lines.len(), 3);
        assert_eq!(snapshot.cursor, None);
    }
}
