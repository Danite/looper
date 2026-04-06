use crate::types::{Category, Confidence, ShadowDep};
use colored::Colorize;
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;

/// Format scan results as a colored terminal report.
pub fn format_terminal(deps: &[ShadowDep], file_count: usize, duration_ms: u64) -> String {
    let mut out = String::new();

    writeln!(
        out,
        "{} {} — scanned {} files in {}ms",
        "looper".bold(),
        format!("v{}", env!("CARGO_PKG_VERSION")).dimmed(),
        file_count,
        duration_ms,
    )
    .unwrap();

    if deps.is_empty() {
        writeln!(out, "\n  {}", "No shadow dependencies found.".dimmed()).unwrap();
        return out;
    }

    let mut by_category: HashMap<Category, Vec<&ShadowDep>> = HashMap::new();
    for dep in deps {
        by_category.entry(dep.category).or_default().push(dep);
    }

    let mut total = 0;
    let mut categories_with_findings = 0;

    for category in Category::all() {
        let Some(cat_deps) = by_category.get(category) else {
            continue;
        };
        categories_with_findings += 1;
        total += cat_deps.len();

        writeln!(
            out,
            "\n {} ({} found)",
            category.label().bold(),
            cat_deps.len()
        )
        .unwrap();
        writeln!(
            out,
            " {}",
            "──────────────────────────────────────────────────".dimmed()
        )
        .unwrap();

        for dep in cat_deps {
            let loc = dep
                .locations
                .first()
                .map(|l| {
                    let base = format!("{}:{}", l.file, l.line);
                    let extra = dep.locations.len() - 1;
                    if extra > 0 {
                        format!("{base} (+{extra} more)")
                    } else {
                        base
                    }
                })
                .unwrap_or_default();
            let conf = format_confidence(dep.confidence);
            writeln!(out, "  {:<20} {:<25} {}", dep.name, loc.dimmed(), conf).unwrap();
        }
    }

    writeln!(
        out,
        "\n {} {} shadow deps across {} {}",
        "Summary:".bold(),
        total,
        categories_with_findings,
        if categories_with_findings == 1 {
            "category"
        } else {
            "categories"
        },
    )
    .unwrap();

    out
}

/// Format scan results as a single summary line.
pub fn format_summary(deps: &[ShadowDep], file_count: usize, duration_ms: u64) -> String {
    let mut by_category: HashMap<Category, usize> = HashMap::new();
    for dep in deps {
        *by_category.entry(dep.category).or_default() += 1;
    }

    format!(
        "looper v{} — {} files, {}ms — {} shadow deps across {} categories\n",
        env!("CARGO_PKG_VERSION"),
        file_count,
        duration_ms,
        deps.len(),
        by_category.len(),
    )
}

fn format_confidence(conf: Confidence) -> String {
    match conf {
        Confidence::High => "high".green().to_string(),
        Confidence::Medium => "medium".yellow().to_string(),
        Confidence::Low => "low".red().to_string(),
    }
}
