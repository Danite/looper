use looper::doctor::run_doctor;
use looper::types::*;
use serde_json::json;
use std::path::Path;

fn env_dep(name: &str) -> ShadowDep {
    let mut meta = Metadata::new();
    meta.insert("required", json!(true));
    ShadowDep::new(Category::EnvVar, name, Confidence::High, Location::new("a.ts", 1, 1))
        .with_metadata(meta)
}

fn optional_env_dep(name: &str, default: &str) -> ShadowDep {
    let mut meta = Metadata::new();
    meta.insert("required", json!(false));
    meta.insert("default_value", json!(default));
    ShadowDep::new(Category::EnvVar, name, Confidence::High, Location::new("a.ts", 1, 1))
        .with_metadata(meta)
}

fn binary_dep(name: &str) -> ShadowDep {
    ShadowDep::new(Category::Binary, name, Confidence::High, Location::new("a.ts", 2, 1))
}

fn fs_dep(path: &str) -> ShadowDep {
    ShadowDep::new(Category::Filesystem, path, Confidence::High, Location::new("a.ts", 3, 1))
}

fn platform_dep() -> ShadowDep {
    ShadowDep::new(Category::Platform, "process.platform", Confidence::High, Location::new("a.ts", 4, 1))
}

fn dynamic_env_dep() -> ShadowDep {
    ShadowDep::new(Category::EnvVar, "<dynamic>", Confidence::Low, Location::new("a.ts", 5, 1))
}

// --- Env var checks ---

#[test]
fn env_var_set_passes() {
    std::env::set_var("LOOPER_TEST_VAR", "hello");
    let deps = vec![env_dep("LOOPER_TEST_VAR")];
    let report = run_doctor(&deps, 1000, Path::new("."));

    assert_eq!(report.checks.len(), 1);
    assert!(report.checks[0].passed);
    assert!(report.checks[0].detail.contains("hello"));
    std::env::remove_var("LOOPER_TEST_VAR");
}

#[test]
fn env_var_missing_fails() {
    std::env::remove_var("LOOPER_MISSING_VAR_XYZ");
    let deps = vec![env_dep("LOOPER_MISSING_VAR_XYZ")];
    let report = run_doctor(&deps, 1000, Path::new("."));

    assert!(!report.checks[0].passed);
    assert_eq!(report.checks[0].detail, "missing");
    assert!(report.checks[0].suggestion.is_some());
}

#[test]
fn env_var_secret_redacted() {
    std::env::set_var("LOOPER_SECRET_KEY", "super-secret");
    let deps = vec![env_dep("LOOPER_SECRET_KEY")];
    let report = run_doctor(&deps, 1000, Path::new("."));

    assert!(report.checks[0].passed);
    assert!(report.checks[0].detail.contains("*****"));
    assert!(!report.checks[0].detail.contains("super-secret"));
    std::env::remove_var("LOOPER_SECRET_KEY");
}

#[test]
fn env_var_optional_not_set_still_passes() {
    std::env::remove_var("LOOPER_OPT_VAR_XYZ");
    let deps = vec![optional_env_dep("LOOPER_OPT_VAR_XYZ", "3000")];
    let report = run_doctor(&deps, 1000, Path::new("."));

    assert!(report.checks[0].passed);
    assert!(report.checks[0].detail.contains("optional"));
    assert!(report.checks[0].detail.contains("3000"));
}

#[test]
fn env_var_dynamic_skipped() {
    let deps = vec![dynamic_env_dep()];
    let report = run_doctor(&deps, 1000, Path::new("."));
    assert!(report.checks.is_empty());
}

// --- Binary checks ---

#[test]
fn binary_found_passes() {
    // 'sh' should exist on any Unix system
    let deps = vec![binary_dep("sh")];
    let report = run_doctor(&deps, 1000, Path::new("."));

    assert!(report.checks[0].passed);
    assert!(report.checks[0].detail.contains('/'));
}

#[test]
fn binary_not_found_fails() {
    let deps = vec![binary_dep("nonexistent_binary_xyz_looper")];
    let report = run_doctor(&deps, 1000, Path::new("."));

    assert!(!report.checks[0].passed);
    assert_eq!(report.checks[0].detail, "not found");
}

#[test]
fn binary_not_found_suggests_install() {
    let deps = vec![binary_dep("convert")];
    let report = run_doctor(&deps, 1000, Path::new("."));

    if !report.checks[0].passed {
        let suggestion = report.checks[0].suggestion.as_ref().unwrap();
        assert!(suggestion.contains("imagemagick"));
    }
}

// --- Filesystem checks ---

#[test]
fn filesystem_exists_passes() {
    let deps = vec![fs_dep("Cargo.toml")];
    let report = run_doctor(&deps, 1000, Path::new("."));

    assert!(report.checks[0].passed);
    assert_eq!(report.checks[0].detail, "exists");
}

#[test]
fn filesystem_missing_fails() {
    let deps = vec![fs_dep("nonexistent_file_xyz.json")];
    let report = run_doctor(&deps, 1000, Path::new("."));

    assert!(!report.checks[0].passed);
    assert_eq!(report.checks[0].detail, "missing");
}

#[test]
fn filesystem_relative_to_scan_root() {
    let deps = vec![fs_dep("docker-compose.yml")];
    let report = run_doctor(&deps, 1000, Path::new("tests/fixtures/infra"));

    assert!(report.checks[0].passed);
}

// --- Platform skipped ---

#[test]
fn platform_skipped() {
    let deps = vec![platform_dep()];
    let report = run_doctor(&deps, 1000, Path::new("."));
    assert!(report.checks.is_empty());
}

// --- Report helpers ---

#[test]
fn report_counts() {
    std::env::set_var("LOOPER_COUNT_TEST", "val");
    let deps = vec![
        env_dep("LOOPER_COUNT_TEST"),
        binary_dep("nonexistent_xyz_looper"),
        fs_dep("Cargo.toml"),
    ];
    let report = run_doctor(&deps, 1000, Path::new("."));

    assert_eq!(report.passed_count(), 2);
    assert_eq!(report.failed_count(), 1);
    assert!(!report.all_passed());
    std::env::remove_var("LOOPER_COUNT_TEST");
}

#[test]
fn all_passed_true() {
    std::env::set_var("LOOPER_ALL_PASS", "yes");
    let deps = vec![env_dep("LOOPER_ALL_PASS"), fs_dep("Cargo.toml")];
    let report = run_doctor(&deps, 1000, Path::new("."));

    assert!(report.all_passed());
    std::env::remove_var("LOOPER_ALL_PASS");
}

// --- JSON serializable ---

#[test]
fn report_serializes_to_json() {
    let deps = vec![fs_dep("Cargo.toml")];
    let report = run_doctor(&deps, 1000, Path::new("."));
    let json = serde_json::to_string_pretty(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(parsed["checks"].is_array());
    assert_eq!(parsed["checks"][0]["passed"], true);
}

// --- Empty deps ---

#[test]
fn empty_deps() {
    let report = run_doctor(&[], 1000, Path::new("."));
    assert!(report.checks.is_empty());
    assert!(report.all_passed());
}
