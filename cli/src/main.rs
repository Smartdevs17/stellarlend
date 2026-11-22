use clap::Parser;
use sllend::args::Cli;
use sllend::observability::init_tracing;

fn main() {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    if let Err(err) = sllend::execute(cli) {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}
