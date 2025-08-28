pub mod asciicast;
pub mod input;
pub mod renderer;
pub mod terminal;
pub mod theme;

use anyhow::Result;
use clap::ValueEnum;

#[derive(Debug, Clone, ValueEnum)]
pub enum Theme {
    #[clap(name = "asciinema")]
    Asciinema,
    #[clap(name = "dracula")]
    Dracula,
    #[clap(name = "github-dark")]
    GithubDark,
    #[clap(name = "github-light")]
    GithubLight,
    #[clap(name = "monokai")]
    Monokai,
    #[clap(name = "solarized-dark")]
    SolarizedDark,
    #[clap(name = "solarized-light")]
    SolarizedLight,
    
    #[clap(skip)]
    Custom(String),
}

impl TryFrom<Theme> for theme::Theme {
    type Error = anyhow::Error;
    
    fn try_from(theme: Theme) -> Result<Self, Self::Error> {
        use Theme::*;
        
        match theme {
            Asciinema => theme::Theme::from_str("121314,cccccc,000000,dd3c69,4ebf22,ddaf3c,26b0d7,b954e1,54e1b9,d9d9d9,4d4d4d,dd3c69,4ebf22,ddaf3c,26b0d7,b954e1,54e1b9,ffffff"),
            Dracula => theme::Theme::from_str("282a36,f8f8f2,21222c,ff5555,50fa7b,f1fa8c,bd93f9,ff79c6,8be9fd,f8f8f2,6272a4,ff6e6e,69ff94,ffffa5,d6acff,ff92df,a4ffff,ffffff"),
            GithubDark => theme::Theme::from_str("171b21,eceff4,0e1116,f97583,a2fca2,fabb72,7db4f9,c4a0f5,1f6feb,eceff4,6a737d,bf5a64,7abf7a,bf8f57,608bbf,997dbf,195cbf,b9bbbf"),
            GithubLight => theme::Theme::from_str("f6f8fa,24292f,ffffff,cf222e,1a7f37,9a6700,0969da,8250df,1f6feb,24292f,57606a,a40e26,2da44e,bf8700,1f6feb,a475f9,1f6feb,8c959f"),
            Monokai => theme::Theme::from_str("272822,f8f8f2,272822,f92672,a6e22e,f4bf75,66d9ef,ae81ff,a1efe4,f8f8f2,75715e,f92672,a6e22e,f4bf75,66d9ef,ae81ff,a1efe4,f9f8f5"),
            SolarizedDark => theme::Theme::from_str("002b36,839496,073642,dc322f,859900,b58900,268bd2,6c71c4,2aa198,93a1a1,586e75,dc322f,859900,b58900,268bd2,6c71c4,2aa198,fdf6e3"),
            SolarizedLight => theme::Theme::from_str("fdf6e3,657b83,eee8d5,dc322f,859900,b58900,268bd2,6c71c4,2aa198,586e75,93a1a1,dc322f,859900,b58900,268bd2,6c71c4,2aa198,002b36"),
            Custom(colors) => theme::Theme::from_str(&colors),
        }
    }
}

pub struct Config {
    pub theme: Theme,
    pub speed: f64,
    pub fps: u8,
    pub font_size: u8,
    pub font_family: String,
    pub line_height: f32,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
    pub idle_time_limit: Option<f64>,
    pub loop_enable: bool,
    pub at: Option<f64>,
    pub from: Option<f64>,
    pub to: Option<f64>,
    pub no_cursor: bool,
    pub window: bool,
    pub padding: u16,
    pub padding_x: Option<u16>,
    pub padding_y: Option<u16>,
}

impl Config {
    pub fn effective_padding_x(&self) -> u16 {
        self.padding_x.unwrap_or(self.padding)
    }
    
    pub fn effective_padding_y(&self) -> u16 {
        self.padding_y.unwrap_or(self.padding)
    }
}
