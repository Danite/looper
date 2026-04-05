use looper::generate::compose::generate_compose;
use looper::types::{Category, Confidence, Location, Metadata, ShadowDep};
use serde_json::json;

fn port_dep(port: u16, direction: &str, protocol_hint: &str) -> ShadowDep {
    let name = format!(":{port}");
    let id = format!("port::{port}::{direction}");
    let mut meta = Metadata::new();
    meta.insert("direction", json!(direction));
    if !protocol_hint.is_empty() {
        meta.insert("protocol_hint", json!(protocol_hint));
    }
    ShadowDep {
        id,
        category: Category::Port,
        name,
        confidence: Confidence::High,
        metadata: meta,
        locations: vec![Location::new("src/db.ts", 5, 1)],
    }
}

fn env_dep(name: &str) -> ShadowDep {
    let mut meta = Metadata::new();
    meta.insert("required", json!(true));
    ShadowDep::new(
        Category::EnvVar,
        name,
        Confidence::High,
        Location::new("src/config.ts", 10, 1),
    )
    .with_metadata(meta)
}

#[test]
fn empty_deps_produces_header() {
    let result = generate_compose(&[]);
    assert!(result.contains("No services detected"));
}

#[test]
fn redis_service_from_connect_port() {
    let deps = vec![port_dep(6379, "connect", "redis")];
    let result = generate_compose(&deps);
    assert!(result.contains("redis:"));
    assert!(result.contains("image: redis:7"));
    assert!(result.contains("\"6379:6379\""));
}

#[test]
fn postgres_service_with_env() {
    let deps = vec![port_dep(5432, "connect", "postgres")];
    let result = generate_compose(&deps);
    assert!(result.contains("image: postgres:16"));
    assert!(result.contains("POSTGRES_PASSWORD"));
}

#[test]
fn listen_port_goes_to_app_service() {
    let deps = vec![port_dep(3000, "listen", "http")];
    let result = generate_compose(&deps);
    assert!(result.contains("app:"));
    assert!(result.contains("\"3000:3000\""));
    assert!(!result.contains("image: http"));
}

#[test]
fn env_vars_in_app_service() {
    let deps = vec![env_dep("DATABASE_URL"), env_dep("API_KEY")];
    let result = generate_compose(&deps);
    assert!(result.contains("app:"));
    assert!(result.contains("DATABASE_URL"));
    assert!(result.contains("API_KEY"));
}

#[test]
fn app_depends_on_infra_services() {
    let deps = vec![
        port_dep(3000, "listen", "http"),
        port_dep(5432, "connect", "postgres"),
        port_dep(6379, "connect", "redis"),
    ];
    let result = generate_compose(&deps);
    assert!(result.contains("depends_on:"));
    assert!(result.contains("- db"));
    assert!(result.contains("- redis"));
}

#[test]
fn deduplicates_same_protocol() {
    let deps = vec![
        port_dep(5432, "connect", "postgres"),
        port_dep(5432, "connect", "postgres"),
    ];
    let result = generate_compose(&deps);
    let count = result.matches("image: postgres:16").count();
    assert_eq!(count, 1);
}

#[test]
fn unknown_protocol_ignored() {
    let deps = vec![port_dep(9999, "connect", "unknown_proto")];
    let result = generate_compose(&deps);
    assert!(result.contains("No services detected"));
}

#[test]
fn non_port_non_env_deps_ignored() {
    let binary_dep = ShadowDep::new(
        Category::Binary,
        "ffmpeg",
        Confidence::High,
        Location::new("src/video.ts", 5, 1),
    );
    let deps = vec![binary_dep];
    let result = generate_compose(&deps);
    assert!(result.contains("No services detected"));
}

#[test]
fn services_sorted_by_name() {
    let deps = vec![
        port_dep(6379, "connect", "redis"),
        port_dep(5432, "connect", "postgres"),
    ];
    let result = generate_compose(&deps);
    let db_pos = result.find("  db:").unwrap();
    let redis_pos = result.find("  redis:").unwrap();
    assert!(db_pos < redis_pos);
}
