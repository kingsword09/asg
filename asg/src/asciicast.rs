use std::collections::HashMap;
use std::io::BufRead;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::theme::Theme;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    pub version: u32,
    pub width: u32,
    pub height: u32,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_time_limit: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<Theme>,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub time: f64,
    pub event_type: EventType,
    pub data: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventType {
    Output,
    Input,
    Resize,
}

impl Event {
    pub fn from_json_array(arr: &[Value]) -> Result<Self> {
        if arr.len() < 3 {
            anyhow::bail!("Invalid event format: expected at least 3 elements");
        }

        let time = arr[0].as_f64().context("Event time must be a number")?;

        let event_type_str = arr[1].as_str().context("Event type must be a string")?;

        let event_type = match event_type_str {
            "o" => EventType::Output,
            "i" => EventType::Input,
            "r" => EventType::Resize,
            _ => {
                log::warn!("Unknown event type: {}, treating as output", event_type_str);
                EventType::Output
            }
        };

        let data = arr[2]
            .as_str()
            .context("Event data must be a string")?
            .to_string();

        Ok(Event {
            time,
            event_type,
            data,
        })
    }
}

pub struct Parser<R: BufRead> {
    reader: R,
    header: Option<Header>,
}

impl<R: BufRead> Parser<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            header: None,
        }
    }

    pub fn parse_header(&mut self) -> Result<Header> {
        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .context("Failed to read header line")?;

        if line.is_empty() {
            anyhow::bail!("Empty cast file");
        }

        let header: Header = serde_json::from_str(&line).context("Failed to parse header JSON")?;

        if header.version != 2 {
            anyhow::bail!(
                "Unsupported asciicast version: {}. Only version 2 is supported.",
                header.version
            );
        }

        self.header = Some(header.clone());
        Ok(header)
    }

    pub fn events(&mut self) -> EventIterator<'_, R> {
        EventIterator {
            reader: &mut self.reader,
        }
    }
}

pub struct EventIterator<'a, R: BufRead> {
    reader: &'a mut R,
}

impl<'a, R: BufRead> Iterator for EventIterator<'a, R> {
    type Item = Result<Event>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => None, // EOF
            Ok(_) => {
                let line = line.trim();
                if line.is_empty() {
                    return self.next(); // Skip empty lines
                }

                match serde_json::from_str::<Vec<Value>>(line) {
                    Ok(arr) => Some(Event::from_json_array(&arr)),
                    Err(e) => Some(Err(anyhow::anyhow!("Failed to parse event JSON: {}", e))),
                }
            }
            Err(e) => Some(Err(anyhow::anyhow!("Failed to read line: {}", e))),
        }
    }
}

pub fn parse<R: BufRead>(reader: R) -> Result<(Header, Vec<Event>)> {
    let mut parser = Parser::new(reader);
    let header = parser.parse_header()?;

    let mut events = Vec::new();
    for event_result in parser.events() {
        match event_result {
            Ok(event) => events.push(event),
            Err(e) => {
                log::warn!("Failed to parse event: {}", e);
                // Continue parsing other events
            }
        }
    }

    Ok((header, events))
}

#[derive(Debug, Clone)]
pub struct Asciicast {
    pub header: Header,
    pub events: Vec<Event>,
}

impl Asciicast {
    pub fn parse<R: BufRead>(reader: R) -> Result<Self> {
        let (header, events) = parse(reader)?;
        Ok(Self { header, events })
    }
}
