use crate::types::{ScanManifest, ShadowDep};
use colored::Colorize;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub struct ManifestDiff {
    pub added: Vec<ShadowDep>,
    pub removed: Vec<ShadowDep>,
    pub changed: Vec<ChangedDep>,
}

#[derive(Debug, Serialize)]
pub struct ChangedDep {
    pub id: String,
    pub name: String,
    pub old_locations_count: usize,
    pub new_locations_count: usize,
}

impl ManifestDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    pub fn total_changes(&self) -> usize {
        self.added.len() + self.removed.len() + self.changed.len()
    }
}

/// Compare two manifests and produce a diff.
pub fn diff_manifests(old: &ScanManifest, new: &ScanManifest) -> ManifestDiff {
    let old_map: HashMap<&str, &ShadowDep> = old.deps.iter().map(|d| (d.id.as_str(), d)).collect();
    let new_map: HashMap<&str, &ShadowDep> = new.deps.iter().map(|d| (d.id.as_str(), d)).collect();

    let added: Vec<ShadowDep> = new
        .deps
        .iter()
        .filter(|d| !old_map.contains_key(d.id.as_str()))
        .cloned()
        .collect();

    let removed: Vec<ShadowDep> = old
        .deps
        .iter()
        .filter(|d| !new_map.contains_key(d.id.as_str()))
        .cloned()
        .collect();

    let changed: Vec<ChangedDep> = new
        .deps
        .iter()
        .filter_map(|new_dep| {
            let old_dep = old_map.get(new_dep.id.as_str())?;
            if old_dep.locations.len() != new_dep.locations.len()
                || old_dep.confidence != new_dep.confidence
                || old_dep.metadata != new_dep.metadata
            {
                Some(ChangedDep {
                    id: new_dep.id.clone(),
                    name: new_dep.name.clone(),
                    old_locations_count: old_dep.locations.len(),
                    new_locations_count: new_dep.locations.len(),
                })
            } else {
                None
            }
        })
        .collect();

    ManifestDiff {
        added,
        removed,
        changed,
    }
}

/// Format the diff as colored terminal output.
pub fn format_diff_terminal(diff: &ManifestDiff) -> String {
    let mut out = String::new();

    if diff.is_empty() {
        out.push_str(&format!("{}\n", "No changes between manifests.".dimmed()));
        return out;
    }

    if !diff.added.is_empty() {
        out.push_str(&format!(
            "\n {} ({}):\n",
            "Added".green().bold(),
            diff.added.len()
        ));
        for dep in &diff.added {
            let loc = dep
                .locations
                .first()
                .map(|l| format!("({}:{})", l.file, l.line))
                .unwrap_or_default();
            out.push_str(&format!(
                "  {} {}: {}  {}\n",
                "+".green(),
                dep.category,
                dep.name,
                loc.dimmed()
            ));
        }
    }

    if !diff.removed.is_empty() {
        out.push_str(&format!(
            "\n {} ({}):\n",
            "Removed".red().bold(),
            diff.removed.len()
        ));
        for dep in &diff.removed {
            out.push_str(&format!("  {} {}: {}\n", "-".red(), dep.category, dep.name));
        }
    }

    if !diff.changed.is_empty() {
        out.push_str(&format!(
            "\n {} ({}):\n",
            "Changed".yellow().bold(),
            diff.changed.len()
        ));
        for dep in &diff.changed {
            out.push_str(&format!(
                "  {} {}: {} ({} -> {} locations)\n",
                "~".yellow(),
                dep.id,
                dep.name,
                dep.old_locations_count,
                dep.new_locations_count,
            ));
        }
    }

    out.push_str(&format!(
        "\n {} {} total changes\n",
        "Summary:".bold(),
        diff.total_changes()
    ));

    out
}

/// Format the diff as JSON.
pub fn format_diff_json(diff: &ManifestDiff) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(diff)
}
