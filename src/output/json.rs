use crate::types::ScanManifest;

/// Format a ScanManifest as pretty-printed JSON.
pub fn format_json(manifest: &ScanManifest) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(manifest)
}
