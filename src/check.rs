use crate::types::{ScanManifest, ShadowDep};
use colored::Colorize;
use std::collections::HashSet;
use std::path::Path;

pub struct CheckResult {
    pub added: Vec<ShadowDep>,
    pub removed: Vec<ShadowDep>,
}

impl CheckResult {
    pub fn is_clean(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

/// Compare scan results against a declared manifest.
pub fn check_against_manifest(current_deps: &[ShadowDep], manifest: &ScanManifest) -> CheckResult {
    let manifest_ids: HashSet<&str> = manifest.deps.iter().map(|d| d.id.as_str()).collect();
    let current_ids: HashSet<&str> = current_deps.iter().map(|d| d.id.as_str()).collect();

    let added: Vec<ShadowDep> = current_deps
        .iter()
        .filter(|d| !manifest_ids.contains(d.id.as_str()))
        .cloned()
        .collect();

    let removed: Vec<ShadowDep> = manifest
        .deps
        .iter()
        .filter(|d| !current_ids.contains(d.id.as_str()))
        .cloned()
        .collect();

    CheckResult { added, removed }
}

/// Load a manifest from a JSON file.
pub fn load_manifest(path: &Path) -> Result<ScanManifest, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("could not read manifest {}: {e}", path.display()))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("invalid manifest JSON in {}: {e}", path.display()))
}

/// Print the check report to stderr.
pub fn print_check_report(result: &CheckResult) {
    if result.is_clean() {
        eprintln!("{}", "No undeclared shadow dependencies found.".green());
        return;
    }

    if !result.added.is_empty() {
        eprintln!(
            "{} {} undeclared shadow {} found\n",
            "error:".red().bold(),
            result.added.len(),
            if result.added.len() == 1 {
                "dependency"
            } else {
                "dependencies"
            },
        );
        for dep in &result.added {
            let loc = dep
                .locations
                .first()
                .map(|l| format!("({}:{})", l.file, l.line))
                .unwrap_or_default();
            eprintln!(
                "  {} {}: {}  {}",
                "+".green(),
                dep.category,
                dep.name,
                loc.dimmed()
            );
        }
        eprintln!();
    }

    if !result.removed.is_empty() {
        eprintln!(
            "{} {} declared {} no longer found\n",
            "warning:".yellow().bold(),
            result.removed.len(),
            if result.removed.len() == 1 {
                "dependency"
            } else {
                "dependencies"
            },
        );
        for dep in &result.removed {
            eprintln!("  {} {}: {}", "-".red(), dep.category, dep.name);
        }
        eprintln!();
    }

    eprintln!("Update the manifest:  {}", "looper check --update".bold());
}
