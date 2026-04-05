mod cli;
mod commands;
mod scan;

use clap::Parser;
use std::process;

use cli::{Cli, Command, ScanArgs};
use commands::{cmd_check, cmd_diff, cmd_doctor, cmd_drift, cmd_generate, cmd_init};
use scan::cmd_scan;

fn main() {
    let cli = Cli::parse_from(cli::implicit_scan_subcommand());

    let exit_code = match cli.command {
        Some(Command::Scan(args)) => cmd_scan(args),
        Some(Command::Check(args)) => cmd_check(args),
        Some(Command::Init) => cmd_init(),
        Some(Command::Diff(args)) => cmd_diff(args),
        Some(Command::Doctor(args)) => cmd_doctor(args),
        Some(Command::Drift(args)) => cmd_drift(args),
        Some(Command::Generate(args)) => cmd_generate(args),
        None => cmd_scan(ScanArgs::default()),
    };

    process::exit(exit_code);
}
