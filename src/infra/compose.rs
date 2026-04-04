use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// A parsed docker-compose service.
#[derive(Debug, Clone)]
pub struct ComposeService {
    pub name: String,
    pub image: String,
    pub ports: Vec<PortMapping>,
    pub environment: HashSet<String>,
    pub volumes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PortMapping {
    pub host: u16,
    pub container: u16,
}

/// Result of parsing a docker-compose file.
#[derive(Debug, Default)]
pub struct ComposeResult {
    pub services: Vec<ComposeService>,
    pub source_file: Option<String>,
}

/// Parse a docker-compose.yml file.
pub fn parse_compose_file(path: &Path) -> Result<ComposeResult, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;

    let raw: RawCompose = serde_yaml::from_str(&content)
        .map_err(|e| format!("invalid YAML in {}: {e}", path.display()))?;

    let source_file = path.to_string_lossy().into_owned();
    let mut services = Vec::new();

    if let Some(raw_services) = raw.services {
        for (name, svc) in raw_services {
            services.push(ComposeService {
                name,
                image: svc.image.unwrap_or_default(),
                ports: parse_ports(&svc.ports.unwrap_or_default()),
                environment: parse_environment(&svc.environment),
                volumes: parse_volumes(&svc.volumes.unwrap_or_default()),
            });
        }
    }

    services.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(ComposeResult {
        services,
        source_file: Some(source_file),
    })
}

/// Auto-detect and parse docker-compose.yml or docker-compose.yaml.
pub fn find_and_parse_compose(project_root: &Path) -> ComposeResult {
    for filename in &["docker-compose.yml", "docker-compose.yaml", "compose.yml", "compose.yaml"] {
        let path = project_root.join(filename);
        if path.exists() {
            if let Ok(result) = parse_compose_file(&path) {
                return result;
            }
        }
    }
    ComposeResult::default()
}

// --- Raw YAML deserialization types ---

#[derive(Deserialize)]
struct RawCompose {
    services: Option<HashMap<String, RawService>>,
}

#[derive(Deserialize, Default)]
struct RawService {
    image: Option<String>,
    ports: Option<Vec<String>>,
    environment: Option<RawEnvironment>,
    volumes: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawEnvironment {
    Map(HashMap<String, serde_yaml::Value>),
    List(Vec<String>),
}

fn parse_ports(raw: &[String]) -> Vec<PortMapping> {
    raw.iter()
        .filter_map(|s| {
            // Formats: "8080:80", "3000:3000", "127.0.0.1:5432:5432"
            let parts: Vec<&str> = s.split(':').collect();
            match parts.len() {
                2 => {
                    let host = parts[0].parse().ok()?;
                    let container = parts[1].split('/').next()?.parse().ok()?;
                    Some(PortMapping { host, container })
                }
                3 => {
                    let host = parts[1].parse().ok()?;
                    let container = parts[2].split('/').next()?.parse().ok()?;
                    Some(PortMapping { host, container })
                }
                _ => None,
            }
        })
        .collect()
}

fn parse_environment(raw: &Option<RawEnvironment>) -> HashSet<String> {
    match raw {
        Some(RawEnvironment::Map(map)) => map.keys().cloned().collect(),
        Some(RawEnvironment::List(list)) => list
            .iter()
            .filter_map(|entry| {
                let name = entry.split('=').next()?;
                if name.is_empty() {
                    None
                } else {
                    Some(name.to_string())
                }
            })
            .collect(),
        None => HashSet::new(),
    }
}

fn parse_volumes(raw: &[String]) -> Vec<String> {
    raw.iter()
        .map(|v| {
            // Format: "./host/path:/container/path" or "named_vol:/path"
            let parts: Vec<&str> = v.splitn(2, ':').collect();
            if parts.len() == 2 {
                parts[0].to_string()
            } else {
                v.clone()
            }
        })
        .collect()
}
