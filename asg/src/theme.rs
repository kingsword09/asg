use std::str::FromStr;

use anyhow::{Result, anyhow};
use rgb::RGB8;
use serde::{Deserialize, Serialize, de::Visitor};

#[derive(Debug, Clone, Copy)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub fn to_u32(&self) -> u32 {
        u32::from_be_bytes([self.r, self.g, self.b, 0])
    }

    pub fn from_u32(rgb: u32) -> Self {
        let [r, g, b, _] = rgb.to_be_bytes();
        Self { r, g, b }
    }
}

impl Serialize for Rgb {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u32(self.to_u32())
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct V;

        impl<'de> Visitor<'de> for V {
            type Value = Rgb;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an integer represeting an RGB color")
            }

            fn visit_u32<E>(self, v: u32) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Rgb::from_u32(v))
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_u32(
                    v.try_into()
                        .map_err(|_| E::custom("rgb value doesn't fit u32"))?,
                )
            }
        }

        deserializer.deserialize_u32(V)
    }
}

impl From<RGB8> for Rgb {
    fn from(color: RGB8) -> Self {
        Rgb {
            r: color.r,
            g: color.g,
            b: color.b,
        }
    }
}

impl From<Rgb> for RGB8 {
    fn from(color: Rgb) -> Self {
        RGB8 {
            r: color.r,
            g: color.g,
            b: color.b,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub bg: Rgb,
    pub fg: Rgb,
    pub palette: [Rgb; 16],
}

impl FromStr for Theme {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
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

        Ok(Theme {
            bg: bg.into(),
            fg: fg.into(),
            palette: palette.map(|c| c.into()),
        })
    }
}

impl Theme {
    pub fn get_color(&self, index: u8) -> RGB8 {
        if index < 16 {
            self.palette[index as usize].into()
        } else {
            self.fg.into()
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
