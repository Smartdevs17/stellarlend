use crate::args::KeysCommand;
use crate::config::{save_config, CliConfig, KeyEntry};
use crate::error::CliError;

pub fn handle_keys(cfg: &mut CliConfig, cmd: &KeysCommand, json: bool) -> Result<(), CliError> {
    match cmd {
        KeysCommand::Add(args) => {
            let pubkey = if args.secret.starts_with('S') && args.secret.len() >= 56 {
                format!("G{}", &args.secret[1..])
            } else {
                format!("G_DERIVED_{}", args.alias)
            };

            let entry = KeyEntry {
                alias: args.alias.clone(),
                public_key: pubkey.clone(),
                secret: args.secret.clone(),
            };

            cfg.keys.insert(args.alias.clone(), entry);
            save_config(cfg, None)?;

            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "alias": args.alias,
                        "public_key": pubkey,
                        "stored": true
                    })
                );
            } else {
                println!("Key '{}' stored. Public key: {}", args.alias, pubkey);
            }
        }
        KeysCommand::List => {
            if json {
                let keys: Vec<_> = cfg.keys.values().cloned().collect();
                println!("{}", serde_json::to_string_pretty(&keys)?);
            } else if cfg.keys.is_empty() {
                println!("No keys stored.");
            } else {
                println!("Stored Keys:");
                for (alias, key) in &cfg.keys {
                    println!("  • {}: {}", alias, key.public_key);
                }
            }
        }
        KeysCommand::Remove(args) => {
            if cfg.keys.remove(&args.alias).is_some() {
                save_config(cfg, None)?;
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "alias": args.alias,
                            "removed": true
                        })
                    );
                } else {
                    println!("Key '{}' removed.", args.alias);
                }
            } else {
                return Err(CliError::Key(format!("Key '{}' not found", args.alias)));
            }
        }
    }
    Ok(())
}
