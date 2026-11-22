use crate::args::ConfigCommand;
use crate::config::{save_config, CliConfig, NetworkConfig};
use crate::error::CliError;

pub fn handle_config(cfg: &mut CliConfig, cmd: &ConfigCommand, json: bool) -> Result<(), CliError> {
    match cmd {
        ConfigCommand::Show => {
            if json {
                println!("{}", serde_json::to_string_pretty(cfg)?);
            } else {
                print!("{cfg}");
            }
        }
        ConfigCommand::SetNetwork(args) => {
            let entry = NetworkConfig {
                rpc_url: args.rpc_url.clone(),
                horizon_url: args.horizon_url.clone(),
                network_passphrase: args.network_passphrase.clone(),
                contract_id: None,
            };
            cfg.networks.insert(args.name.clone(), entry);
            save_config(cfg, None)?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "network": args.name, "saved": true })
                );
            } else {
                println!("Network '{}' saved.", args.name);
            }
        }
        ConfigCommand::SetContract(args) => {
            let net = cfg.get_network_mut(&args.network)?;
            net.contract_id = Some(args.contract_id.clone());
            save_config(cfg, None)?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "network": args.network,
                        "contract_id": args.contract_id,
                        "saved": true
                    })
                );
            } else {
                println!(
                    "Contract ID for '{}' set to {}",
                    args.network, args.contract_id
                );
            }
        }
        ConfigCommand::SetDefaultNetwork(args) => {
            if !cfg.networks.contains_key(&args.name) {
                return Err(CliError::Config(format!(
                    "Network '{}' does not exist in configuration",
                    args.name
                )));
            }
            cfg.default_network = args.name.clone();
            save_config(cfg, None)?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "default_network": args.name, "saved": true })
                );
            } else {
                println!("Default network set to '{}'", args.name);
            }
        }
    }
    Ok(())
}
