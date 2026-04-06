pub mod compose;
pub mod dockerfile;
pub mod envfile;
pub mod prisma;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Image name prefix -> (protocol hint, default port).
const KNOWN_IMAGES: &[(&str, &str, u16)] = &[
    ("redis", "redis", 6379),
    ("postgres", "postgres", 5432),
    ("mysql", "mysql", 3306),
    ("mariadb", "mysql", 3306),
    ("mongo", "mongodb", 27017),
    ("rabbitmq", "amqp", 5672),
    ("nats", "nats", 4222),
    ("memcached", "memcached", 11211),
    ("elasticsearch", "elasticsearch", 9200),
    ("opensearchproject/opensearch", "elasticsearch", 9200),
    ("minio", "s3", 9000),
];

/// Unified view of what the infrastructure provides.
#[derive(Debug, Default)]
pub struct InfraManifest {
    pub env_vars: HashMap<String, String>,
    pub services: Vec<InfraService>,
    pub binaries: HashSet<String>,
    pub exposed_ports: Vec<InfraPort>,
    pub volumes: Vec<String>,
    pub warnings: Vec<String>,
}

/// A service provided by infrastructure (from docker-compose).
#[derive(Debug, Clone)]
pub struct InfraService {
    pub name: String,
    pub image: String,
    pub ports: Vec<(u16, u16)>,
    pub environment: HashSet<String>,
    pub protocol_hint: Option<String>,
    pub default_port: Option<u16>,
}

/// A port exposed by infrastructure.
#[derive(Debug, Clone)]
pub struct InfraPort {
    pub port: u16,
    pub source: String,
}

impl InfraManifest {
    pub fn env_var_names(&self) -> HashSet<String> {
        self.env_vars.keys().cloned().collect()
    }

    pub fn all_ports(&self) -> HashSet<u16> {
        self.exposed_ports.iter().map(|p| p.port).collect()
    }

    pub fn service_names(&self) -> HashSet<String> {
        self.services.iter().map(|s| s.name.clone()).collect()
    }
}

/// Build a unified InfraManifest from all available infra files in a project.
pub fn build_infra_manifest(project_root: &Path) -> InfraManifest {
    let mut manifest = InfraManifest::default();

    // 1. Parse .env files
    let env_result = envfile::find_and_parse_env(project_root, &mut manifest.warnings);
    for (name, entry) in &env_result.entries {
        let source = format!("{} line {}", entry.source_file, entry.line);
        manifest.env_vars.insert(name.clone(), source);
    }

    // 2. Parse docker-compose
    let compose_result = compose::find_and_parse_compose(project_root, &mut manifest.warnings);
    let compose_source = compose_result
        .source_file
        .as_deref()
        .unwrap_or("docker-compose.yml");

    for svc in &compose_result.services {
        let (protocol_hint, default_port) = resolve_image(&svc.image);

        let infra_svc = InfraService {
            name: svc.name.clone(),
            image: svc.image.clone(),
            ports: svc.ports.iter().map(|p| (p.host, p.container)).collect(),
            environment: svc.environment.clone(),
            protocol_hint: protocol_hint.map(String::from),
            default_port,
        };

        // Add compose service ports to exposed_ports
        for pm in &svc.ports {
            manifest.exposed_ports.push(InfraPort {
                port: pm.host,
                source: format!("{compose_source} service {}", svc.name),
            });
        }

        // If service has a known image but no explicit ports, add default port
        if svc.ports.is_empty() {
            if let Some(dp) = default_port {
                manifest.exposed_ports.push(InfraPort {
                    port: dp,
                    source: format!("{compose_source} service {} (default)", svc.name),
                });
            }
        }

        // Merge compose service env vars
        for env_name in &svc.environment {
            if !manifest.env_vars.contains_key(env_name) {
                manifest.env_vars.insert(
                    env_name.clone(),
                    format!("{compose_source} service {}", svc.name),
                );
            }
        }

        // Merge env vars from compose env_file directives
        for env_file_path in &svc.env_files {
            let resolved = if Path::new(env_file_path).is_absolute() {
                PathBuf::from(env_file_path)
            } else {
                project_root.join(env_file_path)
            };
            if let Ok(result) = envfile::parse_env_file(&resolved) {
                for (name, entry) in result.entries {
                    manifest.env_vars.entry(name).or_insert_with(|| {
                        format!(
                            "{} line {} (via env_file in {})",
                            entry.source_file, entry.line, svc.name
                        )
                    });
                }
            }
        }

        manifest.services.push(infra_svc);
    }

    // Add compose volumes
    for svc in &compose_result.services {
        manifest.volumes.extend(
            svc.volumes
                .iter()
                .filter(|v| v.starts_with('.') || v.starts_with('/'))
                .cloned(),
        );
    }

    // 3. Parse Dockerfile
    let docker_result = dockerfile::find_and_parse_dockerfile(project_root, &mut manifest.warnings);
    manifest.binaries.extend(docker_result.binaries);

    let docker_source = docker_result.source_file.as_deref().unwrap_or("Dockerfile");

    for port in docker_result.exposed_ports {
        manifest.exposed_ports.push(InfraPort {
            port,
            source: format!("{docker_source} EXPOSE"),
        });
    }

    manifest.volumes.extend(docker_result.copy_sources);

    // 4. Parse Prisma schema
    let prisma_result = prisma::find_and_parse_prisma(project_root, &mut manifest.warnings);
    if let Some(ref prisma_source) = prisma_result.source_file {
        for (name, entry) in &prisma_result.entries {
            manifest
                .env_vars
                .entry(name.clone())
                .or_insert_with(|| format!("{prisma_source} line {}", entry.line));
        }
    }

    manifest
}

/// Match image name against KNOWN_IMAGES table.
fn resolve_image(image: &str) -> (Option<&'static str>, Option<u16>) {
    let image_name = image.split(':').next().unwrap_or(image);
    let short_name = image_name.rsplit('/').next().unwrap_or(image_name);

    for (prefix, protocol, port) in KNOWN_IMAGES {
        if short_name.starts_with(prefix) {
            return (Some(protocol), Some(*port));
        }
    }
    (None, None)
}

/// Check if any infra files exist in the project root.
pub fn has_infra_files(project_root: &Path) -> bool {
    let candidates = [
        ".env",
        ".env.example",
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        "compose.yaml",
        "Dockerfile",
        "dockerfile",
        "prisma/schema.prisma",
        "schema.prisma",
    ];
    candidates.iter().any(|f| project_root.join(f).exists())
}
