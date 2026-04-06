use colored::Colorize;
use std::path::PathBuf;

use crate::cli::DriftArgs;
use crate::scan::{log_scan_errors, run_scan};

pub(crate) fn cmd_drift(args: DriftArgs) -> i32 {
    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    if !looper::infra::has_infra_files(&project_root)
        && args.compose.is_none()
        && args.env.is_none()
        && args.dockerfile.is_none()
    {
        eprintln!(
            "{} no infrastructure files found (docker-compose.yml, .env, Dockerfile)",
            "error:".red()
        );
        return 2;
    }

    let scan_paths = args
        .scan_paths
        .unwrap_or_else(|| vec![PathBuf::from(".")]);

    let scan = match run_scan(&scan_paths, false, Vec::new(), args.verbose, args.config.as_deref()) {
        Ok(s) => s,
        Err(code) => return code,
    };
    log_scan_errors(&scan.errors, args.verbose);

    let mut infra = looper::infra::build_infra_manifest(&project_root);
    for w in &infra.warnings {
        eprintln!("  {} {w}", "warning:".yellow());
    }

    if let Some(ref compose_path) = args.compose {
        match looper::infra::compose::parse_compose_file(compose_path) {
            Ok(result) => {
                for svc in &result.services {
                    for pm in &svc.ports {
                        infra.exposed_ports.push(looper::infra::InfraPort {
                            port: pm.host,
                            source: format!("{} service {}", compose_path.display(), svc.name),
                        });
                    }
                    for env_name in &svc.environment {
                        infra.env_vars.entry(env_name.clone()).or_insert_with(|| {
                            format!("{} service {}", compose_path.display(), svc.name)
                        });
                    }
                }
            }
            Err(e) => eprintln!("  {} {e}", "warning:".yellow()),
        }
    }
    if let Some(ref env_path) = args.env {
        match looper::infra::envfile::parse_env_file(env_path) {
            Ok(result) => {
                for (name, entry) in &result.entries {
                    infra.env_vars.entry(name.clone()).or_insert_with(|| {
                        format!("{} line {}", entry.source_file, entry.line)
                    });
                }
            }
            Err(e) => eprintln!("  {} {e}", "warning:".yellow()),
        }
    }
    if let Some(ref df_path) = args.dockerfile {
        match looper::infra::dockerfile::parse_dockerfile(df_path) {
            Ok(result) => {
                infra.binaries.extend(result.binaries);
                for port in result.exposed_ports {
                    infra.exposed_ports.push(looper::infra::InfraPort {
                        port,
                        source: format!("{} EXPOSE", df_path.display()),
                    });
                }
            }
            Err(e) => eprintln!("  {} {e}", "warning:".yellow()),
        }
    }
    let report = looper::drift::compare(&scan.deps, &infra);

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
            let output = format_drift_terminal(&report);
            print!("{output}");
        }
    }

    if args.fail_on_missing && report.has_missing() {
        return 1;
    }

    0
}

fn format_drift_terminal(report: &looper::drift::DriftReport) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    writeln!(out, "{}", "looper drift — comparing code vs infrastructure".bold()).unwrap();

    if !report.missing.is_empty() {
        writeln!(
            out,
            "\n {} {} ({}):",
            "MISSING FROM INFRASTRUCTURE".red().bold(),
            "(code needs, infra doesn't provide)".dimmed(),
            report.missing.len()
        )
        .unwrap();
        writeln!(out, " {}", "──────────────────────────────────────────────────".dimmed()).unwrap();
        for item in &report.missing {
            let src = item.code_source.as_deref().unwrap_or("");
            writeln!(out, "  {:<10} {:<25} {}", item.category, item.name, src.dimmed()).unwrap();
        }
    }

    if !report.external.is_empty() {
        writeln!(
            out,
            "\n {} ({}):",
            "EXTERNAL".dimmed().bold(),
            report.external.len()
        )
        .unwrap();
        writeln!(out, " {}", "──────────────────────────────────────────────────".dimmed()).unwrap();
        for item in &report.external {
            writeln!(out, "  {:<10} {:<25} {}", item.category, item.name, "external API".dimmed()).unwrap();
        }
    }

    if !report.unused.is_empty() {
        writeln!(
            out,
            "\n {} {} ({}):",
            "UNUSED IN INFRASTRUCTURE".yellow().bold(),
            "(infra provides, code doesn't use)".dimmed(),
            report.unused.len()
        )
        .unwrap();
        writeln!(out, " {}", "──────────────────────────────────────────────────".dimmed()).unwrap();
        for item in &report.unused {
            let src = item.infra_source.as_deref().unwrap_or("");
            writeln!(out, "  {:<10} {:<25} {}", item.category, item.name, src.dimmed()).unwrap();
        }
    }

    if !report.covered.is_empty() {
        writeln!(
            out,
            "\n {} ({}):",
            "COVERED".green().bold(),
            report.covered.len()
        )
        .unwrap();
        writeln!(out, " {}", "──────────────────────────────────────────────────".dimmed()).unwrap();
        for item in &report.covered {
            let src = item.infra_source.as_deref().unwrap_or("");
            writeln!(out, "  {:<10} {:<25} {}", item.category, item.name, src.dimmed()).unwrap();
        }
    }

    writeln!(
        out,
        "\n {} {} missing, {} external, {} unused, {} covered",
        "Summary:".bold(),
        report.missing.len(),
        report.external.len(),
        report.unused.len(),
        report.covered.len(),
    )
    .unwrap();

    out
}
