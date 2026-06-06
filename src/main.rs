use std::process::ExitCode;

use clap::Parser;

use ncheap::api::{Client, HttpTransport};
use ncheap::cli::{Cli, Command, DomainsCommand};
use ncheap::{commands, config, output};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let command_name = match &cli.command {
        Command::Domains {
            command: DomainsCommand::List,
        } => "domains.list",
    };
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            output::failure(cli.json, command_name, &e);
            ExitCode::from(e.exit_code())
        }
    }
}

fn run(cli: &Cli) -> Result<(), ncheap::api::Error> {
    let profile = config::load(cli.profile.as_deref())?;
    let client = Client::new(HttpTransport::new(), profile);
    match &cli.command {
        Command::Domains {
            command: DomainsCommand::List,
        } => {
            let domains = commands::domains::list(&client)?;
            output::success(
                cli.json,
                "domains.list",
                &domains,
                client.profile(),
                client.calls(),
                || commands::domains::render_table(&domains),
            );
            Ok(())
        }
    }
}
