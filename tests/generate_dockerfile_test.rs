use looper::generate::dockerfile::generate_dockerfile;
use looper::types::{Category, Confidence, Location, Metadata, ShadowDep};
use serde_json::json;
use tempfile::TempDir;

fn binary_dep(name: &str) -> ShadowDep {
    ShadowDep::new(
        Category::Binary,
        name,
        Confidence::High,
        Location::new("src/video.ts", 15, 1),
    )
}

fn listen_port_dep(port: u16) -> ShadowDep {
    let name = format!(":{port}");
    let id = format!("port::{port}::listen");
    let mut meta = Metadata::new();
    meta.insert("direction", json!("listen"));
    meta.insert("protocol_hint", json!("http"));
    ShadowDep {
        id,
        category: Category::Port,
        name,
        confidence: Confidence::High,
        metadata: meta,
        locations: vec![Location::new("src/server.ts", 10, 1)],
    }
}

#[test]
fn empty_deps_produces_header() {
    let tmp = TempDir::new().unwrap();
    let result = generate_dockerfile(&[], tmp.path());
    assert!(result.contains("No Dockerfile content"));
}

#[test]
fn known_binary_generates_apt_install() {
    let tmp = TempDir::new().unwrap();
    let deps = vec![binary_dep("ffmpeg")];
    let result = generate_dockerfile(&deps, tmp.path());
    assert!(result.contains("apt-get install"));
    assert!(result.contains("ffmpeg"));
}

#[test]
fn unknown_binary_adds_todo_comment() {
    let tmp = TempDir::new().unwrap();
    let deps = vec![binary_dep("my-custom-tool")];
    let result = generate_dockerfile(&deps, tmp.path());
    assert!(result.contains("# - my-custom-tool"));
    assert!(result.contains("TODO"));
}

#[test]
fn listen_port_generates_expose() {
    let tmp = TempDir::new().unwrap();
    let deps = vec![listen_port_dep(3000)];
    let result = generate_dockerfile(&deps, tmp.path());
    assert!(result.contains("EXPOSE 3000"));
}

#[test]
fn connect_port_not_exposed() {
    let tmp = TempDir::new().unwrap();
    let mut meta = Metadata::new();
    meta.insert("direction", json!("connect"));
    let dep = ShadowDep {
        id: "port::5432::connect".into(),
        category: Category::Port,
        name: ":5432".into(),
        confidence: Confidence::High,
        metadata: meta,
        locations: vec![Location::new("src/db.ts", 5, 1)],
    };
    let result = generate_dockerfile(&[dep], tmp.path());
    assert!(!result.contains("EXPOSE 5432"));
}

#[test]
fn detects_node_project() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("package.json"), "{}").unwrap();
    let deps = vec![binary_dep("ffmpeg")];
    let result = generate_dockerfile(&deps, tmp.path());
    assert!(result.contains("FROM node:20-slim"));
}

#[test]
fn detects_python_project() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("requirements.txt"), "").unwrap();
    let deps = vec![binary_dep("curl")];
    let result = generate_dockerfile(&deps, tmp.path());
    assert!(result.contains("FROM python:3.12-slim"));
}

#[test]
fn fallback_base_image() {
    let tmp = TempDir::new().unwrap();
    let deps = vec![binary_dep("curl")];
    let result = generate_dockerfile(&deps, tmp.path());
    assert!(result.contains("FROM debian:bookworm-slim"));
}

#[test]
fn includes_workdir_and_copy() {
    let tmp = TempDir::new().unwrap();
    let deps = vec![binary_dep("git")];
    let result = generate_dockerfile(&deps, tmp.path());
    assert!(result.contains("WORKDIR /app"));
    assert!(result.contains("COPY . ."));
}

#[test]
fn deduplicates_packages() {
    let tmp = TempDir::new().unwrap();
    let deps = vec![binary_dep("ffmpeg"), binary_dep("ffprobe")];
    let result = generate_dockerfile(&deps, tmp.path());
    let count = result.matches("ffmpeg").count();
    assert_eq!(
        count, 1,
        "ffmpeg and ffprobe both resolve to ffmpeg package"
    );
}

#[test]
fn non_binary_deps_ignored() {
    let tmp = TempDir::new().unwrap();
    let env_dep = ShadowDep::new(
        Category::EnvVar,
        "DATABASE_URL",
        Confidence::High,
        Location::new("src/db.ts", 5, 1),
    );
    let result = generate_dockerfile(&[env_dep], tmp.path());
    assert!(result.contains("No Dockerfile content"));
}

#[test]
fn packages_sorted_alphabetically() {
    let tmp = TempDir::new().unwrap();
    let deps = vec![binary_dep("wget"), binary_dep("curl"), binary_dep("git")];
    let result = generate_dockerfile(&deps, tmp.path());
    let curl_pos = result.find("curl").unwrap();
    let git_pos = result.find("git").unwrap();
    let wget_pos = result.find("wget").unwrap();
    assert!(curl_pos < git_pos);
    assert!(git_pos < wget_pos);
}
