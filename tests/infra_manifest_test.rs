use looper::infra::{build_infra_manifest, has_infra_files};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

// --- Fixture-based tests ---

#[test]
fn build_from_fixtures() {
    let manifest = build_infra_manifest(Path::new("tests/fixtures/infra"));

    assert!(!manifest.env_vars.is_empty());
    assert!(!manifest.services.is_empty());
    assert!(!manifest.binaries.is_empty());
    assert!(!manifest.exposed_ports.is_empty());
}

#[test]
fn env_vars_from_dotenv_and_compose() {
    let manifest = build_infra_manifest(Path::new("tests/fixtures/infra"));
    let names = manifest.env_var_names();

    // From .env
    assert!(names.contains("DATABASE_URL"));
    assert!(names.contains("STRIPE_SECRET_KEY"));
    assert!(names.contains("PORT"));

    // From compose service environment
    assert!(names.contains("POSTGRES_DB"));
    assert!(names.contains("POSTGRES_USER"));
}

#[test]
fn services_from_compose() {
    let manifest = build_infra_manifest(Path::new("tests/fixtures/infra"));
    let service_names = manifest.service_names();

    assert!(service_names.contains("app"));
    assert!(service_names.contains("db"));
    assert!(service_names.contains("redis"));
    assert!(service_names.contains("mongo"));
}

#[test]
fn service_image_resolution() {
    let manifest = build_infra_manifest(Path::new("tests/fixtures/infra"));

    let redis = manifest
        .services
        .iter()
        .find(|s| s.name == "redis")
        .unwrap();
    assert_eq!(redis.protocol_hint.as_deref(), Some("redis"));
    assert_eq!(redis.default_port, Some(6379));

    let db = manifest.services.iter().find(|s| s.name == "db").unwrap();
    assert_eq!(db.protocol_hint.as_deref(), Some("postgres"));
    assert_eq!(db.default_port, Some(5432));
}

#[test]
fn binaries_from_dockerfile() {
    let manifest = build_infra_manifest(Path::new("tests/fixtures/infra"));

    assert!(manifest.binaries.contains("node"));
    assert!(manifest.binaries.contains("ffmpeg"));
    assert!(manifest.binaries.contains("convert"));
    assert!(manifest.binaries.contains("git"));
    assert!(manifest.binaries.contains("curl"));
}

#[test]
fn ports_from_compose_and_dockerfile() {
    let manifest = build_infra_manifest(Path::new("tests/fixtures/infra"));
    let ports = manifest.all_ports();

    assert!(ports.contains(&3000));
    assert!(ports.contains(&5432));
    assert!(ports.contains(&6379));
    assert!(ports.contains(&27017));
    assert!(ports.contains(&8080)); // from Dockerfile EXPOSE
}

#[test]
fn env_var_source_tracking() {
    let manifest = build_infra_manifest(Path::new("tests/fixtures/infra"));

    let db_source = manifest.env_vars.get("DATABASE_URL").unwrap();
    assert!(db_source.contains(".env"));
    assert!(db_source.contains("line"));
}

// --- has_infra_files ---

#[test]
fn has_infra_files_with_fixtures() {
    assert!(has_infra_files(Path::new("tests/fixtures/infra")));
}

#[test]
fn has_infra_files_empty_dir() {
    let dir = TempDir::new().unwrap();
    assert!(!has_infra_files(dir.path()));
}

#[test]
fn has_infra_files_only_env() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".env"), "VAR=1").unwrap();
    assert!(has_infra_files(dir.path()));
}

// --- Empty project ---

#[test]
fn empty_project_returns_empty_manifest() {
    let dir = TempDir::new().unwrap();
    let manifest = build_infra_manifest(dir.path());

    assert!(manifest.env_vars.is_empty());
    assert!(manifest.services.is_empty());
    assert!(manifest.binaries.is_empty());
    assert!(manifest.exposed_ports.is_empty());
}

// --- Partial infra ---

#[test]
fn only_env_file() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join(".env"),
        "DB_URL=postgres://localhost\nAPI_KEY=secret\n",
    )
    .unwrap();

    let manifest = build_infra_manifest(dir.path());
    assert_eq!(manifest.env_vars.len(), 2);
    assert!(manifest.services.is_empty());
    assert!(manifest.binaries.is_empty());
}

#[test]
fn only_compose_file() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("docker-compose.yml"),
        "services:\n  redis:\n    image: redis:7\n    ports:\n      - '6379:6379'\n",
    )
    .unwrap();

    let manifest = build_infra_manifest(dir.path());
    assert_eq!(manifest.services.len(), 1);
    assert_eq!(manifest.services[0].name, "redis");
    assert!(manifest.all_ports().contains(&6379));
}

#[test]
fn only_dockerfile() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("Dockerfile"),
        "FROM node:20\nRUN apt-get install -y git\nEXPOSE 3000\n",
    )
    .unwrap();

    let manifest = build_infra_manifest(dir.path());
    assert!(manifest.binaries.contains("node"));
    assert!(manifest.binaries.contains("git"));
    assert!(manifest.all_ports().contains(&3000));
}

#[test]
fn service_without_ports_gets_default() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("docker-compose.yml"),
        "services:\n  cache:\n    image: redis:7\n",
    )
    .unwrap();

    let manifest = build_infra_manifest(dir.path());
    // Redis has no explicit ports but default should be added
    assert!(manifest.all_ports().contains(&6379));
}
