use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{Context, Result};

pub enum InputSource {
    File(String),
    RemoteId(String),
}

// Platform-specific HTTP fetch implementation

// Non-WASM targets: use reqwest blocking client
#[cfg(not(target_family = "wasm"))]
fn fetch_url(url: &str) -> Result<Vec<u8>> {
    let response = reqwest::blocking::get(url)
        .with_context(|| format!("Failed to fetch from URL: {}", url))?;
    if !response.status().is_success() {
        anyhow::bail!("Server returned status: {}", response.status());
    }
    let bytes = response
        .bytes()
        .with_context(|| "Failed to read response body")?;
    Ok(bytes.to_vec())
}

// WASI (wasm32-wasip2): use waki high-level Client
#[cfg(target_os = "wasi")]
fn fetch_url(url: &str) -> Result<Vec<u8>> {
    let resp = waki::Client::new()
        .get(url)
        .send()
        .with_context(|| format!("Failed to fetch from URL: {}", url))?;

    let status = resp.status_code();
    if !(200..300).contains(&status) {
        anyhow::bail!("Server returned status: {}", status);
    }

    let body = resp
        .body()
        .with_context(|| "Failed to read response body")?;
    Ok(body)
}

/// Returns a BufRead for the given input string.
/// - If it looks like a URL (contains "http://" or "https://"): fetches via HTTP.
/// - If it ends with .cast and is not a URL: treats as file path (converts to absolute path if needed).
/// - If it looks like a path and exists: reads from file.
/// - Otherwise: treats it as an asciinema remote id and fetches from default server.
pub fn get_reader(input: &str) -> Result<Box<dyn BufRead>> {
    log::info!("get_reader() input arg: {}", input);
    // Check if it's a URL first
    if input.starts_with("http://") || input.starts_with("https://") {
        // Fetch arbitrary URL
        let url = input.to_string();
        log::info!("Fetching cast file from: {}", url);
        let content = fetch_url(&url)?;
        return Ok(Box::new(BufReader::new(std::io::Cursor::new(content))));
    }

    // On WASI, absolute guest paths should be opened verbatim to respect preopen mounts
    #[cfg(target_os = "wasi")]
    {
        if input.starts_with('/') {
            log::info!("WASI: opening guest-absolute path verbatim: {}", input);
            return FileInput::new(input.to_string()).read();
        }
    }

    // Check if it's a .cast file (and not a URL)
    if input.ends_with(".cast") {
        let file_path = resolve_cast_file_path(input)?;
        return FileInput::new(file_path).read();
    }

    // Check if it's an existing file path
    if Path::new(input).exists() {
        return FileInput::new(input.to_string()).read();
    }

    // Treat as remote id
    RemoteInput::new(input.to_string(), None).read()
}

/// Resolves a .cast file path to an absolute path, handling cross-platform compatibility.
/// - If the path is already absolute, normalizes it and returns it.
/// - If the path is relative, converts it to an absolute path based on current working directory.
/// - Handles Windows, macOS, and Linux path formats.
/// - Resolves ".." and "." components in the path.
fn resolve_cast_file_path(input: &str) -> Result<String> {
    resolve_file_path(input)
}

/// Resolves an output file path to an absolute path, handling cross-platform compatibility.
/// - If the path is already absolute, normalizes it and returns it.
/// - If the path is relative, converts it to an absolute path based on current working directory.
/// - Handles Windows, macOS, and Linux path formats.
/// - Resolves ".." and "." components in the path.
/// - Creates parent directories if they don't exist.
pub fn resolve_output_path(output: &str) -> Result<String> {
    let resolved_path = resolve_file_path(output)?;

    // Create parent directories if they don't exist
    let path = Path::new(&resolved_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create parent directories for: {}", resolved_path))?;
    }

    Ok(resolved_path)
}

/// Common file path resolution logic used by both input and output path resolvers.
/// - If the path is already absolute, normalizes it and returns it.
/// - If the path is relative, converts it to an absolute path based on current working directory.
/// - Handles Windows, macOS, and Linux path formats.
/// - Resolves ".." and "." components in the path.
fn resolve_file_path(input: &str) -> Result<String> {
    let path = Path::new(input);

    let absolute_path = if path.is_absolute() {
        // Already absolute, but we still want to normalize it
        path.to_path_buf()
    } else {
        // Convert relative path to absolute path
        let current_dir =
            std::env::current_dir().with_context(|| "Failed to get current working directory")?;
        current_dir.join(path)
    };

    // On WASI, avoid canonicalize() because it may strip mount prefixes like "/work" or "/mN"
    // which breaks preopen-based path resolution. Instead, only do a logical normalization.
    #[cfg(target_os = "wasi")]
    {
        let normalized = normalize_path(&absolute_path);
        let out = normalized.to_string_lossy().to_string();
        log::info!(
            "WASI: resolve_file_path: input='{}' abs='{}' => normalized='{}'",
            input,
            absolute_path.to_string_lossy(),
            out
        );
        return Ok(out);
    }

    // Non-WASI: use canonicalize for robust resolution
    #[cfg(not(target_os = "wasi"))]
    {
        // Try to canonicalize the path to resolve any ".." or "." components
        // and ensure it's a proper absolute path
        match absolute_path.canonicalize() {
            Ok(canonical_path) => Ok(canonical_path.to_string_lossy().to_string()),
            Err(_) => {
                // If canonicalize fails (e.g., file doesn't exist yet),
                // manually normalize the path by resolving ".." and "." components
                let normalized = normalize_path(&absolute_path);
                Ok(normalized.to_string_lossy().to_string())
            }
        }
    }
}

/// Manually normalize a path by resolving ".." and "." components.
/// This is used when canonicalize() fails (e.g., when the file doesn't exist).
fn normalize_path(path: &Path) -> std::path::PathBuf {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            std::path::Component::CurDir => {
                // Skip "." components
            }
            std::path::Component::ParentDir => {
                // Handle ".." by removing the last component if possible
                if !components.is_empty() {
                    components.pop();
                }
            }
            _ => {
                // Normal component, add it
                components.push(component);
            }
        }
    }

    // Rebuild the path from components
    let mut result = std::path::PathBuf::new();
    for component in components {
        result.push(component);
    }

    result
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
        let content = fetch_url(&url)?;
        Ok(Box::new(BufReader::new(std::io::Cursor::new(content))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_resolve_cast_file_path() {
        // Test relative path
        let relative_path = "demo.cast";
        let resolved = resolve_cast_file_path(relative_path).unwrap();
        println!(
            "Relative path '{}' resolved to: '{}'",
            relative_path, resolved
        );
        assert!(Path::new(&resolved).is_absolute());
        assert!(resolved.ends_with("demo.cast"));

        // Test relative path with subdirectory
        let relative_subdir = "examples/demo.cast";
        let resolved_subdir = resolve_cast_file_path(relative_subdir).unwrap();
        println!(
            "Relative subdir path '{}' resolved to: '{}'",
            relative_subdir, resolved_subdir
        );
        assert!(Path::new(&resolved_subdir).is_absolute());
        assert!(resolved_subdir.ends_with("demo.cast"));

        // Test absolute path (should remain unchanged)
        let current_dir = env::current_dir().unwrap();
        let absolute_path = current_dir.join("test.cast");
        let absolute_str = absolute_path.to_string_lossy().to_string();
        let resolved_abs = resolve_cast_file_path(&absolute_str).unwrap();
        println!(
            "Absolute path '{}' resolved to: '{}'",
            absolute_str, resolved_abs
        );
        assert_eq!(resolved_abs, absolute_str);

        // Test Windows-style path (if on Windows)
        #[cfg(target_os = "windows")]
        {
            let windows_path = r"C:\temp\demo.cast";
            let resolved_win = resolve_cast_file_path(windows_path).unwrap();
            println!(
                "Windows path '{}' resolved to: '{}'",
                windows_path, resolved_win
            );
            assert!(Path::new(&resolved_win).is_absolute());
        }

        // Test path with parent directory references
        let complex_path = "./examples/../examples/demo.cast";
        let resolved_complex = resolve_cast_file_path(complex_path).unwrap();
        println!(
            "Complex path '{}' resolved to: '{}'",
            complex_path, resolved_complex
        );
        assert!(Path::new(&resolved_complex).is_absolute());
        assert!(resolved_complex.ends_with("demo.cast"));
    }

    #[test]
    fn test_get_reader_cast_file() {
        // This test assumes the examples/demo.cast file exists
        let input = "examples/demo.cast";
        let result = get_reader(input);

        // Should not panic and should attempt to read as file
        match result {
            Ok(_) => {
                // File exists and can be read
            }
            Err(e) => {
                // File might not exist, but should be treated as file path
                assert!(e.to_string().contains("Failed to open file"));
            }
        }
    }

    #[test]
    fn test_get_reader_url() {
        let url_input = "https://example.com/demo.cast";
        // This will likely fail due to network, but should be recognized as URL
        let result = get_reader(url_input);

        // Should attempt to fetch URL, not treat as file path
        match result {
            Ok(_) => {
                // Unlikely to succeed in test environment
            }
            Err(e) => {
                // Should be a network error, not a file error
                let error_msg = e.to_string();
                assert!(!error_msg.contains("Failed to open file"));
            }
        }
    }

    #[test]
    fn test_get_reader_remote_id() {
        let remote_id = "123456";
        let result = get_reader(remote_id);

        // Should attempt to fetch from asciinema.org
        match result {
            Ok(_) => {
                // Unlikely to succeed in test environment
            }
            Err(e) => {
                // Should be a network error, not a file error
                let error_msg = e.to_string();
                assert!(!error_msg.contains("Failed to open file"));
            }
        }
    }

    #[test]
    fn test_resolve_output_path() {
        // Test relative output path
        let relative_output = "output.svg";
        let resolved = resolve_output_path(relative_output).unwrap();
        println!(
            "Relative output path '{}' resolved to: '{}'",
            relative_output, resolved
        );
        assert!(Path::new(&resolved).is_absolute());
        assert!(resolved.ends_with("output.svg"));

        // Test output path with subdirectory
        let subdir_output = "results/animation.svg";
        let resolved_subdir = resolve_output_path(subdir_output).unwrap();
        println!(
            "Subdir output path '{}' resolved to: '{}'",
            subdir_output, resolved_subdir
        );
        assert!(Path::new(&resolved_subdir).is_absolute());
        assert!(resolved_subdir.ends_with("animation.svg"));

        // Verify parent directory was created
        let parent_path = Path::new(&resolved_subdir).parent().unwrap();
        assert!(parent_path.exists());

        // Test complex output path with parent directory references
        let complex_output = "./output/../final/result.svg";
        let resolved_complex = resolve_output_path(complex_output).unwrap();
        println!(
            "Complex output path '{}' resolved to: '{}'",
            complex_output, resolved_complex
        );
        assert!(Path::new(&resolved_complex).is_absolute());
        assert!(resolved_complex.ends_with("result.svg"));

        // Verify parent directory was created
        let complex_parent_path = Path::new(&resolved_complex).parent().unwrap();
        assert!(complex_parent_path.exists());

        // Clean up test directories
        let _ = std::fs::remove_dir_all("results");
        let _ = std::fs::remove_dir_all("final");
    }
}
