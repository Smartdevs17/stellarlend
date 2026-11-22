use crate::args::GetPositionArgs;
use crate::client::StellarLendClient;
use crate::error::CliError;
use crate::format::print_result;

pub fn handle_position(
    client: &StellarLendClient,
    account: &str,
    args: &GetPositionArgs,
    json: bool,
) -> Result<(), CliError> {
    let target = args.address.as_deref().unwrap_or(account);
    let result = client.get_position(target)?;
    print_result(&result, json);
    Ok(())
}
