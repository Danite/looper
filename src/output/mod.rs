pub mod html;
pub mod json;
pub mod sarif;
pub mod terminal;

use crate::types::{ScanManifest, ShadowDep};
use std::io::{self, Write};
use std::path::Path;

/// Write output to a file or stdout.
pub fn write_output(content: &str, output_path: Option<&Path>) -> io::Result<()> {
    match output_path {
        Some(path) => std::fs::write(path, content),
        None => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            handle.write_all(content.as_bytes())?;
            handle.write_all(b"\n")?;
            Ok(())
        }
    }
}

/// Build a ScanManifest from scan results.
pub fn build_manifest(
    root: &str,
    files_scanned: usize,
    scan_duration_ms: u64,
    deps: Vec<ShadowDep>,
) -> ScanManifest {
    let mut manifest = ScanManifest::new(root, files_scanned, scan_duration_ms);
    manifest.deps = deps;
    manifest
}

/// Format and write scan results in the requested format.
pub fn format_and_write(
    format: &str,
    quiet: bool,
    output_path: Option<&Path>,
    root: &str,
    file_count: usize,
    duration_ms: u64,
    deps: Vec<ShadowDep>,
) -> Result<(), String> {
    match format {
        "json" => {
            let manifest = build_manifest(root, file_count, duration_ms, deps);
            let content = json::format_json(&manifest)
                .map_err(|e| format!("JSON serialization failed: {e}"))?;
            write_output(&content, output_path).map_err(|e| e.to_string())
        }
        "sarif" => {
            let manifest = build_manifest(root, file_count, duration_ms, deps);
            let content = sarif::format_sarif(&manifest)
                .map_err(|e| format!("SARIF serialization failed: {e}"))?;
            write_output(&content, output_path).map_err(|e| e.to_string())
        }
        "html" => {
            if output_path.is_none() {
                return Err("HTML output requires --output <file>".into());
            }
            let manifest = build_manifest(root, file_count, duration_ms, deps);
            let content = html::format_html(&manifest)
                .map_err(|e| format!("HTML generation failed: {e}"))?;
            write_output(&content, output_path).map_err(|e| e.to_string())
        }
        _ => {
            let content = if quiet {
                terminal::format_summary(&deps, file_count, duration_ms)
            } else {
                terminal::format_terminal(&deps, file_count, duration_ms)
            };
            print!("{content}");
            Ok(())
        }
    }
}
