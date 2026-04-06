use colored::Colorize;
use std::path::PathBuf;

use crate::cli::DoctorArgs;
use crate::scan::{log_scan_errors, run_scan};

pub(crate) fn cmd_doctor(args: DoctorArgs) -> i32 {
    let deps = if let Some(ref manifest_path) = args.manifest {
        match looper::check::load_manifest(manifest_path) {
            Ok(m) => m.deps,
            Err(e) => {
                eprintln!("{} {e}", "error:".red());
                return 2;
            }
        }
    } else {
        let scan_paths = args.scan_paths.unwrap_or_else(|| vec![PathBuf::from(".")]);
        match run_scan(
            &scan_paths,
            false,
            Vec::new(),
            args.verbose,
            args.config.as_deref(),
        ) {
            Ok(s) => {
                log_scan_errors(&s.errors, args.verbose);
                s.deps
            }
            Err(code) => return code,
        }
    };

    if deps.is_empty() {
        eprintln!("{}", "No shadow dependencies found to check.".dimmed());
        return 2;
    }

    let scan_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let report = looper::doctor::run_doctor(&deps, args.timeout, &scan_root);

    match args.format.as_str() {
        "json" => {
            let json = match serde_json::to_string_pretty(&report) {
                Ok(j) => j,
                Err(e) => {
                    eprintln!("{} JSON serialization failed: {e}", "error:".red());
                    return 1;
                }
            };
            if let Err(e) = looper::output::write_output(&json, args.output.as_deref()) {
                eprintln!("{} {e}", "error:".red());
                return 1;
            }
        }
        _ => {
            print!("{}", format_doctor_terminal(&report));
        }
    }

    if args.fail_on_error && !report.all_passed() {
        return 1;
    }

    0
}

fn format_doctor_terminal(report: &looper::doctor::DoctorReport) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    writeln!(out, "{}", "looper doctor — runtime health check".bold()).unwrap();

    let mut current_category = String::new();
    for check in &report.checks {
        if check.category != current_category {
            current_category = check.category.clone();
            let label = match current_category.as_str() {
                "env_var" => "ENV VARS",
                "binary" => "BINARIES",
                "port" => "PORTS",
                "network" => "NETWORK",
                "filesystem" => "FILESYSTEM",
                _ => &current_category,
            };
            writeln!(out, "\n {}", label.bold()).unwrap();
        }

        let icon = if check.passed {
            "✓".green().to_string()
        } else {
            "✗".red().to_string()
        };

        let detail = if check.passed {
            check.detail.dimmed().to_string()
        } else {
            check.detail.red().to_string()
        };

        let mut line = format!("  {icon} {:<20} {detail}", check.name);

        if let Some(ref suggestion) = check.suggestion {
            if !check.passed {
                line.push_str(&format!(" — {}", suggestion.dimmed()));
            }
        }

        writeln!(out, "{line}").unwrap();
    }

    writeln!(
        out,
        "\n {} {} passed, {} failed",
        "Summary:".bold(),
        report.passed_count().to_string().green(),
        report.failed_count().to_string().red(),
    )
    .unwrap();

    out
}
