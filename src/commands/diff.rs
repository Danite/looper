use colored::Colorize;

use crate::cli::DiffArgs;

pub(crate) fn cmd_diff(args: DiffArgs) -> i32 {
    let old_manifest = match looper::check::load_manifest(&args.old) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} {e}", "error:".red());
            return 1;
        }
    };

    let new_manifest = match looper::check::load_manifest(&args.new) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} {e}", "error:".red());
            return 1;
        }
    };

    let diff = looper::diff::diff_manifests(&old_manifest, &new_manifest);

    match args.format.as_str() {
        "json" => match looper::diff::format_diff_json(&diff) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("{} JSON serialization failed: {e}", "error:".red());
                return 1;
            }
        },
        _ => {
            print!("{}", looper::diff::format_diff_terminal(&diff));
        }
    }

    0
}
