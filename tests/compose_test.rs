use looper::infra::compose::{find_and_parse_compose, parse_compose_file};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

// --- Basic parsing ---

#[test]
fn parse_fixture_compose() {
    let result = parse_compose_file(Path::new("tests/fixtures/infra/docker-compose.yml")).unwrap();
    assert_eq!(result.services.len(), 4);
}

#[test]
fn extracts_service_names() {
    let result = parse_compose_file(Path::new("tests/fixtures/infra/docker-compose.yml")).unwrap();
    let names: Vec<&str> = result.services.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"app"));
    assert!(names.contains(&"db"));
    assert!(names.contains(&"redis"));
    assert!(names.contains(&"mongo"));
}

#[test]
fn extracts_images() {
    let result = parse_compose_file(Path::new("tests/fixtures/infra/docker-compose.yml")).unwrap();
    let db = result.services.iter().find(|s| s.name == "db").unwrap();
    assert_eq!(db.image, "postgres:16");

    let redis = result.services.iter().find(|s| s.name == "redis").unwrap();
    assert_eq!(redis.image, "redis:7-alpine");
}

#[test]
fn extracts_ports() {
    let result = parse_compose_file(Path::new("tests/fixtures/infra/docker-compose.yml")).unwrap();

    let app = result.services.iter().find(|s| s.name == "app").unwrap();
    assert_eq!(app.ports.len(), 1);
    assert_eq!(app.ports[0].host, 3000);
    assert_eq!(app.ports[0].container, 3000);

    let db = result.services.iter().find(|s| s.name == "db").unwrap();
    assert_eq!(db.ports[0].host, 5432);
    assert_eq!(db.ports[0].container, 5432);
}

#[test]
fn extracts_environment_list_format() {
    let result = parse_compose_file(Path::new("tests/fixtures/infra/docker-compose.yml")).unwrap();
    let app = result.services.iter().find(|s| s.name == "app").unwrap();

    assert!(app.environment.contains("DATABASE_URL"));
    assert!(app.environment.contains("REDIS_URL"));
    assert!(app.environment.contains("NODE_ENV"));
    assert!(app.environment.contains("JWT_SECRET"));
}

#[test]
fn extracts_environment_map_format() {
    let result = parse_compose_file(Path::new("tests/fixtures/infra/docker-compose.yml")).unwrap();
    let db = result.services.iter().find(|s| s.name == "db").unwrap();

    assert!(db.environment.contains("POSTGRES_DB"));
    assert!(db.environment.contains("POSTGRES_USER"));
    assert!(db.environment.contains("POSTGRES_PASSWORD"));
}

#[test]
fn extracts_volumes() {
    let result = parse_compose_file(Path::new("tests/fixtures/infra/docker-compose.yml")).unwrap();
    let app = result.services.iter().find(|s| s.name == "app").unwrap();
    assert!(app.volumes.iter().any(|v| v.contains("./src")));
}

// --- Auto-detection ---

#[test]
fn find_and_parse_detects_yml() {
    let result = find_and_parse_compose(Path::new("tests/fixtures/infra"), &mut Vec::new());
    assert!(!result.services.is_empty());
}

#[test]
fn find_and_parse_detects_yaml_extension() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("docker-compose.yaml"),
        "services:\n  web:\n    image: nginx\n",
    )
    .unwrap();

    let result = find_and_parse_compose(dir.path(), &mut Vec::new());
    assert_eq!(result.services.len(), 1);
    assert_eq!(result.services[0].name, "web");
}

#[test]
fn find_and_parse_returns_empty_when_missing() {
    let dir = TempDir::new().unwrap();
    let result = find_and_parse_compose(dir.path(), &mut Vec::new());
    assert!(result.services.is_empty());
}

// --- Edge cases ---

#[test]
fn missing_file_returns_error() {
    let result = parse_compose_file(Path::new("/tmp/nonexistent_compose_xyz.yml"));
    assert!(result.is_err());
}

#[test]
fn invalid_yaml_returns_error() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("docker-compose.yml");
    fs::write(&path, "not: [valid: yaml: {{").unwrap();

    let result = parse_compose_file(&path);
    assert!(result.is_err());
}

#[test]
fn service_without_image() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("docker-compose.yml");
    fs::write(&path, "services:\n  app:\n    build: .\n    ports:\n      - '3000:3000'\n").unwrap();

    let result = parse_compose_file(&path).unwrap();
    assert_eq!(result.services.len(), 1);
    assert_eq!(result.services[0].image, "");
    assert_eq!(result.services[0].ports[0].host, 3000);
}

#[test]
fn service_without_ports_or_env() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("docker-compose.yml");
    fs::write(&path, "services:\n  worker:\n    image: myapp:latest\n").unwrap();

    let result = parse_compose_file(&path).unwrap();
    assert_eq!(result.services.len(), 1);
    assert!(result.services[0].ports.is_empty());
    assert!(result.services[0].environment.is_empty());
}

#[test]
fn port_with_ip_binding() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("docker-compose.yml");
    fs::write(&path, "services:\n  db:\n    image: postgres\n    ports:\n      - '127.0.0.1:5432:5432'\n").unwrap();

    let result = parse_compose_file(&path).unwrap();
    assert_eq!(result.services[0].ports[0].host, 5432);
    assert_eq!(result.services[0].ports[0].container, 5432);
}

#[test]
fn sorted_services() {
    let result = parse_compose_file(Path::new("tests/fixtures/infra/docker-compose.yml")).unwrap();
    let names: Vec<&str> = result.services.iter().map(|s| s.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}

#[test]
fn source_file_tracked() {
    let result = parse_compose_file(Path::new("tests/fixtures/infra/docker-compose.yml")).unwrap();
    assert!(result.source_file.unwrap().contains("docker-compose.yml"));
}

#[test]
fn env_file_single_string() {
    let dir = tempfile::TempDir::new().unwrap();
    let compose = r#"
services:
  app:
    image: node:20
    env_file: .env.app
"#;
    std::fs::write(dir.path().join("docker-compose.yml"), compose).unwrap();
    let result = parse_compose_file(&dir.path().join("docker-compose.yml")).unwrap();
    assert_eq!(result.services[0].env_files, vec![".env.app"]);
}

#[test]
fn env_file_list() {
    let dir = tempfile::TempDir::new().unwrap();
    let compose = r#"
services:
  app:
    image: node:20
    env_file:
      - .env
      - .env.local
"#;
    std::fs::write(dir.path().join("docker-compose.yml"), compose).unwrap();
    let result = parse_compose_file(&dir.path().join("docker-compose.yml")).unwrap();
    assert_eq!(result.services[0].env_files, vec![".env", ".env.local"]);
}

#[test]
fn env_file_not_present() {
    let result = parse_compose_file(Path::new("tests/fixtures/infra/docker-compose.yml")).unwrap();
    for svc in &result.services {
        assert!(svc.env_files.is_empty(), "fixture compose has no env_file directives");
    }
}
