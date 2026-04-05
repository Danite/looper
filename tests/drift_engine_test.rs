use looper::drift::compare;
use looper::infra::{InfraManifest, InfraPort, InfraService};
use looper::types::*;
use std::collections::HashSet;

fn make_infra(
    env_vars: &[(&str, &str)],
    binaries: &[&str],
    ports: &[(u16, &str)],
    services: &[(&str, &str)],
) -> InfraManifest {
    InfraManifest {
        env_vars: env_vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        binaries: binaries.iter().map(|s| s.to_string()).collect(),
        exposed_ports: ports
            .iter()
            .map(|(p, s)| InfraPort { port: *p, source: s.to_string() })
            .collect(),
        services: services
            .iter()
            .map(|(name, image)| InfraService {
                name: name.to_string(),
                image: image.to_string(),
                ports: vec![],
                environment: HashSet::new(),
                protocol_hint: None,
                default_port: None,
            })
            .collect(),
        volumes: vec!["./src".into(), "package.json".into()],
    }
}

fn env_dep(name: &str) -> ShadowDep {
    ShadowDep::new(Category::EnvVar, name, Confidence::High, Location::new("app.ts", 1, 1))
}

fn binary_dep(name: &str) -> ShadowDep {
    ShadowDep::new(Category::Binary, name, Confidence::High, Location::new("app.ts", 2, 1))
}

fn port_dep(port: u16) -> ShadowDep {
    let mut dep = ShadowDep::new(Category::Port, format!(":{port}"), Confidence::High, Location::new("app.ts", 3, 1));
    dep.id = format!("port::{port}::connect");
    dep
}

fn network_dep(host: &str) -> ShadowDep {
    ShadowDep::new(Category::Network, host, Confidence::High, Location::new("app.ts", 4, 1))
}

fn fs_dep(path: &str) -> ShadowDep {
    ShadowDep::new(Category::Filesystem, path, Confidence::High, Location::new("app.ts", 5, 1))
}

fn platform_dep() -> ShadowDep {
    ShadowDep::new(Category::Platform, "process.platform === \"darwin\"", Confidence::High, Location::new("app.ts", 6, 1))
}

// --- Env var comparison ---

#[test]
fn env_var_covered() {
    let infra = make_infra(&[("DB_URL", ".env line 1")], &[], &[], &[]);
    let deps = vec![env_dep("DB_URL")];
    let report = compare(&deps, &infra);

    assert_eq!(report.covered.len(), 1);
    assert_eq!(report.covered[0].name, "DB_URL");
    assert!(report.missing.is_empty());
}

#[test]
fn env_var_missing() {
    let infra = make_infra(&[], &[], &[], &[]);
    let deps = vec![env_dep("SECRET_KEY")];
    let report = compare(&deps, &infra);

    assert_eq!(report.missing.len(), 1);
    assert_eq!(report.missing[0].name, "SECRET_KEY");
}

#[test]
fn env_var_unused() {
    let infra = make_infra(&[("OLD_VAR", ".env line 5"), ("DB_URL", ".env line 1")], &[], &[], &[]);
    let deps = vec![env_dep("DB_URL")];
    let report = compare(&deps, &infra);

    assert_eq!(report.unused.len(), 1);
    assert_eq!(report.unused[0].name, "OLD_VAR");
}

// --- Binary comparison ---

#[test]
fn binary_covered() {
    let infra = make_infra(&[], &["ffmpeg", "git"], &[], &[]);
    let deps = vec![binary_dep("ffmpeg")];
    let report = compare(&deps, &infra);

    assert_eq!(report.covered.len(), 1);
    assert_eq!(report.covered[0].name, "ffmpeg");
}

#[test]
fn binary_missing() {
    let infra = make_infra(&[], &["git"], &[], &[]);
    let deps = vec![binary_dep("ffmpeg")];
    let report = compare(&deps, &infra);

    assert_eq!(report.missing.len(), 1);
    assert_eq!(report.missing[0].name, "ffmpeg");
}

// --- Port comparison ---

#[test]
fn port_covered() {
    let infra = make_infra(&[], &[], &[(6379, "compose redis")], &[]);
    let deps = vec![port_dep(6379)];
    let report = compare(&deps, &infra);

    assert_eq!(report.covered.len(), 1);
    assert!(report.covered[0].infra_source.as_ref().unwrap().contains("redis"));
}

#[test]
fn port_missing() {
    let infra = make_infra(&[], &[], &[], &[]);
    let deps = vec![port_dep(6379)];
    let report = compare(&deps, &infra);

    assert_eq!(report.missing.len(), 1);
    assert_eq!(report.missing[0].name, ":6379");
}

// --- Network comparison ---

#[test]
fn network_external_dot_com() {
    let infra = make_infra(&[], &[], &[], &[]);
    let deps = vec![network_dep("api.stripe.com")];
    let report = compare(&deps, &infra);

    assert_eq!(report.external.len(), 1);
    assert_eq!(report.external[0].name, "api.stripe.com");
    assert!(report.missing.is_empty());
}

#[test]
fn network_external_dot_io() {
    let infra = make_infra(&[], &[], &[], &[]);
    let deps = vec![network_dep("sentry.io")];
    let report = compare(&deps, &infra);

    assert_eq!(report.external.len(), 1);
}

#[test]
fn network_internal_bare_name_covered() {
    let infra = make_infra(&[], &[], &[], &[("redis", "redis:7")]);
    let deps = vec![network_dep("redis")];
    let report = compare(&deps, &infra);

    assert_eq!(report.covered.len(), 1);
    assert!(report.covered[0].infra_source.as_ref().unwrap().contains("redis"));
}

#[test]
fn network_internal_bare_name_missing() {
    let infra = make_infra(&[], &[], &[], &[]);
    let deps = vec![network_dep("cache")];
    let report = compare(&deps, &infra);

    assert_eq!(report.missing.len(), 1);
    assert_eq!(report.missing[0].name, "cache");
}

#[test]
fn network_localhost_is_not_external() {
    let infra = make_infra(&[], &[], &[], &[]);
    let deps = vec![network_dep("localhost")];
    let report = compare(&deps, &infra);

    // localhost has no dots, so internal, but not a compose service -> missing
    assert!(report.external.is_empty());
}

// --- Filesystem comparison ---

#[test]
fn filesystem_covered_by_volume() {
    let infra = make_infra(&[], &[], &[], &[]);
    let deps = vec![fs_dep("./src/config.json")];
    let report = compare(&deps, &infra);

    assert_eq!(report.covered.len(), 1);
}

#[test]
fn filesystem_missing() {
    let infra = make_infra(&[], &[], &[], &[]);
    let deps = vec![fs_dep("/etc/ssl/certs/ca.pem")];
    let report = compare(&deps, &infra);

    assert_eq!(report.missing.len(), 1);
}

// --- Platform skipped ---

#[test]
fn platform_skipped() {
    let infra = make_infra(&[], &[], &[], &[]);
    let deps = vec![platform_dep()];
    let report = compare(&deps, &infra);

    assert!(report.missing.is_empty());
    assert!(report.covered.is_empty());
    assert!(report.external.is_empty());
}

// --- Mixed ---

#[test]
fn mixed_report() {
    let infra = make_infra(
        &[("DB_URL", ".env"), ("OLD_KEY", ".env")],
        &["git"],
        &[(3000, "compose")],
        &[("redis", "redis:7")],
    );
    let deps = vec![
        env_dep("DB_URL"),
        env_dep("NEW_SECRET"),
        binary_dep("git"),
        binary_dep("ffmpeg"),
        port_dep(3000),
        port_dep(6379),
        network_dep("api.stripe.com"),
        network_dep("redis"),
        platform_dep(),
    ];
    let report = compare(&deps, &infra);

    assert!(!report.missing.is_empty());
    assert!(!report.external.is_empty());
    assert!(!report.unused.is_empty());
    assert!(!report.covered.is_empty());

    let missing_names: Vec<&str> = report.missing.iter().map(|d| d.name.as_str()).collect();
    assert!(missing_names.contains(&"NEW_SECRET"));
    assert!(missing_names.contains(&"ffmpeg"));
    assert!(missing_names.contains(&":6379"));

    let covered_names: Vec<&str> = report.covered.iter().map(|d| d.name.as_str()).collect();
    assert!(covered_names.contains(&"DB_URL"));
    assert!(covered_names.contains(&"git"));
    assert!(covered_names.contains(&":3000"));
    assert!(covered_names.contains(&"redis"));
}

// --- Report helpers ---

#[test]
fn has_missing() {
    let infra = make_infra(&[], &[], &[], &[]);
    let deps = vec![env_dep("X")];
    let report = compare(&deps, &infra);
    assert!(report.has_missing());
}

#[test]
fn no_missing() {
    let infra = make_infra(&[("X", ".env")], &[], &[], &[]);
    let deps = vec![env_dep("X")];
    let report = compare(&deps, &infra);
    assert!(!report.has_missing());
}

#[test]
fn dynamic_env_var_skipped() {
    let infra = make_infra(&[], &[], &[], &[]);
    let deps = vec![env_dep("<dynamic>"), env_dep("REAL_VAR")];
    let report = compare(&deps, &infra);

    let names: Vec<&str> = report.missing.iter().map(|d| d.name.as_str()).collect();
    assert!(!names.contains(&"<dynamic>"), "<dynamic> should be skipped in drift");
    assert!(names.contains(&"REAL_VAR"), "real env vars should still appear");
}

#[test]
fn sorted_output() {
    let infra = make_infra(&[], &[], &[], &[]);
    let deps = vec![env_dep("Z_VAR"), env_dep("A_VAR"), binary_dep("zzz")];
    let report = compare(&deps, &infra);

    let names: Vec<&str> = report.missing.iter().map(|d| d.name.as_str()).collect();
    // Sorted by category then name: binary first, then env_var
    assert_eq!(names[0], "zzz");
    assert_eq!(names[1], "A_VAR");
    assert_eq!(names[2], "Z_VAR");
}
