use crate::infra::InfraManifest;
use crate::types::{Category, ShadowDep};
use serde::Serialize;

const EXTERNAL_TLDS: &[&str] = &[
    ".com", ".io", ".org", ".net", ".dev", ".co", ".app", ".me", ".info", ".cloud",
];

const EXTERNAL_PATTERNS: &[&str] = &[
    ".amazonaws.com",
    ".googleapis.com",
    ".azure.com",
    ".cloudflare.com",
    ".sentry.io",
];

/// Result of comparing code scan against infrastructure.
#[derive(Debug, Serialize)]
pub struct DriftReport {
    pub missing: Vec<DriftItem>,
    pub external: Vec<DriftItem>,
    pub unused: Vec<DriftItem>,
    pub covered: Vec<DriftItem>,
}

impl DriftReport {
    pub fn has_missing(&self) -> bool {
        !self.missing.is_empty()
    }

    pub fn total(&self) -> usize {
        self.missing.len() + self.external.len() + self.unused.len() + self.covered.len()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DriftItem {
    pub category: String,
    pub name: String,
    pub code_source: Option<String>,
    pub infra_source: Option<String>,
}

/// Compare what code needs (scan) against what infrastructure provides (infra).
pub fn compare(code_deps: &[ShadowDep], infra: &InfraManifest) -> DriftReport {
    let mut missing = Vec::new();
    let mut external = Vec::new();
    let mut covered = Vec::new();

    let infra_env_names = infra.env_var_names();
    let infra_ports = infra.all_ports();
    let infra_service_names = infra.service_names();

    for dep in code_deps {
        match dep.category {
            Category::EnvVar => {
                let code_src = first_location(dep);
                if infra_env_names.contains(&dep.name) {
                    covered.push(DriftItem {
                        category: dep.category.to_string(),
                        name: dep.name.clone(),
                        code_source: Some(code_src),
                        infra_source: infra.env_vars.get(&dep.name).cloned(),
                    });
                } else {
                    missing.push(DriftItem {
                        category: dep.category.to_string(),
                        name: dep.name.clone(),
                        code_source: Some(code_src),
                        infra_source: None,
                    });
                }
            }

            Category::Binary => {
                let code_src = first_location(dep);
                if infra.binaries.contains(&dep.name) {
                    covered.push(DriftItem {
                        category: dep.category.to_string(),
                        name: dep.name.clone(),
                        code_source: Some(code_src),
                        infra_source: Some("Dockerfile".into()),
                    });
                } else {
                    missing.push(DriftItem {
                        category: dep.category.to_string(),
                        name: dep.name.clone(),
                        code_source: Some(code_src),
                        infra_source: None,
                    });
                }
            }

            Category::Port => {
                let code_src = first_location(dep);
                let port_num = extract_port_number(&dep.name);
                if let Some(port) = port_num {
                    if infra_ports.contains(&port) {
                        let source = infra
                            .exposed_ports
                            .iter()
                            .find(|p| p.port == port)
                            .map(|p| p.source.clone());
                        covered.push(DriftItem {
                            category: dep.category.to_string(),
                            name: dep.name.clone(),
                            code_source: Some(code_src),
                            infra_source: source,
                        });
                    } else {
                        missing.push(DriftItem {
                            category: dep.category.to_string(),
                            name: dep.name.clone(),
                            code_source: Some(code_src),
                            infra_source: None,
                        });
                    }
                }
            }

            Category::Network => {
                let code_src = first_location(dep);
                if is_external_host(&dep.name) {
                    external.push(DriftItem {
                        category: dep.category.to_string(),
                        name: dep.name.clone(),
                        code_source: Some(code_src),
                        infra_source: None,
                    });
                } else if infra_service_names.contains(&dep.name) {
                    covered.push(DriftItem {
                        category: dep.category.to_string(),
                        name: dep.name.clone(),
                        code_source: Some(code_src),
                        infra_source: Some(format!("docker-compose service {}", dep.name)),
                    });
                } else {
                    missing.push(DriftItem {
                        category: dep.category.to_string(),
                        name: dep.name.clone(),
                        code_source: Some(code_src),
                        infra_source: None,
                    });
                }
            }

            Category::Filesystem => {
                let code_src = first_location(dep);
                let is_covered = infra.volumes.iter().any(|v| {
                    dep.name.starts_with(v.as_str()) || v.starts_with(dep.name.as_str())
                });
                if is_covered {
                    covered.push(DriftItem {
                        category: dep.category.to_string(),
                        name: dep.name.clone(),
                        code_source: Some(code_src),
                        infra_source: Some("volume/COPY".into()),
                    });
                } else {
                    missing.push(DriftItem {
                        category: dep.category.to_string(),
                        name: dep.name.clone(),
                        code_source: Some(code_src),
                        infra_source: None,
                    });
                }
            }

            Category::Platform => {
                // Skipped -- no meaningful infra counterpart
            }
        }
    }

    // Find unused infra env vars (in infra but not referenced in code)
    let code_env_names: std::collections::HashSet<&str> = code_deps
        .iter()
        .filter(|d| d.category == Category::EnvVar)
        .map(|d| d.name.as_str())
        .collect();

    let mut unused = Vec::new();
    for (name, source) in &infra.env_vars {
        if !code_env_names.contains(name.as_str()) {
            unused.push(DriftItem {
                category: "env_var".into(),
                name: name.clone(),
                code_source: None,
                infra_source: Some(source.clone()),
            });
        }
    }
    unused.sort_by(|a, b| a.name.cmp(&b.name));

    // Sort for deterministic output
    missing.sort_by(|a, b| a.category.cmp(&b.category).then(a.name.cmp(&b.name)));
    external.sort_by(|a, b| a.name.cmp(&b.name));
    covered.sort_by(|a, b| a.category.cmp(&b.category).then(a.name.cmp(&b.name)));

    DriftReport {
        missing,
        external,
        unused,
        covered,
    }
}

fn first_location(dep: &ShadowDep) -> String {
    dep.locations
        .first()
        .map(|l| format!("{}:{}", l.file, l.line))
        .unwrap_or_default()
}

fn extract_port_number(name: &str) -> Option<u16> {
    // Port names look like ":3000" or ":6379"
    let s = name.strip_prefix(':')?;
    s.parse().ok()
}

fn is_external_host(host: &str) -> bool {
    // Bare name without dots = internal (docker-compose service name)
    if !host.contains('.') {
        return false;
    }

    // Known external patterns
    for pattern in EXTERNAL_PATTERNS {
        if host.ends_with(pattern) {
            return true;
        }
    }

    // Known TLDs
    for tld in EXTERNAL_TLDS {
        if host.ends_with(tld) {
            return true;
        }
    }

    // Hosts starting with "api." are likely external
    if host.starts_with("api.") {
        return true;
    }

    // Has a dot but doesn't match known TLDs -- could be internal (e.g., cache.local)
    // Default to internal for safety (won't flag as external unless certain)
    false
}
