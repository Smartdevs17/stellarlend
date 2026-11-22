use crate::args::LiquidateArgs;
use crate::client::StellarLendClient;
use crate::error::CliError;
use crate::format::{parse_amount, print_result};

pub fn handle_liquidate(
    client: &StellarLendClient,
    account: &str,
    args: &LiquidateArgs,
    json: bool,
) -> Result<(), CliError> {
    let repay_amount_stroops = parse_amount(&args.repay_amount)?;
    let result = client.liquidate(
        account,
        &args.borrower,
        repay_amount_stroops,
        args.collateral_asset.as_deref(),
        args.debt_asset.as_deref(),
    )?;
    print_result(&result, json);
    Ok(())
}
