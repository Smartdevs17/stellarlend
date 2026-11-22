use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// StellarLend developer CLI — interact with lending contracts.
///
/// Features:
/// - Lending operations (deposit, withdraw, borrow, repay, liquidate)
/// - Position and pool telemetry (get-position, get-pool)
/// - Key management (keys add, list, remove)
/// - Configuration management (config show, set-network, set-contract, set-default-network)
/// - Programmatic execution via JSON batch files and interactive REPL mode
#[derive(Parser, Debug, Clone, PartialEq)]
#[command(
    name = "sllend",
    author = "StellarLend Contributors",
    version,
    about = "StellarLend developer CLI — interact with lending contracts",
    long_about = "A high-performance CLI for managing collateral, borrowing, repayments, and protocol administration on StellarLend."
)]
pub struct Cli {
    #[arg(
        long,
        global = true,
        env = "SLLEND_NETWORK",
        help = "Network to use: testnet | mainnet | local"
    )]
    pub network: Option<String>,

    #[arg(
        long,
        global = true,
        env = "SLLEND_CONTRACT",
        help = "Override lending contract ID"
    )]
    pub contract: Option<String>,

    #[arg(
        long,
        global = true,
        env = "SLLEND_KEY",
        help = "Keystore alias to use for signing"
    )]
    pub key: Option<String>,

    #[arg(
        long,
        global = true,
        env = "SLLEND_SECRET",
        help = "Raw Stellar secret key (dev only, prefer --key)"
    )]
    pub secret: Option<String>,

    #[arg(
        long,
        global = true,
        env = "SLLEND_ACCOUNT",
        help = "Override sender account address (for read-only queries)"
    )]
    pub account: Option<String>,

    #[arg(
        long,
        global = true,
        help = "Output raw JSON (for scripting and programmatic consumption)"
    )]
    pub json: bool,

    #[arg(
        long,
        global = true,
        env = "SLLEND_RPC_URL",
        help = "Override Soroban RPC URL"
    )]
    pub rpc_url: Option<String>,

    #[arg(
        short,
        long,
        global = true,
        action = clap::ArgAction::Count,
        help = "Verbosity level (-v for debug, -vv for trace)"
    )]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum Commands {
    /// Deposit collateral into the lending pool
    Deposit(DepositArgs),

    /// Withdraw collateral from the lending pool
    Withdraw(WithdrawArgs),

    /// Borrow assets against your collateral
    Borrow(BorrowArgs),

    /// Repay borrowed debt (use 'max' to repay full balance)
    Repay(RepayArgs),

    /// Liquidate an undercollateralized position
    Liquidate(LiquidateArgs),

    /// Query a user's collateral and debt position
    #[command(name = "get-position", alias = "position")]
    GetPosition(GetPositionArgs),

    /// Query protocol pool statistics (utilization, APYs, reserves)
    #[command(name = "get-pool", alias = "pool")]
    GetPool(GetPoolArgs),

    /// Manage signing keys and encrypted keystores
    #[command(subcommand)]
    Keys(KeysCommand),

    /// Manage sllend CLI configuration and network profiles
    #[command(subcommand)]
    Config(ConfigCommand),

    /// Run a JSON batch of commands from a file
    Batch(BatchArgs),

    /// Interactive REPL mode with prompts for all parameters
    #[command(alias = "i")]
    Interactive(InteractiveArgs),
}

#[derive(Args, Debug, Clone, PartialEq)]
pub struct DepositArgs {
    /// Amount of collateral to deposit (e.g. '100' or '10.5')
    #[arg(value_name = "AMOUNT")]
    pub amount: String,

    /// Asset contract address (default: native XLM)
    #[arg(long, value_name = "ADDRESS")]
    pub asset: Option<String>,
}

#[derive(Args, Debug, Clone, PartialEq)]
pub struct WithdrawArgs {
    /// Amount of collateral to withdraw
    #[arg(value_name = "AMOUNT")]
    pub amount: String,

    /// Asset contract address (default: native XLM)
    #[arg(long, value_name = "ADDRESS")]
    pub asset: Option<String>,
}

#[derive(Args, Debug, Clone, PartialEq)]
pub struct BorrowArgs {
    /// Amount of assets to borrow
    #[arg(value_name = "AMOUNT")]
    pub amount: String,

    /// Asset contract address (default: native XLM)
    #[arg(long, value_name = "ADDRESS")]
    pub asset: Option<String>,
}

#[derive(Args, Debug, Clone, PartialEq)]
pub struct RepayArgs {
    /// Amount of debt to repay (use 'max' for full balance)
    #[arg(value_name = "AMOUNT")]
    pub amount: String,

    /// Asset contract address (default: native XLM)
    #[arg(long, value_name = "ADDRESS")]
    pub asset: Option<String>,
}

#[derive(Args, Debug, Clone, PartialEq)]
pub struct LiquidateArgs {
    /// Stellar address of the borrower to liquidate
    #[arg(value_name = "BORROWER")]
    pub borrower: String,

    /// Amount of debt to repay
    #[arg(value_name = "REPAY_AMOUNT")]
    pub repay_amount: String,

    /// Collateral asset to seize
    #[arg(long = "collateral-asset", value_name = "ADDRESS")]
    pub collateral_asset: Option<String>,

    /// Debt asset to repay
    #[arg(long = "debt-asset", value_name = "ADDRESS")]
    pub debt_asset: Option<String>,
}

#[derive(Args, Debug, Clone, PartialEq, Default)]
pub struct GetPositionArgs {
    /// Target account address (defaults to configured account)
    #[arg(value_name = "ADDRESS")]
    pub address: Option<String>,
}

#[derive(Args, Debug, Clone, PartialEq, Default)]
pub struct GetPoolArgs {
    /// Asset contract address to query
    #[arg(long, value_name = "ADDRESS")]
    pub asset: Option<String>,
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum KeysCommand {
    /// Add a new key pair to the keystore
    Add(KeysAddArgs),

    /// List stored key aliases
    List,

    /// Remove a key alias from keystore
    Remove(KeysRemoveArgs),
}

#[derive(Args, Debug, Clone, PartialEq)]
pub struct KeysAddArgs {
    /// Alias name for the key
    #[arg(value_name = "ALIAS")]
    pub alias: String,

    /// Secret key (S...)
    #[arg(value_name = "SECRET")]
    pub secret: String,

    /// Keystore password (optional)
    #[arg(long, value_name = "PASSWORD")]
    pub password: Option<String>,
}

#[derive(Args, Debug, Clone, PartialEq)]
pub struct KeysRemoveArgs {
    /// Alias name to remove
    #[arg(value_name = "ALIAS")]
    pub alias: String,
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum ConfigCommand {
    /// Print current configuration
    Show,

    /// Add or update a network entry
    SetNetwork(ConfigSetNetworkArgs),

    /// Set the lending contract ID for a network
    SetContract(ConfigSetContractArgs),

    /// Set the default network
    SetDefaultNetwork(ConfigSetDefaultNetworkArgs),
}

#[derive(Args, Debug, Clone, PartialEq)]
pub struct ConfigSetNetworkArgs {
    /// Network name (e.g. testnet, mainnet, local)
    #[arg(value_name = "NAME")]
    pub name: String,

    /// Soroban RPC URL
    #[arg(value_name = "RPC_URL")]
    pub rpc_url: String,

    /// Horizon API URL
    #[arg(value_name = "HORIZON_URL")]
    pub horizon_url: String,

    /// Network passphrase
    #[arg(value_name = "PASSPHRASE")]
    pub network_passphrase: String,
}

#[derive(Args, Debug, Clone, PartialEq)]
pub struct ConfigSetContractArgs {
    /// Network name
    #[arg(value_name = "NETWORK")]
    pub network: String,

    /// Contract ID
    #[arg(value_name = "CONTRACT_ID")]
    pub contract_id: String,
}

#[derive(Args, Debug, Clone, PartialEq)]
pub struct ConfigSetDefaultNetworkArgs {
    /// Default network name
    #[arg(value_name = "NAME")]
    pub name: String,
}

#[derive(Args, Debug, Clone, PartialEq)]
pub struct BatchArgs {
    /// Path to JSON file containing command batch
    #[arg(value_name = "FILE")]
    pub file: PathBuf,
}

#[derive(Args, Debug, Clone, PartialEq, Default)]
pub struct InteractiveArgs {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_deposit() {
        let cli = Cli::parse_from(["sllend", "deposit", "10.5"]);
        assert_eq!(
            cli.command,
            Commands::Deposit(DepositArgs {
                amount: "10.5".to_string(),
                asset: None,
            })
        );
        assert!(!cli.json);
    }

    #[test]
    fn test_parse_deposit_with_asset_and_flags() {
        let cli = Cli::parse_from([
            "sllend",
            "--network",
            "testnet",
            "--json",
            "-v",
            "deposit",
            "100",
            "--asset",
            "CASSET123",
        ]);
        assert_eq!(cli.network.as_deref(), Some("testnet"));
        assert!(cli.json);
        assert_eq!(cli.verbose, 1);
        assert_eq!(
            cli.command,
            Commands::Deposit(DepositArgs {
                amount: "100".to_string(),
                asset: Some("CASSET123".to_string()),
            })
        );
    }

    #[test]
    fn test_parse_repay_max() {
        let cli = Cli::parse_from(["sllend", "repay", "max"]);
        assert_eq!(
            cli.command,
            Commands::Repay(RepayArgs {
                amount: "max".to_string(),
                asset: None,
            })
        );
    }

    #[test]
    fn test_parse_liquidate() {
        let cli = Cli::parse_from([
            "sllend",
            "liquidate",
            "GBORROWER123",
            "50.0",
            "--collateral-asset",
            "CCOL123",
            "--debt-asset",
            "CDEBT123",
        ]);
        assert_eq!(
            cli.command,
            Commands::Liquidate(LiquidateArgs {
                borrower: "GBORROWER123".to_string(),
                repay_amount: "50.0".to_string(),
                collateral_asset: Some("CCOL123".to_string()),
                debt_asset: Some("CDEBT123".to_string()),
            })
        );
    }

    #[test]
    fn test_parse_aliases() {
        let cli1 = Cli::parse_from(["sllend", "get-position"]);
        let cli2 = Cli::parse_from(["sllend", "position"]);
        assert_eq!(cli1.command, cli2.command);

        let cli3 = Cli::parse_from(["sllend", "get-pool"]);
        let cli4 = Cli::parse_from(["sllend", "pool"]);
        assert_eq!(cli3.command, cli4.command);

        let cli5 = Cli::parse_from(["sllend", "interactive"]);
        let cli6 = Cli::parse_from(["sllend", "i"]);
        assert_eq!(cli5.command, cli6.command);
    }

    #[test]
    fn test_parse_keys() {
        let cli = Cli::parse_from(["sllend", "keys", "list"]);
        assert_eq!(cli.command, Commands::Keys(KeysCommand::List));

        let cli_add = Cli::parse_from(["sllend", "keys", "add", "mykey", "SSECRET"]);
        assert_eq!(
            cli_add.command,
            Commands::Keys(KeysCommand::Add(KeysAddArgs {
                alias: "mykey".to_string(),
                secret: "SSECRET".to_string(),
                password: None,
            }))
        );
    }

    #[test]
    fn test_parse_config() {
        let cli = Cli::parse_from(["sllend", "config", "show"]);
        assert_eq!(cli.command, Commands::Config(ConfigCommand::Show));

        let cli_set = Cli::parse_from([
            "sllend",
            "config",
            "set-network",
            "custom",
            "http://rpc",
            "http://horizon",
            "Custom Passphrase",
        ]);
        assert_eq!(
            cli_set.command,
            Commands::Config(ConfigCommand::SetNetwork(ConfigSetNetworkArgs {
                name: "custom".to_string(),
                rpc_url: "http://rpc".to_string(),
                horizon_url: "http://horizon".to_string(),
                network_passphrase: "Custom Passphrase".to_string(),
            }))
        );
    }
}
