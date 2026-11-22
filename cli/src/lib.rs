pub mod args;
pub mod client;
pub mod commands;
pub mod config;
pub mod error;
pub mod format;
pub mod observability;

pub use args::{Cli, Commands};
pub use error::CliError;

use client::StellarLendClient;
use config::load_config;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Main execution dispatcher routing clap-parsed subcommands to their handlers.
pub fn execute(cli: Cli) -> Result<(), CliError> {
    let mut cfg = load_config(None)?;

    match &cli.command {
        Commands::Config(cmd) => {
            commands::config::handle_config(&mut cfg, cmd, cli.json)?;
            return Ok(());
        }
        Commands::Keys(cmd) => {
            commands::keys::handle_keys(&mut cfg, cmd, cli.json)?;
            return Ok(());
        }
        Commands::Batch(args) => {
            commands::batch::handle_batch(&cli, args)?;
            return Ok(());
        }
        Commands::Interactive(args) => {
            commands::interactive::handle_interactive(&cli, args)?;
            return Ok(());
        }
        _ => {}
    }

    let net = cfg.get_network(cli.network.as_deref())?;
    let rpc_url = cli.rpc_url.clone().unwrap_or_else(|| net.rpc_url.clone());
    let contract_id = cli
        .contract
        .clone()
        .or_else(|| net.contract_id.clone())
        .unwrap_or_else(|| "CDUMMY_STELLARLEND_CONTRACT_ADDRESS".to_string());

    let account = if let Some(ref acc) = cli.account {
        acc.clone()
    } else if let Some(ref k) = cli.key {
        cfg.keys
            .get(k)
            .map(|e| e.public_key.clone())
            .ok_or_else(|| CliError::Key(format!("Key alias '{k}' not found")))?
    } else if let Some(ref sec) = cli.secret {
        if sec.starts_with('S') && sec.len() >= 56 {
            format!("G{}", &sec[1..])
        } else {
            "G_DERIVED_DEV_ACCOUNT".to_string()
        }
    } else {
        "G_DEFAULT_STELLARLEND_ACCOUNT".to_string()
    };

    let client = StellarLendClient::new(rpc_url, contract_id);

    match &cli.command {
        Commands::Deposit(args) => {
            commands::deposit::handle_deposit(&client, &account, args, cli.json)?;
        }
        Commands::Withdraw(args) => {
            commands::withdraw::handle_withdraw(&client, &account, args, cli.json)?;
        }
        Commands::Borrow(args) => {
            commands::borrow::handle_borrow(&client, &account, args, cli.json)?;
        }
        Commands::Repay(args) => {
            commands::repay::handle_repay(&client, &account, args, cli.json)?;
        }
        Commands::Liquidate(args) => {
            commands::liquidate::handle_liquidate(&client, &account, args, cli.json)?;
        }
        Commands::GetPosition(args) => {
            commands::position::handle_position(&client, &account, args, cli.json)?;
        }
        Commands::GetPool(args) => {
            commands::pool::handle_pool(&client, args, cli.json)?;
        }
        _ => unreachable!(),
    }

    Ok(())
}
