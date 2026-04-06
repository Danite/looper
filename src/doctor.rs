use crate::infra::dockerfile::PACKAGE_TO_BINARIES;
use crate::types::{Category, ShadowDep};
use serde::Serialize;
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::time::Duration;

const SECRET_KEYWORDS: &[&str] = &["SECRET", "KEY", "TOKEN", "PASSWORD", "CREDENTIAL", "AUTH"];

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorCheck {
    pub category: String,
    pub name: String,
    pub passed: bool,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

impl DoctorReport {
    pub fn passed_count(&self) -> usize {
        self.checks.iter().filter(|c| c.passed).count()
    }

    pub fn failed_count(&self) -> usize {
        self.checks.iter().filter(|c| !c.passed).count()
    }

    pub fn all_passed(&self) -> bool {
        self.checks.iter().all(|c| c.passed)
    }
}

/// Run all runtime checks against shadow deps.
pub fn run_doctor(deps: &[ShadowDep], timeout_ms: u64, scan_root: &Path) -> DoctorReport {
    let timeout = Duration::from_millis(timeout_ms);
    let mut checks = Vec::new();

    for dep in deps {
        match dep.category {
            Category::EnvVar => {
                if dep.name == "<dynamic>" {
                    continue;
                }
                checks.push(check_env_var(&dep.name, &dep.metadata));
            }
            Category::Binary => {
                checks.push(check_binary(&dep.name));
            }
            Category::Port => {
                let direction = dep.metadata.get_str("direction").unwrap_or("connect");
                if let Some(port) = extract_port(&dep.name) {
                    checks.push(check_port(port, direction, &dep.name, timeout));
                }
            }
            Category::Network => {
                checks.push(check_network(&dep.name, &dep.metadata, timeout));
            }
            Category::Filesystem => {
                checks.push(check_filesystem(&dep.name, scan_root));
            }
            Category::Platform => {}
        }
    }

    DoctorReport { checks }
}

fn check_env_var(name: &str, metadata: &crate::types::Metadata) -> DoctorCheck {
    let is_required = metadata.get_bool("required").unwrap_or(true);
    let default_value = metadata.get_str("default_value");

    match std::env::var(name) {
        Ok(val) if !val.is_empty() => {
            let display = if is_secret_name(name) {
                "set (*****)".to_string()
            } else if val.len() > 30 {
                format!("set ({}...)", &val[..27])
            } else {
                format!("set ({val})")
            };
            DoctorCheck {
                category: "env_var".into(),
                name: name.into(),
                passed: true,
                detail: display,
                suggestion: None,
            }
        }
        _ => {
            if !is_required {
                let detail = match default_value {
                    Some(d) => format!("optional (default: {d})"),
                    None => "optional".into(),
                };
                DoctorCheck {
                    category: "env_var".into(),
                    name: name.into(),
                    passed: true,
                    detail,
                    suggestion: None,
                }
            } else {
                DoctorCheck {
                    category: "env_var".into(),
                    name: name.into(),
                    passed: false,
                    detail: "missing".into(),
                    suggestion: Some(format!("export {name}=<value>")),
                }
            }
        }
    }
}

fn check_binary(name: &str) -> DoctorCheck {
    match which::which(name) {
        Ok(path) => DoctorCheck {
            category: "binary".into(),
            name: name.into(),
            passed: true,
            detail: path.to_string_lossy().into_owned(),
            suggestion: None,
        },
        Err(_) => {
            let suggestion = suggest_install(name);
            DoctorCheck {
                category: "binary".into(),
                name: name.into(),
                passed: false,
                detail: "not found".into(),
                suggestion,
            }
        }
    }
}

fn check_port(port: u16, direction: &str, display_name: &str, timeout: Duration) -> DoctorCheck {
    if direction == "listen" {
        match TcpListener::bind(format!("127.0.0.1:{port}")) {
            Ok(_listener) => DoctorCheck {
                category: "port".into(),
                name: display_name.into(),
                passed: true,
                detail: "available".into(),
                suggestion: None,
            },
            Err(_) => DoctorCheck {
                category: "port".into(),
                name: display_name.into(),
                passed: false,
                detail: "in use".into(),
                suggestion: Some(format!("port {port} is already bound by another process")),
            },
        }
    } else {
        let addr = format!("127.0.0.1:{port}");
        match addr.parse::<std::net::SocketAddr>() {
            Ok(sock) => match TcpStream::connect_timeout(&sock, timeout) {
                Ok(_) => DoctorCheck {
                    category: "port".into(),
                    name: display_name.into(),
                    passed: true,
                    detail: "reachable".into(),
                    suggestion: None,
                },
                Err(_) => DoctorCheck {
                    category: "port".into(),
                    name: display_name.into(),
                    passed: false,
                    detail: "connection refused".into(),
                    suggestion: Some(format!("start the service listening on port {port}")),
                },
            },
            Err(_) => DoctorCheck {
                category: "port".into(),
                name: display_name.into(),
                passed: false,
                detail: "invalid address".into(),
                suggestion: None,
            },
        }
    }
}

fn resolve_network_port(host: &str, metadata: &crate::types::Metadata) -> u16 {
    if let Some(idx) = host.rfind(':') {
        if let Ok(port) = host[idx + 1..].parse::<u16>() {
            return port;
        }
    }
    match metadata.get_str("protocol").unwrap_or("https") {
        "http" | "ws" => 80,
        _ => 443,
    }
}

fn check_network(host: &str, metadata: &crate::types::Metadata, timeout: Duration) -> DoctorCheck {
    let port = resolve_network_port(host, metadata);
    let clean_host = host
        .rfind(':')
        .and_then(|i| host[i + 1..].parse::<u16>().ok().map(|_| &host[..i]))
        .unwrap_or(host);
    let lookup = format!("{clean_host}:{port}");
    match lookup.to_socket_addrs() {
        Ok(mut addrs) => {
            if let Some(addr) = addrs.next() {
                match TcpStream::connect_timeout(&addr, timeout) {
                    Ok(_) => DoctorCheck {
                        category: "network".into(),
                        name: host.into(),
                        passed: true,
                        detail: "reachable".into(),
                        suggestion: None,
                    },
                    Err(_) => DoctorCheck {
                        category: "network".into(),
                        name: host.into(),
                        passed: false,
                        detail: "DNS resolves but connection refused".into(),
                        suggestion: Some(format!(
                            "host {clean_host} resolves but port {port} is not reachable"
                        )),
                    },
                }
            } else {
                DoctorCheck {
                    category: "network".into(),
                    name: host.into(),
                    passed: false,
                    detail: "DNS resolution failed".into(),
                    suggestion: Some(format!("check DNS for {host}")),
                }
            }
        }
        Err(_) => DoctorCheck {
            category: "network".into(),
            name: host.into(),
            passed: false,
            detail: "DNS resolution failed".into(),
            suggestion: Some(format!("check DNS or network connectivity for {host}")),
        },
    }
}

fn check_filesystem(path_str: &str, scan_root: &Path) -> DoctorCheck {
    let resolved = if Path::new(path_str).is_absolute() {
        Path::new(path_str).to_path_buf()
    } else {
        scan_root.join(path_str)
    };

    if resolved.exists() {
        DoctorCheck {
            category: "filesystem".into(),
            name: path_str.into(),
            passed: true,
            detail: "exists".into(),
            suggestion: None,
        }
    } else {
        DoctorCheck {
            category: "filesystem".into(),
            name: path_str.into(),
            passed: false,
            detail: "missing".into(),
            suggestion: Some(format!("create {path_str}")),
        }
    }
}

fn is_secret_name(name: &str) -> bool {
    let upper = name.to_uppercase();
    SECRET_KEYWORDS.iter().any(|kw| upper.contains(kw))
}

fn extract_port(name: &str) -> Option<u16> {
    name.strip_prefix(':')?.parse().ok()
}

fn detect_install_command() -> &'static str {
    if cfg!(target_os = "macos") {
        return "brew install";
    }

    if which::which("apt-get").is_ok() {
        "apt-get install"
    } else if which::which("apk").is_ok() {
        "apk add"
    } else if which::which("dnf").is_ok() {
        "dnf install"
    } else if which::which("yum").is_ok() {
        "yum install"
    } else if which::which("pacman").is_ok() {
        "pacman -S"
    } else {
        "install"
    }
}

fn suggest_install(binary: &str) -> Option<String> {
    for (pkg, bins) in PACKAGE_TO_BINARIES {
        if bins.contains(&binary) {
            let cmd = detect_install_command();
            return Some(format!("{cmd} {pkg}"));
        }
    }
    None
}
