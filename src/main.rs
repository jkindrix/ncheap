use std::process::ExitCode;

use clap::Parser;

use ncheap::api::{Client, HttpTransport};
use ncheap::cli::{AccountCommand, Cli, Command, DnsCommand, DomainsCommand, PrivacyCommand};
use ncheap::commands::account::PricingQuery;
use ncheap::{commands, config, output};

fn main() -> ExitCode {
    // println! panics when stdout closes mid-write (`ncheap ... | head`).
    // A closed pipe is normal termination for a CLI, not a crash: exit 0
    // quietly instead of exit 101 with panic spew, like cat does.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let broken_pipe = info
            .payload()
            .downcast_ref::<String>()
            .is_some_and(|s| s.contains("Broken pipe"));
        if broken_pipe {
            std::process::exit(0);
        }
        default_hook(info);
    }));

    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            output::failure(cli.json, cli.command.name(), &e);
            ExitCode::from(e.exit_code())
        }
    }
}

fn run(cli: &Cli) -> Result<(), ncheap::api::Error> {
    let name = cli.command.name();
    let profile = config::load(cli.profile.as_deref())?;
    let client = Client::new(HttpTransport::new(), profile);
    match &cli.command {
        Command::Domains { command } => match command {
            DomainsCommand::List => {
                let domains = commands::domains::list(&client)?;
                output::success(
                    cli.json,
                    name,
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
                    name,
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
                    name,
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
                    name,
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
                        name,
                        &contacts,
                        client.profile(),
                        client.calls(),
                        human,
                    );
                } else {
                    let view = commands::domains::contacts_redacted_view(&contacts);
                    output::success(
                        cli.json,
                        name,
                        &view,
                        client.profile(),
                        client.calls(),
                        human,
                    );
                }
                Ok(())
            }
        },
        Command::Account { command } => match command {
            AccountCommand::Balances { full } => {
                let balances = commands::account::balances(&client)?;
                let human = || commands::account::render(&balances, *full);
                if *full {
                    output::success(
                        cli.json,
                        name,
                        &balances,
                        client.profile(),
                        client.calls(),
                        human,
                    );
                } else {
                    let view = commands::account::redacted_view(&balances);
                    output::success(
                        cli.json,
                        name,
                        &view,
                        client.profile(),
                        client.calls(),
                        human,
                    );
                }
                Ok(())
            }
            AccountCommand::Pricing {
                product_type,
                category,
                action,
                product,
            } => {
                let query = PricingQuery {
                    product_type: product_type.clone(),
                    category: category.clone(),
                    action: action.clone(),
                    product: product.clone(),
                };
                let cache_dir = dirs::cache_dir().map(|d| d.join("ncheap"));
                let (rows, _cached) =
                    commands::account::pricing(&client, &query, cache_dir.as_deref())?;
                output::success(
                    cli.json,
                    name,
                    &rows,
                    client.profile(),
                    client.calls(),
                    || commands::account::render_pricing(&rows),
                );
                Ok(())
            }
        },
        Command::Dns {
            command: DnsCommand::Get { domain },
        } => {
            let dns = commands::dns::get(&client, domain)?;
            output::success(
                cli.json,
                name,
                &dns,
                client.profile(),
                client.calls(),
                || commands::dns::render(&dns),
            );
            Ok(())
        }
        Command::Privacy {
            command: PrivacyCommand::List,
        } => {
            let subs = commands::privacy::list(&client)?;
            output::success(
                cli.json,
                name,
                &subs,
                client.profile(),
                client.calls(),
                || commands::privacy::render_table(&subs),
            );
            Ok(())
        }
        Command::Raw { command, params } => {
            let params = commands::raw::parse_params(params)?;
            let body = commands::raw::call(&client, command, &params)?;
            output::success(
                cli.json,
                name,
                &body,
                client.profile(),
                client.calls(),
                || println!("{body}"),
            );
            Ok(())
        }
    }
}
