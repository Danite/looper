use looper::generate::readme::generate_readme;
use looper::types::{Category, Confidence, Location, Metadata, ShadowDep};
use serde_json::json;

fn env_dep(name: &str, required: bool, default_value: Option<&str>) -> ShadowDep {
    let mut meta = Metadata::new();
    meta.insert("required", json!(required));
    if let Some(dv) = default_value {
        meta.insert("default_value", json!(dv));
    }
    ShadowDep::new(Category::EnvVar, name, Confidence::High, Location::new("src/config.ts", 10, 1))
        .with_metadata(meta)
}

fn binary_dep(name: &str) -> ShadowDep {
    ShadowDep::new(Category::Binary, name, Confidence::High, Location::new("src/video.ts", 15, 1))
}

fn port_dep(port: u16, direction: &str, protocol: &str) -> ShadowDep {
    let mut meta = Metadata::new();
    meta.insert("direction", json!(direction));
    meta.insert("protocol_hint", json!(protocol));
    ShadowDep {
        id: format!("port::{port}::{direction}"),
        category: Category::Port,
        name: format!(":{port}"),
        confidence: Confidence::High,
        metadata: meta,
        locations: vec![Location::new("src/server.ts", 5, 1)],
    }
}

fn network_dep(host: &str) -> ShadowDep {
    let mut meta = Metadata::new();
    meta.insert("protocol", json!("https"));
    ShadowDep::new(Category::Network, host, Confidence::High, Location::new("src/api.ts", 20, 1))
        .with_metadata(meta)
}

fn fs_dep(path: &str, operation: &str) -> ShadowDep {
    let mut meta = Metadata::new();
    meta.insert("operation", json!(operation));
    ShadowDep::new(Category::Filesystem, path, Confidence::High, Location::new("src/io.ts", 8, 1))
        .with_metadata(meta)
}

#[test]
fn empty_deps() {
    let result = generate_readme(&[]);
    assert!(result.contains("No shadow dependencies"));
}

#[test]
fn env_vars_table() {
    let deps = vec![
        env_dep("DATABASE_URL", true, None),
        env_dep("PORT", false, Some("3000")),
    ];
    let result = generate_readme(&deps);
    assert!(result.contains("## Environment Variables"));
    assert!(result.contains("| `DATABASE_URL` | Yes | - |"));
    assert!(result.contains("| `PORT` | No | 3000 |"));
}

#[test]
fn binaries_with_install() {
    let deps = vec![binary_dep("ffmpeg"), binary_dep("curl")];
    let result = generate_readme(&deps);
    assert!(result.contains("## System Binaries"));
    assert!(result.contains("| `ffmpeg` | `apt-get install ffmpeg` |"));
    assert!(result.contains("| `curl` | `apt-get install curl` |"));
}

#[test]
fn ports_table() {
    let deps = vec![port_dep(3000, "listen", "http")];
    let result = generate_readme(&deps);
    assert!(result.contains("## Ports"));
    assert!(result.contains("| `:3000` | listen | http |"));
}

#[test]
fn network_endpoints() {
    let deps = vec![network_dep("api.stripe.com")];
    let result = generate_readme(&deps);
    assert!(result.contains("## Network Endpoints"));
    assert!(result.contains("- `https://api.stripe.com`"));
}

#[test]
fn filesystem_paths() {
    let deps = vec![fs_dep("./config.json", "read")];
    let result = generate_readme(&deps);
    assert!(result.contains("## Filesystem Paths"));
    assert!(result.contains("- `./config.json` (read)"));
}

#[test]
fn omits_empty_sections() {
    let deps = vec![binary_dep("git")];
    let result = generate_readme(&deps);
    assert!(result.contains("## System Binaries"));
    assert!(!result.contains("## Environment Variables"));
    assert!(!result.contains("## Ports"));
    assert!(!result.contains("## Network"));
    assert!(!result.contains("## Filesystem"));
}

#[test]
fn all_categories() {
    let deps = vec![
        env_dep("API_KEY", true, None),
        binary_dep("ffmpeg"),
        port_dep(3000, "listen", "http"),
        network_dep("api.stripe.com"),
        fs_dep("./config.json", "read"),
    ];
    let result = generate_readme(&deps);
    assert!(result.contains("## Environment Variables"));
    assert!(result.contains("## System Binaries"));
    assert!(result.contains("## Ports"));
    assert!(result.contains("## Network Endpoints"));
    assert!(result.contains("## Filesystem Paths"));
}
