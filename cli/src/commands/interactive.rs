use crate::args::{Cli, Commands, InteractiveArgs};
use crate::error::CliError;
use clap::Parser;
use std::io::{self, BufRead, Write};

pub fn handle_interactive(_cli_base: &Cli, _args: &InteractiveArgs) -> Result<(), CliError> {
    println!("StellarLend interactive mode. Type 'help' for commands or 'exit'/'quit' to exit.\n");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("sllend> ");
        stdout.flush()?;

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "exit" || trimmed == "quit" {
            println!("Goodbye!");
            break;
        }

        let words: Vec<&str> = trimmed.split_whitespace().collect();
        let mut argv = vec!["sllend"];
        argv.extend(words);

        match Cli::try_parse_from(&argv) {
            Ok(parsed) => {
                if let Commands::Interactive(_) = parsed.command {
                    println!("Already in interactive mode.");
                    continue;
                }
                if let Err(e) = crate::execute(parsed) {
                    eprintln!("Error: {e}");
                }
            }
            Err(e) => {
                eprintln!("{e}");
            }
        }
    }

    Ok(())
}
