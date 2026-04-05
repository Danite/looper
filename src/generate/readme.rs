use crate::infra::dockerfile::PACKAGE_TO_BINARIES;
use crate::types::{Category, ShadowDep};
use std::fmt::Write;

/// Generate a markdown Requirements section from all shadow dep categories.
pub fn generate_readme(deps: &[ShadowDep]) -> String {
    if deps.is_empty() {
        return String::from("# Requirements\n\nNo shadow dependencies detected.\n");
    }

    let mut out = String::new();
    writeln!(out, "# Requirements\n").unwrap();
    writeln!(out, "Shadow dependencies detected by [looper](https://github.com/example/looper).\n").unwrap();

    write_env_section(&mut out, deps);
    write_binary_section(&mut out, deps);
    write_port_section(&mut out, deps);
    write_network_section(&mut out, deps);
    write_filesystem_section(&mut out, deps);

    out
}

fn write_env_section(out: &mut String, deps: &[ShadowDep]) {
    let env_deps: Vec<&ShadowDep> = deps.iter().filter(|d| d.category == Category::EnvVar).collect();
    if env_deps.is_empty() {
        return;
    }

    writeln!(out, "## Environment Variables\n").unwrap();
    writeln!(out, "| Variable | Required | Default |").unwrap();
    writeln!(out, "|----------|----------|---------|").unwrap();

    for dep in &env_deps {
        let required = dep.metadata.get_bool("required").unwrap_or(true);
        let default_val = dep.metadata.get_str("default_value").unwrap_or("-");
        let req_str = if required { "Yes" } else { "No" };
        writeln!(out, "| `{}` | {} | {} |", dep.name, req_str, default_val).unwrap();
    }
    writeln!(out).unwrap();
}

fn write_binary_section(out: &mut String, deps: &[ShadowDep]) {
    let bin_deps: Vec<&ShadowDep> = deps.iter().filter(|d| d.category == Category::Binary).collect();
    if bin_deps.is_empty() {
        return;
    }

    writeln!(out, "## System Binaries\n").unwrap();
    writeln!(out, "| Binary | Install |").unwrap();
    writeln!(out, "|--------|---------|").unwrap();

    for dep in &bin_deps {
        let install = suggest_install(&dep.name).unwrap_or_else(|| "-".to_string());
        writeln!(out, "| `{}` | `{}` |", dep.name, install).unwrap();
    }
    writeln!(out).unwrap();
}

fn write_port_section(out: &mut String, deps: &[ShadowDep]) {
    let port_deps: Vec<&ShadowDep> = deps.iter().filter(|d| d.category == Category::Port).collect();
    if port_deps.is_empty() {
        return;
    }

    writeln!(out, "## Ports\n").unwrap();
    writeln!(out, "| Port | Direction | Protocol |").unwrap();
    writeln!(out, "|------|-----------|----------|").unwrap();

    for dep in &port_deps {
        let direction = dep.metadata.get_str("direction").unwrap_or("-");
        let protocol = dep.metadata.get_str("protocol_hint").unwrap_or("-");
        writeln!(out, "| `{}` | {} | {} |", dep.name, direction, protocol).unwrap();
    }
    writeln!(out).unwrap();
}

fn write_network_section(out: &mut String, deps: &[ShadowDep]) {
    let net_deps: Vec<&ShadowDep> = deps.iter().filter(|d| d.category == Category::Network).collect();
    if net_deps.is_empty() {
        return;
    }

    writeln!(out, "## Network Endpoints\n").unwrap();
    for dep in &net_deps {
        let protocol = dep.metadata.get_str("protocol").unwrap_or("https");
        writeln!(out, "- `{protocol}://{}`", dep.name).unwrap();
    }
    writeln!(out).unwrap();
}

fn write_filesystem_section(out: &mut String, deps: &[ShadowDep]) {
    let fs_deps: Vec<&ShadowDep> = deps.iter().filter(|d| d.category == Category::Filesystem).collect();
    if fs_deps.is_empty() {
        return;
    }

    writeln!(out, "## Filesystem Paths\n").unwrap();
    for dep in &fs_deps {
        let op = dep.metadata.get_str("operation").unwrap_or("access");
        writeln!(out, "- `{}` ({})", dep.name, op).unwrap();
    }
    writeln!(out).unwrap();
}

fn suggest_install(binary: &str) -> Option<String> {
    for (pkg, bins) in PACKAGE_TO_BINARIES {
        if bins.contains(&binary) {
            return Some(format!("apt-get install {pkg}"));
        }
    }
    None
}
