use crate::error::CliError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkConfig {
    pub rpc_url: String,
    pub horizon_url: String,
    pub network_passphrase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KeyEntry {
    pub alias: String,
    pub public_key: String,
    pub secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CliConfig {
    pub default_network: String,
    pub networks: HashMap<String, NetworkConfig>,
    #[serde(default)]
    pub keys: HashMap<String, KeyEntry>,
}

impl Default for CliConfig {
    fn default() -> Self {
        let mut networks = HashMap::new();

        networks.insert(
            "testnet".to_string(),
            NetworkConfig {
                rpc_url: "https://soroban-testnet.stellar.org".to_string(),
                horizon_url: "https://horizon-testnet.stellar.org".to_string(),
                network_passphrase: "Test SDF Network ; September 2015".to_string(),
                contract_id: None,
            },
        );

        networks.insert(
            "mainnet".to_string(),
            NetworkConfig {
                rpc_url: "https://soroban-rpc.mainnet.stellar.org".to_string(),
                horizon_url: "https://horizon.stellar.org".to_string(),
                network_passphrase: "Public Global Stellar Network ; September 2015".to_string(),
                contract_id: None,
            },
        );

        networks.insert(
            "local".to_string(),
            NetworkConfig {
                rpc_url: "http://localhost:8000/soroban/rpc".to_string(),
                horizon_url: "http://localhost:8000".to_string(),
                network_passphrase: "Standalone Network ; February 2017".to_string(),
                contract_id: None,
            },
        );

        Self {
            default_network: "testnet".to_string(),
            networks,
            keys: HashMap::new(),
        }
    }
}

impl std::fmt::Display for CliConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "StellarLend Configuration:")?;
        writeln!(f, "  Default Network: {}", self.default_network)?;
        writeln!(f, "  Networks:")?;
        for (name, net) in &self.networks {
            let active = if name == &self.default_network {
                " (default)"
            } else {
                ""
            };
            writeln!(f, "    • {}{}:", name, active)?;
            writeln!(f, "        RPC URL:     {}", net.rpc_url)?;
            writeln!(f, "        Horizon URL: {}", net.horizon_url)?;
            writeln!(f, "        Passphrase:  {}", net.network_passphrase)?;
            if let Some(ref cid) = net.contract_id {
                writeln!(f, "        Contract ID: {}", cid)?;
            }
        }
        writeln!(f, "  Stored Keys: {}", self.keys.len())?;
        for (alias, key) in &self.keys {
            writeln!(f, "    • {}: {}", alias, key.public_key)?;
        }
        Ok(())
    }
}

pub fn default_config_path() -> PathBuf {
    if let Ok(val) = std::env::var("SLLEND_CONFIG_PATH") {
        return PathBuf::from(val);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".sllend").join("config.toml")
}

pub fn load_config(path: Option<&Path>) -> Result<CliConfig, CliError> {
    let p = path.map(PathBuf::from).unwrap_or_else(default_config_path);
    if !p.exists() {
        return Ok(CliConfig::default());
    }

    let contents = fs::read_to_string(&p)?;
    let cfg: CliConfig = toml::from_str(&contents)?;
    Ok(cfg)
}

pub fn save_config(cfg: &CliConfig, path: Option<&Path>) -> Result<(), CliError> {
    let p = path.map(PathBuf::from).unwrap_or_else(default_config_path);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }

    let toml_str = toml::to_string_pretty(cfg)?;
    fs::write(&p, toml_str)?;
    Ok(())
}

impl CliConfig {
    pub fn get_network(&self, override_net: Option<&str>) -> Result<&NetworkConfig, CliError> {
        let name = override_net.unwrap_or(&self.default_network);
        self.networks
            .get(name)
            .ok_or_else(|| CliError::Config(format!("Network '{name}' not found in configuration")))
    }

    pub fn get_network_mut(&mut self, name: &str) -> Result<&mut NetworkConfig, CliError> {
        self.networks
            .get_mut(name)
            .ok_or_else(|| CliError::Config(format!("Network '{name}' not found in configuration")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = CliConfig::default();
        assert_eq!(cfg.default_network, "testnet");
        assert!(cfg.networks.contains_key("testnet"));
        assert!(cfg.networks.contains_key("mainnet"));
        assert!(cfg.networks.contains_key("local"));
        assert_eq!(cfg.keys.len(), 0);
    }

    #[test]
    fn test_get_network() {
        let cfg = CliConfig::default();
        let net = cfg.get_network(None).unwrap();
        assert_eq!(net.rpc_url, "https://soroban-testnet.stellar.org");

        let local = cfg.get_network(Some("local")).unwrap();
        assert_eq!(local.rpc_url, "http://localhost:8000/soroban/rpc");

        assert!(cfg.get_network(Some("nonexistent")).is_err());
    }

    #[test]
    fn test_serialization() {
        let cfg = CliConfig::default();
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        let de: CliConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(cfg, de);
    }
}
