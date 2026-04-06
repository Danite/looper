use looper::infra::prisma::{find_and_parse_prisma, parse_prisma_file};
use std::fs;
use tempfile::TempDir;

#[test]
fn parse_basic_schema() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("schema.prisma");
    fs::write(
        &path,
        r#"
datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

generator client {
  provider = "prisma-client-js"
}
"#,
    )
    .unwrap();

    let result = parse_prisma_file(&path).unwrap();
    assert_eq!(result.entries.len(), 1);
    assert!(result.entries.contains_key("DATABASE_URL"));
    assert_eq!(result.entries["DATABASE_URL"].line, 4);
}

#[test]
fn parse_multiple_env_refs() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("schema.prisma");
    fs::write(
        &path,
        r#"
datasource db {
  provider  = "postgresql"
  url       = env("DATABASE_URL")
  directUrl = env("DIRECT_URL")
}
"#,
    )
    .unwrap();

    let result = parse_prisma_file(&path).unwrap();
    assert_eq!(result.entries.len(), 2);
    assert!(result.entries.contains_key("DATABASE_URL"));
    assert!(result.entries.contains_key("DIRECT_URL"));
}

#[test]
fn parse_env_with_spaces() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("schema.prisma");
    fs::write(
        &path,
        r#"
datasource db {
  url = env( "SPACED_URL" )
}
"#,
    )
    .unwrap();

    let result = parse_prisma_file(&path).unwrap();
    assert!(result.entries.contains_key("SPACED_URL"));
}

#[test]
fn no_env_calls_returns_empty() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("schema.prisma");
    fs::write(
        &path,
        r#"
generator client {
  provider = "prisma-client-js"
}
"#,
    )
    .unwrap();

    let result = parse_prisma_file(&path).unwrap();
    assert!(result.entries.is_empty());
}

#[test]
fn find_prisma_in_prisma_subdir() {
    let dir = TempDir::new().unwrap();
    let prisma_dir = dir.path().join("prisma");
    fs::create_dir(&prisma_dir).unwrap();
    fs::write(
        prisma_dir.join("schema.prisma"),
        r#"
datasource db {
  url = env("DB_URL")
}
"#,
    )
    .unwrap();

    let result = find_and_parse_prisma(dir.path());
    assert!(result.entries.contains_key("DB_URL"));
    assert!(result.source_file.is_some());
}

#[test]
fn find_prisma_in_root() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("schema.prisma"),
        r#"
datasource db {
  url = env("ROOT_DB")
}
"#,
    )
    .unwrap();

    let result = find_and_parse_prisma(dir.path());
    assert!(result.entries.contains_key("ROOT_DB"));
}

#[test]
fn find_returns_empty_when_no_prisma() {
    let dir = TempDir::new().unwrap();
    let result = find_and_parse_prisma(dir.path());
    assert!(result.entries.is_empty());
    assert!(result.source_file.is_none());
}

#[test]
fn source_file_and_line_tracked() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("schema.prisma");
    fs::write(
        &path,
        "// header\ndatasource db {\n  url = env(\"MY_VAR\")\n}\n",
    )
    .unwrap();

    let result = parse_prisma_file(&path).unwrap();
    let entry = &result.entries["MY_VAR"];
    assert_eq!(entry.line, 3);
    assert!(entry.source_file.contains("schema.prisma"));
}

#[test]
fn duplicate_env_ref_uses_first_occurrence() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("schema.prisma");
    fs::write(
        &path,
        r#"
datasource db {
  url       = env("DATABASE_URL")
  directUrl = env("DATABASE_URL")
}
"#,
    )
    .unwrap();

    let result = parse_prisma_file(&path).unwrap();
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries["DATABASE_URL"].line, 3);
}
