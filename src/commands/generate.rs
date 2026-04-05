use colored::Colorize;
use std::path::PathBuf;

use crate::cli::GenerateArgs;
use crate::scan::{log_scan_errors, run_scan};

pub(crate) fn cmd_generate(args: GenerateArgs) -> i32 {
    let targets = match args.target.as_str() {
        "all" => looper::generate::GenerateTarget::all().to_vec(),
        other => match looper::generate::GenerateTarget::from_str(other) {
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

    let scan_paths = args
        .scan_paths
        .unwrap_or_else(|| vec![PathBuf::from(".")]);

    let scan = match run_scan(&scan_paths, false, Vec::new(), args.verbose, args.config.as_deref()) {
        Ok(s) => s,
        Err(code) => return code,
    };
    log_scan_errors(&scan.errors, args.verbose);

    let mut deps = scan.deps;

    if deps.is_empty() {
        eprintln!("{}", "No shadow dependencies found to generate from.".dimmed());
        return 2;
    }

    if let Some(min) = looper::types::Confidence::parse(&args.min_confidence) {
        deps.retain(|d| d.confidence >= min);
    }
    deps.retain(|d| d.name != "<dynamic>");

    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    for target in &targets {
        let content = looper::generate::generate(*target, &deps, &project_root);

        let output_path = if targets.len() > 1 {
            // "all" mode: use default filenames unless --output is set
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
                continue;
            }
            if let Err(e) = std::fs::write(path, &content) {
                eprintln!("{} failed to write {}: {e}", "error:".red(), path.display());
                return 1;
            }
            eprintln!("  {} {}", "created".green(), path.display());
        } else {
            print!("{content}");
        }
    }

    0
}
