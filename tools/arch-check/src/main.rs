use std::path::PathBuf;
use std::process::ExitCode;

use arch_check::{check, explain, load_rules, workspace_graph};
use clap::Parser;

/// Fails when a workspace crate depends on a layer it may not use.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Workspace manifest. Defaults to the workspace this tool belongs to.
    #[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml"))]
    manifest: PathBuf,
    /// Layer rules.
    #[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/rules.toml"))]
    rules: PathBuf,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let result = load_rules(&args.rules).and_then(|rules| {
        let (members, edges) = workspace_graph(&args.manifest)?;
        Ok((check(&members, &edges, &rules), rules))
    });
    match result {
        Ok((violations, _)) if violations.is_empty() => {
            println!("Architecture check passed");
            ExitCode::SUCCESS
        }
        Ok((violations, rules)) => {
            for v in &violations {
                println!("{}", explain(v, &rules));
            }
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}
