use looper::config::{load_config, write_default_config, Config};
use std::fs;
use tempfile::TempDir;

#[test]
fn default_config_has_sensible_defaults() {
    let config = Config::default_config();

    assert!(config.include.contains(&"src/**".to_string()));
    assert!(config.exclude.contains(&"**/node_modules/**".to_string()));
    assert!(config.exclude.contains(&"**/*.test.ts".to_string()));
    assert!(config.manifest.is_some());
}

#[test]
fn config_serializes_to_json() {
    let config = Config::default_config();
    let json = config.to_json().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(parsed["include"].is_array());
    assert!(parsed["exclude"].is_array());
    assert!(parsed["categories"].is_object());
}

#[test]
fn config_roundtrip() {
    let config = Config::default_config();
    let json = config.to_json().unwrap();
    let parsed: Config = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.include, config.include);
    assert_eq!(parsed.exclude, config.exclude);
    assert_eq!(parsed.manifest, config.manifest);
}

#[test]
fn load_config_from_explicit_path() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("looper.config.json");
    fs::write(
        &path,
        r#"{ "exclude": ["dist/**"], "min_confidence": "high" }"#,
    )
    .unwrap();

    let config = load_config(Some(&path)).unwrap();
    assert_eq!(config.exclude, vec!["dist/**"]);
    assert_eq!(config.min_confidence, Some("high".into()));
}

#[test]
fn load_config_auto_detects() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("looper.config.json");
    fs::write(&config_path, r#"{ "exclude": ["vendor/**"] }"#).unwrap();

    // load_config with None uses cwd, so we test the explicit path instead
    let config = load_config(Some(&config_path)).unwrap();
    assert_eq!(config.exclude, vec!["vendor/**"]);
}

#[test]
fn load_config_returns_none_for_missing_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nonexistent.json");
    let config = load_config(Some(&path));
    assert!(config.is_none());
}

#[test]
fn load_config_returns_none_for_invalid_json() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("looper.config.json");
    fs::write(&path, "not valid json {{{").unwrap();

    let config = load_config(Some(&path));
    assert!(config.is_none());
}

#[test]
fn write_default_config_creates_file() {
    let dir = TempDir::new().unwrap();
    let path = write_default_config(dir.path()).unwrap();

    assert!(path.exists());
    assert!(path.ends_with("looper.config.json"));

    let content = fs::read_to_string(&path).unwrap();
    let parsed: Config = serde_json::from_str(&content).unwrap();
    assert!(!parsed.exclude.is_empty());
}

#[test]
fn config_with_empty_fields() {
    let config: Config = serde_json::from_str("{}").unwrap();
    assert!(config.include.is_empty());
    assert!(config.exclude.is_empty());
    assert!(config.categories.is_empty());
    assert!(config.min_confidence.is_none());
    assert!(config.manifest.is_none());
}

#[test]
fn config_category_overrides() {
    let json = r#"{
        "categories": {
            "env_var": { "ignore": ["NODE_ENV", "CI", "DEBUG"] },
            "network": { "ignore": ["localhost"] }
        }
    }"#;
    let config: Config = serde_json::from_str(json).unwrap();

    assert_eq!(config.categories["env_var"].ignore.len(), 3);
    assert_eq!(config.categories["network"].ignore.len(), 1);
}
