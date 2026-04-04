pub mod json;

use crate::types::ScanManifest;
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
    deps: Vec<crate::types::ShadowDep>,
) -> ScanManifest {
    let mut manifest = ScanManifest::new(root, files_scanned, scan_duration_ms);
    manifest.deps = deps;
    manifest
}
