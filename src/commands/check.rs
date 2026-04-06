use colored::Colorize;
use std::path::PathBuf;

use crate::cli::CheckArgs;
use crate::scan::{log_scan_errors, run_scan};

pub(crate) fn cmd_check(args: CheckArgs) -> i32 {
    let config = looper::config::load_config(args.config.as_deref());
    let manifest_path = args.manifest.unwrap_or_else(|| {
        config
            .as_ref()
            .and_then(|c| c.manifest.as_deref())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("shadow-deps.json"))
    });

    let manifest = match looper::check::load_manifest(&manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} {e}", "error:".red());
            return 2;
        }
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

    if args.update {
        let new_manifest = looper::output::build_manifest(
            &scan.root,
            scan.file_count,
            scan.duration_ms,
            scan.deps,
        );
        match looper::output::json::format_json(&new_manifest) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&manifest_path, &json) {
                    eprintln!("{} failed to write manifest: {e}", "error:".red());
                    return 1;
                }
                eprintln!(
                    "  {} {} ({} deps)",
                    "updated".green(),
                    manifest_path.display(),
                    new_manifest.deps.len()
                );
            }
            Err(e) => {
                eprintln!("{} JSON serialization failed: {e}", "error:".red());
                return 1;
            }
        }
        return 0;
    }

    let result = looper::check::check_against_manifest(&scan.deps, &manifest);
    looper::check::print_check_report(&result);

    if !result.added.is_empty() && args.fail_on_new {
        return 1;
    }
    if !result.removed.is_empty() && args.fail_on_removed {
        return 1;
    }

    0
}
