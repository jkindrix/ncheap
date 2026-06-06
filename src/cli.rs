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
    /// Domain privacy operations
    Privacy {
        #[command(subcommand)]
        command: PrivacyCommand,
    },
    /// Call an allowlisted read-only API method directly, emitting raw XML
    Raw {
        /// API command, e.g. domains.getTldList ("namecheap." prefix optional)
        command: String,
        /// Method parameter, repeatable: --param Key=Value
        #[arg(long = "param", value_name = "KEY=VALUE")]
        params: Vec<String>,
    },
}

impl Command {
    /// The command name used in the JSON envelope; the single source of
    /// truth so main's error path and success path cannot drift apart.
    pub fn name(&self) -> &'static str {
        match self {
            Command::Domains { command } => match command {
                DomainsCommand::List => "domains.list",
                DomainsCommand::Check { .. } => "domains.check",
                DomainsCommand::Lock { .. } => "domains.lock",
                DomainsCommand::Info { .. } => "domains.info",
                DomainsCommand::Contacts { .. } => "domains.contacts",
                DomainsCommand::Register { .. } => "domains.register",
                DomainsCommand::Renew { .. } => "domains.renew",
            },
            Command::Account { command } => match command {
                AccountCommand::Balances { .. } => "account.balances",
                AccountCommand::Pricing { .. } => "account.pricing",
            },
            Command::Dns { command } => match command {
                DnsCommand::Get { .. } => "dns.get",
                DnsCommand::Set { .. } => "dns.set",
            },
            Command::Privacy { command } => match command {
                PrivacyCommand::List => "privacy.list",
                PrivacyCommand::Enable { .. } => "privacy.enable",
                PrivacyCommand::Disable { .. } => "privacy.disable",
            },
            Command::Raw { .. } => "raw",
        }
    }
}

#[derive(Subcommand)]
pub enum PrivacyCommand {
    /// List all domain privacy subscriptions (auto-paginated)
    List,
    /// Enable domain privacy (mutating)
    Enable {
        domain: String,
        /// Email address privacy emails are forwarded to (required, never defaulted)
        #[arg(long)]
        forward_to: String,
        /// Confirm the mutation (required for non-interactive use)
        #[arg(long)]
        yes: bool,
    },
    /// Disable domain privacy (mutating)
    Disable {
        domain: String,
        /// Confirm the mutation (required for non-interactive use)
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum DnsCommand {
    /// Show nameserver mode and host records for a domain
    Get { domain: String },
    /// Point a domain at custom nameservers (mutating)
    Set {
        domain: String,
        /// Nameserver hostnames (registries require at least two)
        #[arg(required = true, num_args = 2..)]
        nameservers: Vec<String>,
        /// Confirm the mutation (required for non-interactive use)
        #[arg(long)]
        yes: bool,
    },
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
    /// Register a domain (mutating, charges money; live price guard)
    Register {
        domain: String,
        /// Registration period in years
        #[arg(long, default_value_t = 1)]
        years: u8,
        /// Ceiling on the live LISTED price (the actual charge may add ICANN fees)
        #[arg(long)]
        max_price: f64,
        /// Owned domain whose contacts are copied for the registration
        #[arg(long)]
        contacts_from: String,
        /// Confirm the mutation (required for non-interactive use)
        #[arg(long)]
        yes: bool,
    },
    /// Renew a domain (mutating, charges money; live price guard)
    Renew {
        domain: String,
        /// Renewal period in years
        #[arg(long, default_value_t = 1)]
        years: u8,
        /// Ceiling on the live LISTED price (the actual charge may add ICANN fees)
        #[arg(long)]
        max_price: f64,
        /// Confirm the mutation (required for non-interactive use)
        #[arg(long)]
        yes: bool,
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
    /// Show product pricing (response cached locally for 24h)
    Pricing {
        /// Product type (DOMAIN, SSLCERTIFICATE)
        #[arg(long = "type", default_value = "DOMAIN")]
        product_type: String,
        /// Product category filter (e.g. DOMAINS)
        #[arg(long)]
        category: Option<String>,
        /// Action filter (e.g. REGISTER, RENEW, TRANSFER)
        #[arg(long)]
        action: Option<String>,
        /// Product name filter (e.g. com)
        #[arg(long)]
        product: Option<String>,
    },
}
