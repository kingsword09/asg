use std::fmt;
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use avt::Color;

/// A compact, serializable RGB color used by the cast parser and SVG encoder.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim().strip_prefix('#').unwrap_or(value.trim());
        if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("expected a 6-digit hex color, got {value:?}");
        }

        Ok(Self {
            r: u8::from_str_radix(&value[0..2], 16)?,
            g: u8::from_str_radix(&value[2..4], 16)?,
            b: u8::from_str_radix(&value[4..6], 16)?,
        })
    }

    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// Terminal colors after applying a named, custom, or cast-provided theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub background: Rgb,
    pub foreground: Rgb,
    pub palette: [Rgb; 16],
    pub bold: Rgb,
    pub cursor: Rgb,
}

impl Theme {
    /// Build a theme from asciicast v3's foreground, background and 8/16-color
    /// palette. Eight-color palettes are repeated, matching asciinema itself.
    pub fn from_v3(foreground: Rgb, background: Rgb, mut colors: Vec<Rgb>) -> Result<Self> {
        if colors.len() == 8 {
            colors.extend_from_within(..);
        }
        if colors.len() != 16 {
            bail!(
                "v3 terminal palette must contain 8 or 16 colors, got {}",
                colors.len()
            );
        }

        let palette: [Rgb; 16] = colors
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid v3 terminal palette"))?;

        Ok(Self {
            background,
            foreground,
            bold: foreground,
            cursor: palette[8],
            palette,
        })
    }

    pub fn named(name: &str) -> Result<Self> {
        let value = match name {
            "svg-term" | "atom-one" => return Ok(Self::default()),
            "asciinema" => {
                "121314,cccccc,000000,dd3c69,4ebf22,ddaf3c,26b0d7,b954e1,54e1b9,d9d9d9,4d4d4d,dd3c69,4ebf22,ddaf3c,26b0d7,b954e1,54e1b9,ffffff"
            }
            "dracula" => {
                "282a36,f8f8f2,21222c,ff5555,50fa7b,f1fa8c,bd93f9,ff79c6,8be9fd,f8f8f2,6272a4,ff6e6e,69ff94,ffffa5,d6acff,ff92df,a4ffff,ffffff"
            }
            "github-dark" => {
                "171b21,eceff4,0e1116,f97583,a2fca2,fabb72,7db4f9,c4a0f5,1f6feb,eceff4,6a737d,bf5a64,7abf7a,bf8f57,608bbf,997dbf,195cbf,b9bbbf"
            }
            "github-light" => {
                "f6f8fa,24292f,ffffff,cf222e,1a7f37,9a6700,0969da,8250df,1f6feb,24292f,57606a,a40e26,2da44e,bf8700,1f6feb,a475f9,1f6feb,8c959f"
            }
            "monokai" => {
                "272822,f8f8f2,272822,f92672,a6e22e,f4bf75,66d9ef,ae81ff,a1efe4,f8f8f2,75715e,f92672,a6e22e,f4bf75,66d9ef,ae81ff,a1efe4,f9f8f5"
            }
            "solarized-dark" => {
                "002b36,839496,073642,dc322f,859900,b58900,268bd2,6c71c4,2aa198,93a1a1,586e75,dc322f,859900,b58900,268bd2,6c71c4,2aa198,fdf6e3"
            }
            "solarized-light" => {
                "fdf6e3,657b83,eee8d5,dc322f,859900,b58900,268bd2,6c71c4,2aa198,586e75,93a1a1,dc322f,859900,b58900,268bd2,6c71c4,2aa198,002b36"
            }
            _ => bail!("unknown theme {name:?}"),
        };

        value.parse()
    }

    pub fn resolve(&self, color: Option<Color>, fallback: Rgb) -> Rgb {
        match color {
            None => fallback,
            Some(Color::RGB(color)) => Rgb::new(color.r, color.g, color.b),
            Some(Color::Indexed(index @ 0..=15)) => self.palette[index as usize],
            Some(Color::Indexed(index @ 16..=231)) => {
                let index = index - 16;
                let level = |component: u8| match component {
                    0 => 0,
                    value => 55 + value * 40,
                };

                Rgb::new(level(index / 36), level((index % 36) / 6), level(index % 6))
            }
            Some(Color::Indexed(index)) => {
                let level = 8 + (index - 232) * 10;
                Rgb::new(level, level, level)
            }
        }
    }
}

impl FromStr for Theme {
    type Err = anyhow::Error;

    /// Parse the svg-term compatible `bg,fg,16 palette colors` form.
    fn from_str(value: &str) -> Result<Self> {
        let colors = value
            .split(',')
            .map(Rgb::parse)
            .collect::<Result<Vec<_>>>()
            .context("invalid custom theme")?;

        if colors.len() != 18 {
            bail!(
                "custom theme must contain background, foreground and 16 palette colors (18 total), got {}",
                colors.len()
            );
        }

        let palette: [Rgb; 16] = colors[2..]
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid custom theme palette"))?;

        Ok(Self {
            background: colors[0],
            foreground: colors[1],
            bold: colors[1],
            cursor: palette[8],
            palette,
        })
    }
}

impl Default for Theme {
    fn default() -> Self {
        // svg-term's Atom One defaults. Keeping these exact makes unthemed
        // recordings visually comparable with svg-term-cli output.
        let palette = [
            Rgb::new(40, 45, 53),
            Rgb::new(232, 131, 136),
            Rgb::new(168, 204, 140),
            Rgb::new(219, 171, 121),
            Rgb::new(113, 190, 242),
            Rgb::new(210, 144, 228),
            Rgb::new(102, 194, 205),
            Rgb::new(185, 191, 202),
            Rgb::new(111, 119, 131),
            Rgb::new(232, 131, 136),
            Rgb::new(168, 204, 140),
            Rgb::new(219, 171, 121),
            Rgb::new(115, 190, 243),
            Rgb::new(210, 144, 227),
            Rgb::new(102, 194, 205),
            Rgb::new(255, 255, 255),
        ];

        Self {
            background: Rgb::new(40, 45, 53),
            foreground: Rgb::new(185, 192, 203),
            bold: Rgb::new(185, 192, 203),
            cursor: Rgb::new(111, 118, 131),
            palette,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_an_eight_color_v3_palette() {
        let colors = (0..8).map(|v| Rgb::new(v, v, v)).collect();
        let theme = Theme::from_v3(Rgb::new(1, 2, 3), Rgb::new(4, 5, 6), colors).unwrap();

        assert_eq!(theme.palette[0], theme.palette[8]);
        assert_eq!(theme.palette[7], theme.palette[15]);
    }

    #[test]
    fn resolves_the_xterm_color_cube_and_grayscale() {
        let theme = Theme::default();

        assert_eq!(
            theme.resolve(Some(Color::Indexed(16)), theme.foreground),
            Rgb::new(0, 0, 0)
        );
        assert_eq!(
            theme.resolve(Some(Color::Indexed(231)), theme.foreground),
            Rgb::new(255, 255, 255)
        );
        assert_eq!(
            theme.resolve(Some(Color::Indexed(232)), theme.foreground),
            Rgb::new(8, 8, 8)
        );
        assert_eq!(
            theme.resolve(Some(Color::Indexed(255)), theme.foreground),
            Rgb::new(238, 238, 238)
        );
    }

    #[test]
    fn rejects_incomplete_custom_themes() {
        assert!("000000,ffffff".parse::<Theme>().is_err());
    }
}
