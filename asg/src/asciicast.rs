use std::collections::HashMap;
use std::io::BufRead;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

use crate::theme::{Rgb, Theme};

#[derive(Debug, Clone)]
pub struct Header {
    pub term: Terminal,
    pub timestamp: Option<u64>,
    pub idle_time_limit: Option<f64>,
    pub command: Option<String>,
    pub title: Option<String>,
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Terminal {
    pub cols: u16,
    pub rows: u16,
    pub kind: Option<String>,
    pub version: Option<String>,
    pub theme: Option<Theme>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// Absolute time accumulated from v3's relative event intervals.
    pub time: Duration,
    pub kind: EventKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventKind {
    Output(String),
    Input(String),
    Resize { cols: u16, rows: u16 },
    Marker(String),
    Exit(i32),
    Other { code: char, data: String },
}

#[derive(Debug, Clone)]
pub struct Asciicast {
    pub header: Header,
    pub events: Vec<Event>,
}

#[derive(Deserialize)]
struct RawHeader {
    version: u8,
    term: RawTerminal,
    timestamp: Option<u64>,
    idle_time_limit: Option<f64>,
    command: Option<String>,
    title: Option<String>,
    env: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
struct VersionHeader {
    version: u8,
}

#[derive(Deserialize)]
struct RawTerminal {
    cols: u16,
    rows: u16,
    #[serde(rename = "type")]
    kind: Option<String>,
    version: Option<String>,
    theme: Option<RawTheme>,
}

#[derive(Deserialize)]
struct RawTheme {
    fg: String,
    bg: String,
    palette: String,
}

#[derive(Deserialize)]
struct RawEvent(f64, String, String);

impl Asciicast {
    /// Parse an asciicast v3 stream. v1/v2 are intentionally rejected: their
    /// timestamp semantics differ and silently accepting them would corrupt the
    /// animation timeline.
    pub fn parse<R: BufRead>(reader: R) -> Result<Self> {
        let mut lines = reader.lines().enumerate();
        let (_, first_line) = lines.next().ok_or_else(|| anyhow!("empty cast file"))?;
        let first_line = first_line.context("failed to read asciicast header")?;
        let first_line = first_line.strip_prefix('\u{feff}').unwrap_or(&first_line);

        let version: VersionHeader =
            serde_json::from_str(first_line).context("invalid asciicast header on line 1")?;
        if version.version != 3 {
            bail!(
                "unsupported asciicast version {}; only version 3 is supported",
                version.version
            );
        }
        let raw: RawHeader =
            serde_json::from_str(first_line).context("invalid asciicast v3 header on line 1")?;
        let header = Header::try_from(raw)?;

        let mut events = Vec::new();
        let mut time = Duration::ZERO;

        for (index, line) in lines {
            let line_number = index + 1;
            let line = line.with_context(|| format!("failed to read line {line_number}"))?;
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let raw: RawEvent = serde_json::from_str(&line)
                .with_context(|| format!("invalid asciicast v3 event on line {line_number}"))?;
            let delta = duration(raw.0)
                .with_context(|| format!("invalid event time on line {line_number}"))?;
            time = time
                .checked_add(delta)
                .ok_or_else(|| anyhow!("event timeline overflow on line {line_number}"))?;
            let kind = parse_event_kind(&raw.1, raw.2)
                .with_context(|| format!("invalid event on line {line_number}"))?;

            events.push(Event { time, kind });
        }

        Ok(Self { header, events })
    }
}

impl TryFrom<RawHeader> for Header {
    type Error = anyhow::Error;

    fn try_from(raw: RawHeader) -> Result<Self> {
        if raw.version != 3 {
            bail!(
                "unsupported asciicast version {}; only version 3 is supported",
                raw.version
            );
        }
        if raw.term.cols == 0 || raw.term.rows == 0 {
            bail!("terminal dimensions must be greater than zero");
        }
        if let Some(limit) = raw.idle_time_limit
            && (!limit.is_finite() || limit < 0.0)
        {
            bail!("idle_time_limit must be a finite, non-negative number");
        }

        let theme = raw
            .term
            .theme
            .map(Theme::try_from)
            .transpose()
            .context("invalid v3 terminal theme")?;

        Ok(Self {
            term: Terminal {
                cols: raw.term.cols,
                rows: raw.term.rows,
                kind: raw.term.kind,
                version: raw.term.version,
                theme,
            },
            timestamp: raw.timestamp,
            idle_time_limit: raw.idle_time_limit,
            command: raw.command,
            title: raw.title,
            env: raw.env.unwrap_or_default(),
        })
    }
}

impl TryFrom<RawTheme> for Theme {
    type Error = anyhow::Error;

    fn try_from(raw: RawTheme) -> Result<Self> {
        let foreground = Rgb::parse(&raw.fg).context("invalid foreground color")?;
        let background = Rgb::parse(&raw.bg).context("invalid background color")?;
        let palette = raw
            .palette
            .split(':')
            .map(Rgb::parse)
            .collect::<Result<Vec<_>>>()
            .context("invalid palette color")?;

        Theme::from_v3(foreground, background, palette)
    }
}

fn duration(value: f64) -> Result<Duration> {
    if !value.is_finite() || value < 0.0 {
        bail!("event interval must be a finite, non-negative number");
    }

    Duration::try_from_secs_f64(value).context("event interval is out of range")
}

fn parse_event_kind(code: &str, data: String) -> Result<EventKind> {
    let kind = match code {
        "o" => EventKind::Output(data),
        "i" => EventKind::Input(data),
        "m" => EventKind::Marker(data),
        "x" => EventKind::Exit(data.parse().context("exit status must be an integer")?),
        "r" => {
            let (cols, rows) = data
                .split_once('x')
                .ok_or_else(|| anyhow!("resize data must have the form COLSxROWS"))?;
            let cols: u16 = cols.parse().context("invalid resize column count")?;
            let rows: u16 = rows.parse().context("invalid resize row count")?;
            if cols == 0 || rows == 0 {
                bail!("resize dimensions must be greater than zero");
            }
            EventKind::Resize { cols, rows }
        }
        "" => bail!("event code cannot be empty"),
        other => EventKind::Other {
            code: other.chars().next().expect("non-empty event code"),
            data,
        },
    };

    Ok(kind)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn parse(input: &str) -> Result<Asciicast> {
        Asciicast::parse(Cursor::new(input))
    }

    #[test]
    fn parses_v3_header_and_accumulates_relative_times() {
        let cast = parse(
            r##"{"version":3,"term":{"cols":80,"rows":24,"type":"xterm-256color","theme":{"fg":"#ffffff","bg":"#000000","palette":"#000000:#111111:#222222:#333333:#444444:#555555:#666666:#777777"}},"idle_time_limit":2.5}
[0.5,"o","a"]
[1.25,"i","b"]
[0.25,"r","100x30"]
[0,"m","chapter"]
[0,"x","7"]
"##,
        )
        .unwrap();

        assert_eq!((cast.header.term.cols, cast.header.term.rows), (80, 24));
        assert_eq!(cast.header.term.kind.as_deref(), Some("xterm-256color"));
        assert_eq!(cast.header.idle_time_limit, Some(2.5));
        assert_eq!(cast.events[0].time.as_secs_f64(), 0.5);
        assert_eq!(cast.events[1].time.as_secs_f64(), 1.75);
        assert_eq!(cast.events[2].time.as_secs_f64(), 2.0);
        assert_eq!(
            cast.events[2].kind,
            EventKind::Resize {
                cols: 100,
                rows: 30
            }
        );
        assert_eq!(cast.events[4].kind, EventKind::Exit(7));
        assert_eq!(
            cast.header.term.theme.unwrap().palette[0],
            Rgb::new(0, 0, 0)
        );
    }

    #[test]
    fn ignores_v3_comment_and_empty_lines() {
        let cast =
            parse("{\"version\":3,\"term\":{\"cols\":2,\"rows\":1}}\n# note\n\n[0,\"o\",\"x\"]\n")
                .unwrap();

        assert_eq!(cast.events.len(), 1);
    }

    #[test]
    fn rejects_v2_instead_of_misreading_absolute_timestamps() {
        let error = parse("{\"version\":2,\"width\":80,\"height\":24}\n").unwrap_err();
        assert!(error.to_string().contains("only version 3"));
    }

    #[test]
    fn accepts_null_optional_environment_metadata() {
        let cast =
            parse("{\"version\":3,\"term\":{\"cols\":2,\"rows\":1},\"env\":null}\n").unwrap();

        assert!(cast.header.env.is_empty());
    }

    #[test]
    fn reports_the_bad_event_line() {
        let error = parse(
            "{\"version\":3,\"term\":{\"cols\":2,\"rows\":1}}\n[0,\"o\",\"x\"]\n[-1,\"o\",\"y\"]\n",
        )
        .unwrap_err();

        assert!(format!("{error:#}").contains("line 3"));
    }

    #[test]
    fn preserves_unknown_v3_events() {
        let cast = parse("{\"version\":3,\"term\":{\"cols\":2,\"rows\":1}}\n[0,\"z\",\"data\"]\n")
            .unwrap();

        assert_eq!(
            cast.events[0].kind,
            EventKind::Other {
                code: 'z',
                data: "data".to_owned()
            }
        );
    }
}
