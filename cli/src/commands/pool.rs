use crate::args::GetPoolArgs;
use crate::client::StellarLendClient;
use crate::error::CliError;
use crate::format::print_result;

pub fn handle_pool(
    client: &StellarLendClient,
    args: &GetPoolArgs,
    json: bool,
) -> Result<(), CliError> {
    let result = client.get_pool(args.asset.as_deref())?;
    print_result(&result, json);
    Ok(())
}
