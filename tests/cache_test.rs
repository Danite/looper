use looper::cache::{
    hash_content, hash_file_or_empty, load_cache, save_cache, CacheEntry, ScanCache,
};
use looper::types::*;
use std::collections::HashMap;
use std::fs;
use tempfile::TempDir;

fn sample_entry() -> CacheEntry {
    CacheEntry {
        hash: "abc123".to_string(),
        deps: vec![ShadowDep::new(
            Category::EnvVar,
            "DB_URL",
            Confidence::High,
            Location::new("src/db.ts", 5, 1),
        )],
    }
}

fn sample_cache() -> ScanCache {
    let mut entries = HashMap::new();
    entries.insert("src/db.ts".to_string(), sample_entry());
    ScanCache {
        version: "0.1.0".to_string(),
        config_hash: "confighash".to_string(),
        entries,
    }
}

// --- hash_content ---

#[test]
fn hash_content_deterministic() {
    let h1 = hash_content("hello world");
    let h2 = hash_content("hello world");
    assert_eq!(h1, h2);
}

#[test]
fn hash_content_different_for_different_input() {
    let h1 = hash_content("hello");
    let h2 = hash_content("world");
    assert_ne!(h1, h2);
}

#[test]
fn hash_content_is_hex_string() {
    let h = hash_content("test");
    assert_eq!(h.len(), 16);
    assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
}

// --- hash_file_or_empty ---

#[test]
fn hash_file_returns_hash_for_existing_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    fs::write(&path, "content").unwrap();
    let h = hash_file_or_empty(&path);
    assert!(!h.is_empty());
    assert_eq!(h, hash_content("content"));
}

#[test]
fn hash_file_returns_empty_for_missing_file() {
    let h = hash_file_or_empty(std::path::Path::new("/nonexistent/file.txt"));
    assert!(h.is_empty());
}

// --- save_cache / load_cache roundtrip ---

#[test]
fn save_and_load_roundtrip() {
    let dir = TempDir::new().unwrap();
    let cache = sample_cache();

    save_cache(dir.path(), &cache);
    let loaded = load_cache(dir.path(), "0.1.0", "confighash");

    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.version, "0.1.0");
    assert_eq!(loaded.config_hash, "confighash");
    assert_eq!(loaded.entries.len(), 1);

    let entry = &loaded.entries["src/db.ts"];
    assert_eq!(entry.hash, "abc123");
    assert_eq!(entry.deps.len(), 1);
    assert_eq!(entry.deps[0].name, "DB_URL");
}

// --- Cache invalidation ---

#[test]
fn load_returns_none_for_version_mismatch() {
    let dir = TempDir::new().unwrap();
    save_cache(dir.path(), &sample_cache());

    let loaded = load_cache(dir.path(), "999.0.0", "confighash");
    assert!(loaded.is_none());
}

#[test]
fn load_returns_none_for_config_hash_mismatch() {
    let dir = TempDir::new().unwrap();
    save_cache(dir.path(), &sample_cache());

    let loaded = load_cache(dir.path(), "0.1.0", "differenthash");
    assert!(loaded.is_none());
}

#[test]
fn load_returns_none_for_missing_file() {
    let dir = TempDir::new().unwrap();
    let loaded = load_cache(dir.path(), "0.1.0", "confighash");
    assert!(loaded.is_none());
}

#[test]
fn load_returns_none_for_corrupt_file() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".looper-cache"), "not json").unwrap();
    let loaded = load_cache(dir.path(), "0.1.0", "confighash");
    assert!(loaded.is_none());
}

// --- ShadowDep serialization fidelity ---

#[test]
fn cache_preserves_metadata() {
    let dir = TempDir::new().unwrap();
    let mut dep = ShadowDep::new(
        Category::EnvVar,
        "PORT",
        Confidence::Medium,
        Location::new("src/server.ts", 10, 3),
    );
    let mut meta = Metadata::new();
    meta.insert("required", serde_json::json!(false));
    meta.insert("default_value", serde_json::json!("3000"));
    dep.metadata = meta;

    let cache = ScanCache {
        version: "0.1.0".to_string(),
        config_hash: String::new(),
        entries: HashMap::from([(
            "src/server.ts".to_string(),
            CacheEntry {
                hash: "xyz".to_string(),
                deps: vec![dep],
            },
        )]),
    };

    save_cache(dir.path(), &cache);
    let loaded = load_cache(dir.path(), "0.1.0", "").unwrap();
    let entry = &loaded.entries["src/server.ts"];
    let dep = &entry.deps[0];

    assert_eq!(dep.name, "PORT");
    assert_eq!(dep.confidence, Confidence::Medium);
    assert_eq!(dep.metadata.get_bool("required"), Some(false));
    assert_eq!(dep.metadata.get_str("default_value"), Some("3000"));
    assert_eq!(dep.locations[0].file, "src/server.ts");
    assert_eq!(dep.locations[0].line, 10);
}

#[test]
fn cache_preserves_multiple_entries() {
    let dir = TempDir::new().unwrap();
    let cache = ScanCache {
        version: "0.1.0".to_string(),
        config_hash: String::new(),
        entries: HashMap::from([
            (
                "a.ts".to_string(),
                CacheEntry {
                    hash: "h1".to_string(),
                    deps: vec![ShadowDep::new(
                        Category::Network,
                        "api.example.com",
                        Confidence::High,
                        Location::new("a.ts", 1, 1),
                    )],
                },
            ),
            (
                "b.ts".to_string(),
                CacheEntry {
                    hash: "h2".to_string(),
                    deps: vec![],
                },
            ),
        ]),
    };

    save_cache(dir.path(), &cache);
    let loaded = load_cache(dir.path(), "0.1.0", "").unwrap();
    assert_eq!(loaded.entries.len(), 2);
    assert_eq!(loaded.entries["a.ts"].deps.len(), 1);
    assert!(loaded.entries["b.ts"].deps.is_empty());
}
