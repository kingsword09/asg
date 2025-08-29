use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub enum InputSource {
    File(String),
    RemoteId(String),
}

/// Returns a BufRead for the given input string.
/// - If it looks like a path and exists: reads from file.
/// - If it looks like a URL (contains "://"): fetches via HTTP.
/// - Otherwise: treats it as an asciinema remote id and fetches from default server.
pub fn get_reader(input: &str) -> Result<Box<dyn BufRead>> {
    if Path::new(input).exists() {
        return FileInput::new(input.to_string()).read();
    }

    if input.contains("://") {
        // Fetch arbitrary URL
        let url = input.to_string();
        log::info!("Fetching cast file from: {}", url);
        let response = reqwest::blocking::get(&url)
            .with_context(|| format!("Failed to fetch from URL: {}", url))?;
        if !response.status().is_success() {
            anyhow::bail!("Server returned status: {}", response.status());
        }
        let content = response
            .bytes()
            .with_context(|| "Failed to read response body")?;
        return Ok(Box::new(BufReader::new(std::io::Cursor::new(content))));
    }

    // Treat as remote id
    RemoteInput::new(input.to_string(), None).read()
}

pub trait InputReader {
    fn read(&self) -> Result<Box<dyn BufRead>>;
}

pub struct FileInput {
    path: String,
}

impl FileInput {
    pub fn new(path: String) -> Self {
        Self { path }
    }
}

impl InputReader for FileInput {
    fn read(&self) -> Result<Box<dyn BufRead>> {
        let file = File::open(&self.path)
            .with_context(|| format!("Failed to open file: {}", self.path))?;
        Ok(Box::new(BufReader::new(file)))
    }
}

pub struct RemoteInput {
    id: String,
    server: String,
}

impl RemoteInput {
    pub fn new(id: String, server: Option<String>) -> Self {
        Self {
            id,
            server: server.unwrap_or_else(|| "https://asciinema.org".to_string()),
        }
    }
}

impl InputReader for RemoteInput {
    fn read(&self) -> Result<Box<dyn BufRead>> {
        let url = format!("{}/a/{}.cast", self.server, self.id);
        log::info!("Fetching cast file from: {}", url);

        let response = reqwest::blocking::get(&url)
            .with_context(|| format!("Failed to fetch from URL: {}", url))?;

        if !response.status().is_success() {
            anyhow::bail!("Server returned status: {}", response.status());
        }

        let content = response
            .bytes()
            .with_context(|| "Failed to read response body")?;

        Ok(Box::new(BufReader::new(std::io::Cursor::new(content))))
    }
}

pub fn get_input_reader(source: &InputSource) -> Result<Box<dyn InputReader>> {
    match source {
        InputSource::File(path) => {
            if !Path::new(path).exists() {
                anyhow::bail!("File does not exist: {}", path);
            }
            Ok(Box::new(FileInput::new(path.clone())))
        }
        InputSource::RemoteId(id) => Ok(Box::new(RemoteInput::new(id.clone(), None))),
    }
}
