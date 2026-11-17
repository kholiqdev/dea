use std::path::PathBuf;
use std::process::ExitCode;

use checklist_gate::{evaluate, format_findings};
use clap::Parser;

/// Fails when a phase, or any phase before it, has checklist items that are
/// not both built and tested.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Phase being released. Phases 1 up to this one are checked.
    #[arg(long)]
    phase: u8,
    /// Root of the planning documents, containing the fase-<N>-* folders.
    #[arg(long)]
    docs: PathBuf,
    /// Print findings but always exit with success.
    #[arg(long)]
    report_only: bool,
    /// Print only the number of incomplete items per phase.
    #[arg(long)]
    summary: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let findings = match evaluate(&args.docs, args.phase) {
        Ok(findings) => findings,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(2);
        }
    };

    if findings.is_empty() {
        if args.phase == 1 {
            println!("Gate passed: phase 1 complete");
        } else {
            println!("Gate passed: phases 1 to {} complete", args.phase);
        }
        return ExitCode::SUCCESS;
    }

    print!("{}", format_findings(&findings, args.summary));
    if args.report_only {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
