use crate::args::{BatchArgs, Cli, Commands};
use crate::error::CliError;
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
pub struct BatchCommandItem {
    pub cmd: String,
    pub args: Vec<String>,
}

pub fn handle_batch(cli_base: &Cli, args: &BatchArgs) -> Result<(), CliError> {
    if !args.file.exists() {
        return Err(CliError::Validation(format!(
            "Batch file not found: {}",
            args.file.display()
        )));
    }

    let contents = fs::read_to_string(&args.file)?;
    let items: Vec<BatchCommandItem> = serde_json::from_str(&contents)?;

    tracing::info!(count = items.len(), file = %args.file.display(), "Executing batch commands");

    for (idx, item) in items.iter().enumerate() {
        println!(
            "\n>>> [{}/{}] {} {}",
            idx + 1,
            items.len(),
            item.cmd,
            item.args.join(" ")
        );

        let mut full_args = vec!["sllend".to_string()];
        if let Some(ref net) = cli_base.network {
            full_args.extend(["--network".to_string(), net.clone()]);
        }
        if let Some(ref cid) = cli_base.contract {
            full_args.extend(["--contract".to_string(), cid.clone()]);
        }
        if let Some(ref k) = cli_base.key {
            full_args.extend(["--key".to_string(), k.clone()]);
        }
        if cli_base.json {
            full_args.push("--json".to_string());
        }

        full_args.push(item.cmd.clone());
        full_args.extend(item.args.clone());

        match clap::Parser::try_parse_from(&full_args) {
            Ok(sub_cli) => {
                let sub_cli: Cli = sub_cli;
                if let Commands::Batch(_) = sub_cli.command {
                    eprintln!("Nested batch command ignored.");
                    continue;
                }
                if let Err(e) = crate::execute(sub_cli) {
                    eprintln!("Error in batch step: {e}");
                }
            }
            Err(e) => {
                eprintln!("Failed to parse batch command: {e}");
            }
        }
    }

    Ok(())
}
