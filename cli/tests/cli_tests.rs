use clap::Parser;
use sllend::args::Cli;
use sllend::format::{format_amount, format_health_factor, format_percent};

#[test]
fn test_cli_command_tree_help() {
    use clap::CommandFactory;
    let mut cmd = Cli::command();
    let help = cmd.render_help().to_string();

    assert!(help.contains("sllend"));
    assert!(help.contains("deposit"));
    assert!(help.contains("withdraw"));
    assert!(help.contains("borrow"));
    assert!(help.contains("repay"));
    assert!(help.contains("liquidate"));
    assert!(help.contains("get-position"));
    assert!(help.contains("get-pool"));
    assert!(help.contains("keys"));
    assert!(help.contains("config"));
    assert!(help.contains("batch"));
    assert!(help.contains("interactive"));
}

#[test]
fn test_subcommand_helps() {
    use clap::CommandFactory;
    let cmd = Cli::command();

    let subcommands = [
        "deposit",
        "withdraw",
        "borrow",
        "repay",
        "liquidate",
        "get-position",
        "get-pool",
        "keys",
        "config",
        "batch",
        "interactive",
    ];
    for sub in subcommands {
        let sub_cmd = cmd.find_subcommand(sub);
        assert!(
            sub_cmd.is_some(),
            "Subcommand {sub} must be present in command tree"
        );
    }
}

#[test]
fn test_deposit_command_execution() {
    let cli = Cli::parse_from(["sllend", "--json", "deposit", "25.75"]);
    assert!(sllend::execute(cli).is_ok());
}

#[test]
fn test_withdraw_command_execution() {
    let cli = Cli::parse_from(["sllend", "withdraw", "10", "--asset", "CASSET_TOKEN_1"]);
    assert!(sllend::execute(cli).is_ok());
}

#[test]
fn test_borrow_command_execution() {
    let cli = Cli::parse_from(["sllend", "borrow", "5.5"]);
    assert!(sllend::execute(cli).is_ok());
}

#[test]
fn test_repay_command_execution() {
    let cli = Cli::parse_from(["sllend", "repay", "max"]);
    assert!(sllend::execute(cli).is_ok());
}

#[test]
fn test_liquidate_command_execution() {
    let cli = Cli::parse_from([
        "sllend",
        "liquidate",
        "GBORROWER_TEST_1",
        "100",
        "--collateral-asset",
        "CCOL",
        "--debt-asset",
        "CDEBT",
    ]);
    assert!(sllend::execute(cli).is_ok());
}

#[test]
fn test_position_and_pool_queries() {
    let cli_pos = Cli::parse_from(["sllend", "--json", "get-position", "GUSER123"]);
    assert!(sllend::execute(cli_pos).is_ok());

    let cli_pool = Cli::parse_from(["sllend", "--json", "get-pool"]);
    assert!(sllend::execute(cli_pool).is_ok());
}

#[test]
fn test_config_operations() {
    let cli_show = Cli::parse_from(["sllend", "--json", "config", "show"]);
    assert!(sllend::execute(cli_show).is_ok());
}

#[test]
fn test_keys_operations() {
    let cli_list = Cli::parse_from(["sllend", "--json", "keys", "list"]);
    assert!(sllend::execute(cli_list).is_ok());
}

#[test]
fn test_batch_execution() {
    let temp_dir = std::env::temp_dir();
    let batch_file = temp_dir.join("test_sllend_batch.json");
    let batch_json = serde_json::json!([
        { "cmd": "deposit", "args": ["10"] },
        { "cmd": "borrow", "args": ["2"] },
        { "cmd": "get-position", "args": [] }
    ]);
    std::fs::write(&batch_file, batch_json.to_string()).unwrap();

    let cli = Cli::parse_from(["sllend", "batch", batch_file.to_str().unwrap()]);
    assert!(sllend::execute(cli).is_ok());

    let _ = std::fs::remove_file(batch_file);
}

#[test]
fn test_formatters() {
    assert_eq!(format_amount(10_000_000), "1.0000000");
    assert_eq!(format_percent(750), "7.50%");
    assert!(format_health_factor(2.0).contains("Safe"));
    assert!(format_health_factor(1.2).contains("Warning"));
    assert!(format_health_factor(0.8).contains("Liquidatable"));
}

#[test]
fn test_invalid_cli_arguments() {
    assert!(Cli::try_parse_from(["sllend", "nonexistent-cmd"]).is_err());
    assert!(Cli::try_parse_from(["sllend", "deposit"]).is_err()); // Missing required amount
    assert!(Cli::try_parse_from(["sllend", "liquidate", "onlyone"]).is_err()); // Missing repay-amount
}
