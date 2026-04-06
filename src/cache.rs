use crate::types::ShadowDep;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

const CACHE_FILENAME: &str = ".looper-cache";

#[derive(Debug, Serialize, Deserialize)]
pub struct ScanCache {
    pub version: String,
    pub config_hash: String,
    pub entries: HashMap<String, CacheEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub hash: String,
    pub deps: Vec<ShadowDep>,
}

pub fn hash_content(content: &str) -> String {
    format!("{:016x}", xxhash_rust::xxh3::xxh3_64(content.as_bytes()))
}

pub fn hash_file_or_empty(path: &Path) -> String {
    std::fs::read_to_string(path)
        .map(|c| hash_content(&c))
        .unwrap_or_default()
}

pub fn load_cache(
    project_root: &Path,
    current_version: &str,
    current_config_hash: &str,
) -> Option<ScanCache> {
    let cache_path = project_root.join(CACHE_FILENAME);
    let content = std::fs::read_to_string(&cache_path).ok()?;
    let cache: ScanCache = serde_json::from_str(&content).ok()?;

    if cache.version != current_version || cache.config_hash != current_config_hash {
        return None;
    }

    Some(cache)
}

pub fn save_cache(project_root: &Path, cache: &ScanCache) {
    let cache_path = project_root.join(CACHE_FILENAME);
    let tmp_path = cache_path.with_extension("tmp");

    let content = match serde_json::to_string(cache) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("  warning: failed to serialize cache: {e}");
            return;
        }
    };
    if let Err(e) = std::fs::write(&tmp_path, &content) {
        eprintln!("  warning: failed to write cache: {e}");
        return;
    }
    if let Err(e) = std::fs::rename(&tmp_path, &cache_path) {
        eprintln!("  warning: failed to save cache: {e}");
    }
}
