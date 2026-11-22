use crate::args::BorrowArgs;
use crate::client::StellarLendClient;
use crate::error::CliError;
use crate::format::{parse_amount, print_result};

pub fn handle_borrow(
    client: &StellarLendClient,
    account: &str,
    args: &BorrowArgs,
    json: bool,
) -> Result<(), CliError> {
    let amount_stroops = parse_amount(&args.amount)?;
    let result = client.borrow(account, amount_stroops, args.asset.as_deref())?;
    print_result(&result, json);
    Ok(())
}
