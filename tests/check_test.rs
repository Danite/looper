use looper::check::{check_against_manifest, load_manifest};
use looper::output::build_manifest;
use looper::types::*;
use std::fs;
use tempfile::TempDir;

fn sample_deps() -> Vec<ShadowDep> {
    vec![
        ShadowDep::new(
            Category::EnvVar,
            "DB_URL",
            Confidence::High,
            Location::new("a.ts", 1, 1),
        ),
        ShadowDep::new(
            Category::Network,
            "api.com",
            Confidence::High,
            Location::new("a.ts", 2, 1),
        ),
        ShadowDep::new(
            Category::Binary,
            "git",
            Confidence::High,
            Location::new("a.ts", 3, 1),
        ),
    ]
}

#[test]
fn check_clean_when_identical() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let result = check_against_manifest(&sample_deps(), &manifest);
    assert!(result.is_clean());
    assert!(result.added.is_empty());
    assert!(result.removed.is_empty());
}

#[test]
fn check_detects_new_deps() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let mut current = sample_deps();
    current.push(ShadowDep::new(
        Category::EnvVar,
        "NEW_VAR",
        Confidence::High,
        Location::new("b.ts", 1, 1),
    ));

    let result = check_against_manifest(&current, &manifest);
    assert!(!result.is_clean());
    assert_eq!(result.added.len(), 1);
    assert_eq!(result.added[0].name, "NEW_VAR");
    assert!(result.removed.is_empty());
}

#[test]
fn check_detects_removed_deps() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let current = vec![sample_deps()[0].clone()];

    let result = check_against_manifest(&current, &manifest);
    assert!(!result.is_clean());
    assert!(result.added.is_empty());
    assert_eq!(result.removed.len(), 2);
}

#[test]
fn check_detects_both_added_and_removed() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let current = vec![
        sample_deps()[0].clone(),
        ShadowDep::new(
            Category::Port,
            ":8080",
            Confidence::High,
            Location::new("c.ts", 1, 1),
        ),
    ];

    let result = check_against_manifest(&current, &manifest);
    assert_eq!(result.added.len(), 1);
    assert_eq!(result.added[0].name, ":8080");
    assert_eq!(result.removed.len(), 2);
}

#[test]
fn check_empty_manifest_all_new() {
    let manifest = build_manifest(".", 0, 0, Vec::new());
    let result = check_against_manifest(&sample_deps(), &manifest);
    assert_eq!(result.added.len(), 3);
    assert!(result.removed.is_empty());
}

#[test]
fn check_empty_scan_all_removed() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let result = check_against_manifest(&[], &manifest);
    assert!(result.added.is_empty());
    assert_eq!(result.removed.len(), 3);
}

#[test]
fn load_manifest_valid() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("manifest.json");
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let json = serde_json::to_string_pretty(&manifest).unwrap();
    fs::write(&path, &json).unwrap();

    let loaded = load_manifest(&path).unwrap();
    assert_eq!(loaded.deps.len(), 3);
    assert_eq!(loaded.schema_version, "1");
}

#[test]
fn load_manifest_missing_file() {
    let result = load_manifest(std::path::Path::new("/tmp/nonexistent_manifest_xyz.json"));
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("could not read"));
}

#[test]
fn load_manifest_invalid_json() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bad.json");
    fs::write(&path, "not json {{{").unwrap();

    let result = load_manifest(&path);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("invalid manifest"));
}

#[test]
fn is_clean_helper() {
    let manifest = build_manifest(".", 10, 5, sample_deps());
    let clean = check_against_manifest(&sample_deps(), &manifest);
    assert!(clean.is_clean());

    let mut more = sample_deps();
    more.push(ShadowDep::new(
        Category::EnvVar,
        "X",
        Confidence::High,
        Location::new("x.ts", 1, 1),
    ));
    let dirty = check_against_manifest(&more, &manifest);
    assert!(!dirty.is_clean());
}
