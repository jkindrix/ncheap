use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ncheap", version, about = "Namecheap registrar API CLI")]
pub struct Cli {
    /// Emit machine-readable JSON on stdout
    #[arg(long, global = true)]
    pub json: bool,

    /// Config profile to use (overrides NCHEAP_PROFILE and default_profile)
    #[arg(long, global = true)]
    pub profile: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Domain operations
    Domains {
        #[command(subcommand)]
        command: DomainsCommand,
    },
    /// Account operations
    Account {
        #[command(subcommand)]
        command: AccountCommand,
    },
    /// DNS operations
    Dns {
        #[command(subcommand)]
        command: DnsCommand,
    },
}

#[derive(Subcommand)]
pub enum DnsCommand {
    /// Show nameserver mode and host records for a domain
    Get { domain: String },
}

#[derive(Subcommand)]
pub enum DomainsCommand {
    /// List all domains in the account (auto-paginated)
    List,
    /// Check availability of one or more domains
    Check {
        /// Domains to check (the API caps one call at 50)
        #[arg(required = true)]
        domains: Vec<String>,
    },
    /// Show the registrar lock status of a domain
    Lock { domain: String },
    /// Show registration, privacy, and DNS details for a domain
    Info { domain: String },
    /// Show domain contacts (PII redacted unless --full)
    Contacts {
        domain: String,
        /// Show the actual contact fields
        #[arg(long)]
        full: bool,
    },
}

#[derive(Subcommand)]
pub enum AccountCommand {
    /// Show account balance summary (amounts redacted unless --full)
    Balances {
        /// Show exact balance amounts
        #[arg(long)]
        full: bool,
    },
}
