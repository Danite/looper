use looper::infra::dockerfile::{find_and_parse_dockerfile, parse_dockerfile};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

// --- Basic parsing ---

#[test]
fn parse_fixture_dockerfile() {
    let result = parse_dockerfile(Path::new("tests/fixtures/infra/Dockerfile")).unwrap();
    assert!(result.base_image.is_some());
    assert!(!result.binaries.is_empty());
    assert!(!result.exposed_ports.is_empty());
}

#[test]
fn extracts_base_image() {
    let result = parse_dockerfile(Path::new("tests/fixtures/infra/Dockerfile")).unwrap();
    // Last FROM wins (multi-stage)
    assert!(result.base_image.unwrap().contains("node"));
}

#[test]
fn extracts_node_binary_from_image() {
    let result = parse_dockerfile(Path::new("tests/fixtures/infra/Dockerfile")).unwrap();
    assert!(result.binaries.contains("node"));
}

#[test]
fn extracts_apk_packages() {
    let result = parse_dockerfile(Path::new("tests/fixtures/infra/Dockerfile")).unwrap();
    // ffmpeg package -> ffmpeg, ffprobe
    assert!(result.binaries.contains("ffmpeg"));
    assert!(result.binaries.contains("ffprobe"));
    // imagemagick -> convert, mogrify, etc.
    assert!(result.binaries.contains("convert"));
    assert!(result.binaries.contains("mogrify"));
    // git
    assert!(result.binaries.contains("git"));
    // curl
    assert!(result.binaries.contains("curl"));
}

#[test]
fn extracts_expose_ports() {
    let result = parse_dockerfile(Path::new("tests/fixtures/infra/Dockerfile")).unwrap();
    assert!(result.exposed_ports.contains(&3000));
    assert!(result.exposed_ports.contains(&8080));
}

#[test]
fn extracts_copy_sources() {
    let result = parse_dockerfile(Path::new("tests/fixtures/infra/Dockerfile")).unwrap();
    assert!(result.copy_sources.iter().any(|s| s.contains("package.json")));
    assert!(result.copy_sources.iter().any(|s| s.contains("src/")));
}

// --- apt-get install ---

#[test]
fn parses_apt_get_install() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Dockerfile");
    fs::write(&path, "FROM ubuntu:22.04\nRUN apt-get update && apt-get install -y wget jq\n").unwrap();

    let result = parse_dockerfile(&path).unwrap();
    assert!(result.binaries.contains("wget"));
    assert!(result.binaries.contains("jq"));
}

#[test]
fn parses_yum_install() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Dockerfile");
    fs::write(&path, "FROM centos:7\nRUN yum install -y git curl\n").unwrap();

    let result = parse_dockerfile(&path).unwrap();
    assert!(result.binaries.contains("git"));
    assert!(result.binaries.contains("curl"));
}

// --- Continuation lines ---

#[test]
fn handles_continuation_lines() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Dockerfile");
    fs::write(&path, "FROM alpine\nRUN apk add \\\n    ffmpeg \\\n    git\n").unwrap();

    let result = parse_dockerfile(&path).unwrap();
    assert!(result.binaries.contains("ffmpeg"));
    assert!(result.binaries.contains("git"));
}

// --- EXPOSE with protocol ---

#[test]
fn expose_with_protocol() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Dockerfile");
    fs::write(&path, "FROM alpine\nEXPOSE 443/tcp 8080/udp\n").unwrap();

    let result = parse_dockerfile(&path).unwrap();
    assert!(result.exposed_ports.contains(&443));
    assert!(result.exposed_ports.contains(&8080));
}

// --- COPY with flags ---

#[test]
fn copy_with_from_flag() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Dockerfile");
    fs::write(&path, "FROM alpine\nCOPY --from=builder /app/dist ./dist\n").unwrap();

    let result = parse_dockerfile(&path).unwrap();
    assert!(result.copy_sources.iter().any(|s| s.contains("/app/dist")));
}

// --- Image-to-binary mapping ---

#[test]
fn python_image_provides_python() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Dockerfile");
    fs::write(&path, "FROM python:3.12\n").unwrap();

    let result = parse_dockerfile(&path).unwrap();
    assert!(result.binaries.contains("python"));
}

#[test]
fn golang_image_provides_go() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Dockerfile");
    fs::write(&path, "FROM golang:1.22\n").unwrap();

    let result = parse_dockerfile(&path).unwrap();
    assert!(result.binaries.contains("go"));
}

// --- Auto-detection ---

#[test]
fn find_and_parse_detects_dockerfile() {
    let result = find_and_parse_dockerfile(Path::new("tests/fixtures/infra"));
    assert!(result.base_image.is_some());
}

#[test]
fn find_and_parse_returns_empty_when_missing() {
    let dir = TempDir::new().unwrap();
    let result = find_and_parse_dockerfile(dir.path());
    assert!(result.base_image.is_none());
    assert!(result.binaries.is_empty());
}

// --- JSON exec form ---

#[test]
fn parses_json_exec_form_install() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Dockerfile");
    fs::write(&path, r#"FROM alpine
RUN ["apt-get", "install", "-y", "curl", "git"]
"#).unwrap();

    let result = parse_dockerfile(&path).unwrap();
    assert!(result.binaries.contains("curl"));
    assert!(result.binaries.contains("git"));
}

#[test]
fn parses_json_exec_form_apk() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Dockerfile");
    fs::write(&path, r#"FROM alpine
RUN ["apk", "add", "ffmpeg"]
"#).unwrap();

    let result = parse_dockerfile(&path).unwrap();
    assert!(result.binaries.contains("ffmpeg"));
    assert!(result.binaries.contains("ffprobe"));
}

#[test]
fn json_exec_form_non_install_ignored() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Dockerfile");
    fs::write(&path, r#"FROM node:20
RUN ["node", "build.js"]
"#).unwrap();

    let result = parse_dockerfile(&path).unwrap();
    assert!(!result.binaries.contains("build.js"));
}

// --- Edge cases ---

#[test]
fn missing_file_returns_error() {
    let result = parse_dockerfile(Path::new("/tmp/nonexistent_dockerfile_xyz"));
    assert!(result.is_err());
}

#[test]
fn empty_dockerfile() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Dockerfile");
    fs::write(&path, "").unwrap();

    let result = parse_dockerfile(&path).unwrap();
    assert!(result.base_image.is_none());
    assert!(result.binaries.is_empty());
}

#[test]
fn unknown_package_uses_name_as_binary() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Dockerfile");
    fs::write(&path, "FROM alpine\nRUN apk add some-custom-tool\n").unwrap();

    let result = parse_dockerfile(&path).unwrap();
    assert!(result.binaries.contains("some-custom-tool"));
}

#[test]
fn source_file_tracked() {
    let result = parse_dockerfile(Path::new("tests/fixtures/infra/Dockerfile")).unwrap();
    assert!(result.source_file.unwrap().contains("Dockerfile"));
}
