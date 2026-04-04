use looper::infra::envfile::{find_and_parse_env, parse_env_file};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

// --- Basic parsing ---

#[test]
fn parse_simple_env_file() {
    let result = parse_env_file(Path::new("tests/fixtures/infra/.env")).unwrap();
    let names = result.var_names();

    assert!(names.contains("DATABASE_URL"));
    assert!(names.contains("REDIS_URL"));
    assert!(names.contains("STRIPE_SECRET_KEY"));
    assert!(names.contains("JWT_SECRET"));
    assert!(names.contains("PORT"));
    assert!(names.contains("NODE_ENV"));
    assert!(names.contains("DEBUG"));
}

#[test]
fn skips_comments_and_empty_lines() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".env");
    fs::write(&path, "# comment\n\nVAR_A=1\n\n# another\nVAR_B=2\n").unwrap();

    let result = parse_env_file(&path).unwrap();
    assert_eq!(result.entries.len(), 2);
    assert!(result.var_names().contains("VAR_A"));
    assert!(result.var_names().contains("VAR_B"));
}

#[test]
fn handles_empty_values() {
    let result = parse_env_file(Path::new("tests/fixtures/infra/.env")).unwrap();
    assert!(result.var_names().contains("EMPTY_VAR"));
}

#[test]
fn handles_quoted_values() {
    let result = parse_env_file(Path::new("tests/fixtures/infra/.env")).unwrap();
    assert!(result.var_names().contains("QUOTED_DOUBLE"));
    assert!(result.var_names().contains("QUOTED_SINGLE"));
}

#[test]
fn handles_export_prefix() {
    let result = parse_env_file(Path::new("tests/fixtures/infra/.env")).unwrap();
    assert!(result.var_names().contains("EXPORTED_VAR"));
}

#[test]
fn tracks_line_numbers() {
    let result = parse_env_file(Path::new("tests/fixtures/infra/.env")).unwrap();
    let db = &result.entries["DATABASE_URL"];
    assert_eq!(db.line, 2);
    let port = &result.entries["PORT"];
    assert_eq!(port.line, 10);
}

#[test]
fn get_source_formats_correctly() {
    let result = parse_env_file(Path::new("tests/fixtures/infra/.env")).unwrap();
    let source = result.get_source("DATABASE_URL").unwrap();
    assert!(source.contains(".env"));
    assert!(source.contains("line 2"));
}

// --- Auto-detection ---

#[test]
fn find_and_parse_prefers_env_over_example() {
    let result = find_and_parse_env(Path::new("tests/fixtures/infra"));
    let names = result.var_names();

    // .env has REDIS_URL but .env.example doesn't
    assert!(names.contains("REDIS_URL"));
    // .env.example has FALLBACK_ONLY_VAR but .env doesn't
    assert!(!names.contains("FALLBACK_ONLY_VAR"));
}

#[test]
fn find_and_parse_falls_back_to_example() {
    let dir = TempDir::new().unwrap();
    // No .env, only .env.example
    fs::write(dir.path().join(".env.example"), "FALLBACK_VAR=default\n").unwrap();

    let result = find_and_parse_env(dir.path());
    assert!(result.var_names().contains("FALLBACK_VAR"));
}

#[test]
fn find_and_parse_returns_empty_when_no_files() {
    let dir = TempDir::new().unwrap();
    let result = find_and_parse_env(dir.path());
    assert!(result.entries.is_empty());
}

// --- Edge cases ---

#[test]
fn rejects_invalid_var_names() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".env");
    fs::write(&path, "123BAD=val\nGOOD_VAR=val\n=no_name\n").unwrap();

    let result = parse_env_file(&path).unwrap();
    assert_eq!(result.entries.len(), 1);
    assert!(result.var_names().contains("GOOD_VAR"));
}

#[test]
fn handles_inline_comments() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".env");
    fs::write(&path, "VAR=value # this is a comment\n").unwrap();

    let result = parse_env_file(&path).unwrap();
    assert!(result.var_names().contains("VAR"));
}

#[test]
fn empty_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".env");
    fs::write(&path, "").unwrap();

    let result = parse_env_file(&path).unwrap();
    assert!(result.entries.is_empty());
}

#[test]
fn missing_file_returns_error() {
    let result = parse_env_file(Path::new("/tmp/nonexistent_env_xyz"));
    assert!(result.is_err());
}

#[test]
fn var_count_fixture() {
    let result = parse_env_file(Path::new("tests/fixtures/infra/.env")).unwrap();
    // DATABASE_URL, REDIS_URL, STRIPE_SECRET_KEY, JWT_SECRET, PORT, NODE_ENV,
    // DEBUG, EMPTY_VAR, QUOTED_DOUBLE, QUOTED_SINGLE, EXPORTED_VAR = 11
    assert_eq!(result.entries.len(), 11);
}
