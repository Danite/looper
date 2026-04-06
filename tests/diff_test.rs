use looper::diff::{diff_manifests, format_diff_json, format_diff_terminal};
use looper::output::build_manifest;
use looper::types::*;
use serde_json::json;

fn deps_a() -> Vec<ShadowDep> {
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

fn deps_b() -> Vec<ShadowDep> {
    vec![
        ShadowDep::new(
            Category::EnvVar,
            "DB_URL",
            Confidence::High,
            Location::new("a.ts", 1, 1),
        ),
        ShadowDep::new(
            Category::Binary,
            "git",
            Confidence::High,
            Location::new("a.ts", 3, 1),
        ),
        ShadowDep::new(
            Category::Port,
            ":3000",
            Confidence::High,
            Location::new("b.ts", 5, 1),
        ),
    ]
}

#[test]
fn diff_identical_manifests() {
    let old = build_manifest(".", 10, 5, deps_a());
    let new = build_manifest(".", 10, 5, deps_a());
    let diff = diff_manifests(&old, &new);
    assert!(diff.is_empty());
    assert_eq!(diff.total_changes(), 0);
}

#[test]
fn diff_detects_added() {
    let old = build_manifest(".", 10, 5, deps_a());
    let new = build_manifest(".", 10, 5, deps_b());
    let diff = diff_manifests(&old, &new);

    assert_eq!(diff.added.len(), 1);
    assert_eq!(diff.added[0].name, ":3000");
}

#[test]
fn diff_detects_removed() {
    let old = build_manifest(".", 10, 5, deps_a());
    let new = build_manifest(".", 10, 5, deps_b());
    let diff = diff_manifests(&old, &new);

    assert_eq!(diff.removed.len(), 1);
    assert_eq!(diff.removed[0].name, "api.com");
}

#[test]
fn diff_detects_changed() {
    let mut dep_modified = ShadowDep::new(
        Category::EnvVar,
        "DB_URL",
        Confidence::High,
        Location::new("a.ts", 1, 1),
    );
    dep_modified.locations.push(Location::new("b.ts", 10, 1));

    let old = build_manifest(".", 10, 5, deps_a());
    let new = build_manifest(
        ".",
        10,
        5,
        vec![
            dep_modified,
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
        ],
    );
    let diff = diff_manifests(&old, &new);

    assert_eq!(diff.changed.len(), 1);
    assert_eq!(diff.changed[0].name, "DB_URL");
    assert_eq!(diff.changed[0].old_locations_count, 1);
    assert_eq!(diff.changed[0].new_locations_count, 2);
}

#[test]
fn diff_empty_to_full() {
    let old = build_manifest(".", 0, 0, Vec::new());
    let new = build_manifest(".", 10, 5, deps_a());
    let diff = diff_manifests(&old, &new);

    assert_eq!(diff.added.len(), 3);
    assert!(diff.removed.is_empty());
    assert!(diff.changed.is_empty());
}

#[test]
fn diff_full_to_empty() {
    let old = build_manifest(".", 10, 5, deps_a());
    let new = build_manifest(".", 0, 0, Vec::new());
    let diff = diff_manifests(&old, &new);

    assert!(diff.added.is_empty());
    assert_eq!(diff.removed.len(), 3);
}

#[test]
fn format_terminal_no_changes() {
    let old = build_manifest(".", 10, 5, deps_a());
    let diff = diff_manifests(&old, &old);
    let output = format_diff_terminal(&diff);
    assert!(output.contains("No changes"));
}

#[test]
fn format_terminal_with_changes() {
    let old = build_manifest(".", 10, 5, deps_a());
    let new = build_manifest(".", 10, 5, deps_b());
    let diff = diff_manifests(&old, &new);
    let output = format_diff_terminal(&diff);

    assert!(output.contains("Added"));
    assert!(output.contains("Removed"));
    assert!(output.contains(":3000"));
    assert!(output.contains("api.com"));
}

#[test]
fn format_json_valid() {
    let old = build_manifest(".", 10, 5, deps_a());
    let new = build_manifest(".", 10, 5, deps_b());
    let diff = diff_manifests(&old, &new);
    let json = format_diff_json(&diff).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(parsed["added"].is_array());
    assert!(parsed["removed"].is_array());
    assert!(parsed["changed"].is_array());
    assert_eq!(parsed["added"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["removed"].as_array().unwrap().len(), 1);
}

#[test]
fn total_changes() {
    let old = build_manifest(".", 10, 5, deps_a());
    let new = build_manifest(".", 10, 5, deps_b());
    let diff = diff_manifests(&old, &new);
    assert_eq!(diff.total_changes(), 2); // 1 added + 1 removed
}

#[test]
fn diff_confidence_change_detected() {
    let old_deps = vec![ShadowDep::new(
        Category::EnvVar,
        "X",
        Confidence::Medium,
        Location::new("a.ts", 1, 1),
    )];
    let new_deps = vec![ShadowDep::new(
        Category::EnvVar,
        "X",
        Confidence::High,
        Location::new("a.ts", 1, 1),
    )];
    let old = build_manifest(".", 1, 1, old_deps);
    let new = build_manifest(".", 1, 1, new_deps);
    let diff = diff_manifests(&old, &new);

    assert_eq!(diff.changed.len(), 1);
}

#[test]
fn diff_metadata_change_detected() {
    let mut meta = Metadata::new();
    meta.insert("required", json!(true));
    let old_deps = vec![ShadowDep::new(
        Category::EnvVar,
        "X",
        Confidence::High,
        Location::new("a.ts", 1, 1),
    )];
    let new_deps = vec![ShadowDep::new(
        Category::EnvVar,
        "X",
        Confidence::High,
        Location::new("a.ts", 1, 1),
    )
    .with_metadata(meta)];
    let old = build_manifest(".", 1, 1, old_deps);
    let new = build_manifest(".", 1, 1, new_deps);
    let diff = diff_manifests(&old, &new);

    assert_eq!(diff.changed.len(), 1);
}
