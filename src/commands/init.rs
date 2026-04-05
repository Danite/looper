use colored::Colorize;

pub(crate) fn cmd_init() -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{} could not determine current directory: {e}", "error:".red());
            return 1;
        }
    };

    let config_path = cwd.join("looper.config.json");
    if config_path.exists() {
        eprintln!(
            "{} looper.config.json already exists",
            "warning:".yellow()
        );
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

    0
}
