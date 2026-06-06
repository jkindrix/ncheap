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

    // try_parse rather than parse: "every command with --json emits one
    // envelope" must hold for malformed invocations too — agents generate
    // them routinely. clap never parsed, so --json is detected from argv.
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            use clap::error::ErrorKind;
            let is_help = matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion);
            if !is_help && std::env::args().any(|a| a == "--json") {
                let err = ncheap::api::Error::Usage(e.to_string().trim().to_owned());
                output::failure(true, "cli", &err, None);
                return ExitCode::from(2);
            }
            let _ = e.print();
            return ExitCode::from(if is_help { 0 } else { 2 });
        }
    };
    let name = cli.command.name();
    let profile = match config::load(cli.profile.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            let err = ncheap::api::Error::from(e);
            output::failure(cli.json, name, &err, None);
            return ExitCode::from(err.exit_code());
        }
    };
    let client = Client::new(HttpTransport::new(), profile);
    match run(&cli, &client) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // meta carries which profile/sandbox the failed call targeted
            output::failure(cli.json, name, &e, Some((client.profile(), client.calls())));
            ExitCode::from(e.exit_code())
        }
    }
}

/// Mutations require explicit confirmation: --yes, or an interactive
/// y/N prompt when stdin is a terminal. Non-interactive callers (agents,
/// scripts) without --yes are refused before any client work.
fn confirm_mutation(description: &str, yes: bool) -> Result<(), ncheap::api::Error> {
    use std::io::IsTerminal;
    if yes {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        return Err(ncheap::api::Error::Usage(
            "mutating commands require --yes in non-interactive use".into(),
        ));
    }
    eprint!("{description} — proceed? [y/N] ");
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() || !line.trim().eq_ignore_ascii_case("y") {
        return Err(ncheap::api::Error::Usage("mutation not confirmed".into()));
    }
    Ok(())
}

fn run(cli: &Cli, client: &Client<HttpTransport>) -> Result<(), ncheap::api::Error> {
    let name = cli.command.name();
    match &cli.command {
        Command::Domains { command } => match command {
            DomainsCommand::List => {
                let domains = commands::domains::list(client)?;
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
                let results = commands::domains::check(client, domains)?;
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
                let status = commands::domains::lock_status(client, domain)?;
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
                let info = commands::domains::info(client, domain)?;
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
                let contacts = commands::domains::contacts(client, domain)?;
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
            DomainsCommand::Register {
                domain,
                years,
                max_price,
                contacts_from,
                yes,
            } => {
                confirm_mutation(
                    &format!(
                        "register {domain} for {years} year(s) at up to {max_price:.2}, \
                         contacts copied from {contacts_from}"
                    ),
                    *yes,
                )?;
                let result =
                    commands::domains::register(client, domain, *years, *max_price, contacts_from)?;
                output::success(
                    cli.json,
                    name,
                    &result,
                    client.profile(),
                    client.calls(),
                    || commands::domains::render_register(&result),
                );
                Ok(())
            }
            DomainsCommand::Renew {
                domain,
                years,
                max_price,
                yes,
            } => {
                confirm_mutation(
                    &format!("renew {domain} for {years} year(s) at up to {max_price:.2}"),
                    *yes,
                )?;
                let result = commands::domains::renew(client, domain, *years, *max_price)?;
                output::success(
                    cli.json,
                    name,
                    &result,
                    client.profile(),
                    client.calls(),
                    || commands::domains::render_renew(&result),
                );
                Ok(())
            }
        },
        Command::Account { command } => match command {
            AccountCommand::Balances { full } => {
                let balances = commands::account::balances(client)?;
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
                    commands::account::pricing(client, &query, cache_dir.as_deref())?;
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
        Command::Dns { command } => match command {
            DnsCommand::Get { domain } => {
                let dns = commands::dns::get(client, domain)?;
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
            DnsCommand::Set {
                domain,
                nameservers,
                yes,
            } => {
                confirm_mutation(
                    &format!("set nameservers of {domain} to {}", nameservers.join(", ")),
                    *yes,
                )?;
                let result = commands::dns::set(client, domain, nameservers)?;
                output::success(
                    cli.json,
                    name,
                    &result,
                    client.profile(),
                    client.calls(),
                    || commands::dns::render_set(&result),
                );
                Ok(())
            }
        },
        Command::Privacy { command } => match command {
            PrivacyCommand::List => {
                let subs = commands::privacy::list(client)?;
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
            PrivacyCommand::Enable {
                domain,
                forward_to,
                yes,
            } => {
                confirm_mutation(
                    &format!("enable privacy for {domain}, forwarding to {forward_to}"),
                    *yes,
                )?;
                let result = commands::privacy::enable(client, domain, forward_to)?;
                output::success(
                    cli.json,
                    name,
                    &result,
                    client.profile(),
                    client.calls(),
                    || commands::privacy::render_toggle(&result),
                );
                Ok(())
            }
            PrivacyCommand::Disable { domain, yes } => {
                confirm_mutation(&format!("disable privacy for {domain}"), *yes)?;
                let result = commands::privacy::disable(client, domain)?;
                output::success(
                    cli.json,
                    name,
                    &result,
                    client.profile(),
                    client.calls(),
                    || commands::privacy::render_toggle(&result),
                );
                Ok(())
            }
        },
        Command::Raw { command, params } => {
            let params = commands::raw::parse_params(params)?;
            let body = commands::raw::call(client, command, &params)?;
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
