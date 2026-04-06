use colored::Colorize;
use std::path::PathBuf;

use crate::cli::InitArgs;
use crate::scan::run_scan;

pub(crate) fn cmd_init(args: InitArgs) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "{} could not determine current directory: {e}",
                "error:".red()
            );
            return 1;
        }
    };

    let config_path = cwd.join("looper.config.json");
    if config_path.exists() {
        eprintln!("{} looper.config.json already exists", "warning:".yellow());
    } else {
        match looper::config::write_default_config(&cwd) {
            Ok(path) => {
                eprintln!("  {} {}", "created".green(), path.display());
            }
            Err(e) => {
                eprintln!("{} failed to write config: {e}", "error:".red());
                return 1;
            }
        }
    }

    if args.with_env {
        let env_path = cwd.join(".env.example");
        if env_path.exists() {
            eprintln!(
                "{} .env.example already exists (skipping)",
                "warning:".yellow()
            );
        } else {
            let scan = match run_scan(&[PathBuf::from(".")], false, Vec::new(), false, None) {
                Ok(s) => s,
                Err(_) => {
                    eprintln!(
                        "{} scan failed, skipping .env.example generation",
                        "warning:".yellow()
                    );
                    return 1;
                }
            };

            let mut deps = scan.deps;
            if let Some(min) = looper::types::Confidence::parse("medium") {
                deps.retain(|d| d.confidence >= min);
            }
            deps.retain(|d| d.name != "<dynamic>");

            let content = looper::generate::env::generate_env(&deps);
            match std::fs::write(&env_path, &content) {
                Ok(()) => {
                    eprintln!("  {} {}", "created".green(), env_path.display());
                }
                Err(e) => {
                    eprintln!("{} failed to write .env.example: {e}", "error:".red());
                    return 1;
                }
            }
        }
    }

    0
}
