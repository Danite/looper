use colored::Colorize;
use std::path::PathBuf;

use crate::cli::GenerateArgs;
use crate::scan::{log_scan_errors, run_scan};

pub(crate) fn cmd_generate(args: GenerateArgs) -> i32 {
    let targets = match args.target.as_str() {
        "all" => looper::generate::GenerateTarget::all().to_vec(),
        other => match looper::generate::GenerateTarget::parse(other) {
            Some(t) => vec![t],
            None => {
                eprintln!(
                    "{} unknown generate target '{other}'. Valid: env, compose, dockerfile, readme, all",
                    "error:".red()
                );
                return 1;
            }
        },
    };

    let scan_paths = args.scan_paths.unwrap_or_else(|| vec![PathBuf::from(".")]);

    let scan = match run_scan(
        &scan_paths,
        false,
        Vec::new(),
        args.verbose,
        args.config.as_deref(),
    ) {
        Ok(s) => s,
        Err(code) => return code,
    };
    log_scan_errors(&scan.errors, args.verbose);

    let mut deps = scan.deps;

    if deps.is_empty() {
        eprintln!(
            "{}",
            "No shadow dependencies found to generate from.".dimmed()
        );
        return 2;
    }

    match looper::types::Confidence::parse(&args.min_confidence) {
        Some(min) => deps.retain(|d| d.confidence >= min),
        None => {
            eprintln!(
                "{} invalid --min-confidence '{}'. Valid values: low, medium, high",
                "error:".red(),
                args.min_confidence
            );
            return 1;
        }
    }
    deps.retain(|d| d.name != "<dynamic>");

    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut skipped = 0usize;
    let mut written = 0usize;

    for target in &targets {
        let content = looper::generate::generate(*target, &deps, &project_root);

        let output_path = if targets.len() > 1 {
            if args.output.is_some() {
                eprintln!(
                    "{} --output cannot be used with 'all' target (use individual targets instead)",
                    "error:".red()
                );
                return 1;
            }
            Some(PathBuf::from(target.default_filename()))
        } else {
            args.output.clone()
        };

        if let Some(ref path) = output_path {
            if path.exists() && !args.force {
                eprintln!(
                    "{} {} already exists (use --force to overwrite)",
                    "warning:".yellow(),
                    path.display()
                );
                skipped += 1;
                continue;
            }
            if let Err(e) = std::fs::write(path, &content) {
                eprintln!("{} failed to write {}: {e}", "error:".red(), path.display());
                return 1;
            }
            eprintln!("  {} {}", "created".green(), path.display());
            written += 1;
        } else {
            print!("{content}");
            written += 1;
        }
    }

    if written == 0 && skipped > 0 {
        return 1;
    }

    0
}
