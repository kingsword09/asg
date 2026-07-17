use std::fs::File;
use std::io::{self, BufRead, BufReader, Cursor, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

const ZSTD_MAGIC: &[u8; 4] = b"\x28\xb5\x2f\xfd";

/// Open a local path, URL, asciinema.org recording ID, or `-` for stdin.
/// Plain and zstd-compressed v3 streams are detected from their magic bytes.
pub fn open(input: &str) -> Result<Box<dyn BufRead>> {
    let reader: Box<dyn Read> = if input == "-" {
        Box::new(io::stdin())
    } else if input.starts_with("http://") || input.starts_with("https://") {
        fetch_url(input)?
    } else if Path::new(input).exists() || looks_like_path(input) {
        Box::new(File::open(input).with_context(|| format!("cannot open input file {input:?}"))?)
    } else {
        let url = format!("https://asciinema.org/a/{input}.cast?dl=true");
        log::info!("fetching recording from {url}");
        fetch_url(&url)?
    };

    decode(reader)
}

pub fn write(output: &str, svg: &str) -> Result<Option<PathBuf>> {
    if output == "-" {
        let mut stdout = io::stdout().lock();
        stdout
            .write_all(svg.as_bytes())
            .context("failed to write SVG to stdout")?;
        stdout.flush().context("failed to flush stdout")?;
        return Ok(None);
    }

    let path = PathBuf::from(output);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create output directory {}", parent.display()))?;
    }
    let mut file = File::create(&path)
        .with_context(|| format!("cannot create output file {}", path.display()))?;
    file.write_all(svg.as_bytes())
        .with_context(|| format!("cannot write output file {}", path.display()))?;

    Ok(Some(path))
}

fn looks_like_path(input: &str) -> bool {
    input.ends_with(".cast")
        || input.ends_with(".cast.zst")
        || input.contains('/')
        || input.contains('\\')
        || input.starts_with('.')
}

fn decode(mut reader: Box<dyn Read>) -> Result<Box<dyn BufRead>> {
    let mut prefix = Vec::with_capacity(ZSTD_MAGIC.len());
    (&mut reader)
        .take(ZSTD_MAGIC.len() as u64)
        .read_to_end(&mut prefix)
        .context("failed to inspect input stream")?;
    let compressed = prefix.as_slice() == ZSTD_MAGIC;
    let replay = Cursor::new(prefix).chain(reader);

    if compressed {
        let decoder = ruzstd::StreamingDecoder::new(replay)
            .map_err(|error| anyhow::anyhow!("invalid zstd-compressed cast: {error}"))?;
        Ok(Box::new(BufReader::new(decoder)))
    } else {
        Ok(Box::new(BufReader::new(replay)))
    }
}

#[cfg(not(target_family = "wasm"))]
fn fetch_url(url: &str) -> Result<Box<dyn Read>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(concat!("asg/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("failed to create HTTP client")?;
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("failed to fetch {url}"))?
        .error_for_status()
        .with_context(|| format!("server rejected {url}"))?;

    Ok(Box::new(response))
}

#[cfg(target_os = "wasi")]
fn fetch_url(url: &str) -> Result<Box<dyn Read>> {
    let response = waki::Client::new()
        .get(url)
        .send()
        .with_context(|| format!("failed to fetch {url}"))?;
    let status = response.status_code();
    if !(200..300).contains(&status) {
        anyhow::bail!("server returned HTTP {status} for {url}");
    }
    let body = response
        .body()
        .with_context(|| format!("failed to read response body from {url}"))?;

    Ok(Box::new(Cursor::new(body)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_plain_local_input_without_losing_the_prefix() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("demo.cast");
        std::fs::write(&path, b"123456").unwrap();
        let mut reader = open(path.to_str().unwrap()).unwrap();
        let mut value = String::new();
        reader.read_to_string(&mut value).unwrap();

        assert_eq!(value, "123456");
    }

    #[test]
    fn creates_output_parent_directories() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested/demo.svg");
        let written = write(path.to_str().unwrap(), "<svg/>").unwrap().unwrap();

        assert_eq!(written, path);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "<svg/>");
    }

    #[test]
    fn missing_cast_path_is_not_misread_as_a_remote_id() {
        let error = open("definitely-missing.cast").err().unwrap();
        assert!(error.to_string().contains("input file"));
    }
}
