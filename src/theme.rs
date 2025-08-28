use anyhow::{anyhow, Result};
use rgb::RGB8;

#[derive(Debug, Clone)]
pub struct Theme {
    pub bg: RGB8,
    pub fg: RGB8,
    pub palette: [RGB8; 16],
}

impl Theme {
    pub fn from_str(s: &str) -> Result<Self> {
        let colors: Vec<&str> = s.split(',').collect();
        
        if colors.len() != 18 {
            return Err(anyhow!(
                "Theme must have exactly 18 colors (bg, fg, and 16 palette colors), got {}",
                colors.len()
            ));
        }
        
        let bg = parse_hex_color(colors[0])?;
        let fg = parse_hex_color(colors[1])?;
        
        let mut palette = [RGB8::default(); 16];
        for (i, color_str) in colors[2..].iter().enumerate() {
            palette[i] = parse_hex_color(color_str)?;
        }
        
        Ok(Theme { bg, fg, palette })
    }
    
    pub fn get_color(&self, index: u8) -> RGB8 {
        if index < 16 {
            self.palette[index as usize]
        } else {
            self.fg
        }
    }
}

fn parse_hex_color(s: &str) -> Result<RGB8> {
    let s = s.trim();
    let s = s.trim_start_matches('#');
    
    if s.len() != 6 {
        return Err(anyhow!("Invalid hex color: {}", s));
    }
    
    let r = u8::from_str_radix(&s[0..2], 16)?;
    let g = u8::from_str_radix(&s[2..4], 16)?;
    let b = u8::from_str_radix(&s[4..6], 16)?;
    
    Ok(RGB8::new(r, g, b))
}

impl Default for Theme {
    fn default() -> Self {
        // Asciinema theme as default
        Self::from_str("121314,cccccc,000000,dd3c69,4ebf22,ddaf3c,26b0d7,b954e1,54e1b9,d9d9d9,4d4d4d,dd3c69,4ebf22,ddaf3c,26b0d7,b954e1,54e1b9,ffffff")
            .unwrap()
    }
}
