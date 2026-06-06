use std::process::ExitCode;

use clap::Parser;

use ncheap::api::{Client, HttpTransport};
use ncheap::cli::{AccountCommand, Cli, Command, DnsCommand, DomainsCommand};
use ncheap::{commands, config, output};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let command_name = match &cli.command {
        Command::Domains { command } => match command {
            DomainsCommand::List => "domains.list",
            DomainsCommand::Check { .. } => "domains.check",
            DomainsCommand::Lock { .. } => "domains.lock",
            DomainsCommand::Info { .. } => "domains.info",
            DomainsCommand::Contacts { .. } => "domains.contacts",
        },
        Command::Account {
            command: AccountCommand::Balances { .. },
        } => "account.balances",
        Command::Dns {
            command: DnsCommand::Get { .. },
        } => "dns.get",
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
        Command::Domains { command } => match command {
            DomainsCommand::List => {
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
            DomainsCommand::Check { domains } => {
                let results = commands::domains::check(&client, domains)?;
                output::success(
                    cli.json,
                    "domains.check",
                    &results,
                    client.profile(),
                    client.calls(),
                    || commands::domains::render_check(&results),
                );
                Ok(())
            }
            DomainsCommand::Lock { domain } => {
                let status = commands::domains::lock_status(&client, domain)?;
                output::success(
                    cli.json,
                    "domains.lock",
                    &status,
                    client.profile(),
                    client.calls(),
                    || commands::domains::render_lock(&status),
                );
                Ok(())
            }
            DomainsCommand::Info { domain } => {
                let info = commands::domains::info(&client, domain)?;
                output::success(
                    cli.json,
                    "domains.info",
                    &info,
                    client.profile(),
                    client.calls(),
                    || commands::domains::render_info(&info),
                );
                Ok(())
            }
            DomainsCommand::Contacts { domain, full } => {
                let contacts = commands::domains::contacts(&client, domain)?;
                let human = || commands::domains::render_contacts(&contacts, *full);
                if *full {
                    output::success(
                        cli.json,
                        "domains.contacts",
                        &contacts,
                        client.profile(),
                        client.calls(),
                        human,
                    );
                } else {
                    let view = commands::domains::contacts_redacted_view(&contacts);
                    output::success(
                        cli.json,
                        "domains.contacts",
                        &view,
                        client.profile(),
                        client.calls(),
                        human,
                    );
                }
                Ok(())
            }
        },
        Command::Account {
            command: AccountCommand::Balances { full },
        } => {
            let balances = commands::account::balances(&client)?;
            let human = || commands::account::render(&balances, *full);
            if *full {
                output::success(
                    cli.json,
                    "account.balances",
                    &balances,
                    client.profile(),
                    client.calls(),
                    human,
                );
            } else {
                let view = commands::account::redacted_view(&balances);
                output::success(
                    cli.json,
                    "account.balances",
                    &view,
                    client.profile(),
                    client.calls(),
                    human,
                );
            }
            Ok(())
        }
        Command::Dns {
            command: DnsCommand::Get { domain },
        } => {
            let dns = commands::dns::get(&client, domain)?;
            output::success(
                cli.json,
                "dns.get",
                &dns,
                client.profile(),
                client.calls(),
                || commands::dns::render(&dns),
            );
            Ok(())
        }
    }
}
