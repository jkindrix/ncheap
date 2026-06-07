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

    /// Refuse before any API call unless the resolved profile has this
    /// name (guards against a leaked NCHEAP_PROFILE switching accounts)
    #[arg(long, global = true, value_name = "NAME")]
    pub expect_profile: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Run every read-only safety check across the account (3 + 2N API calls)
    Audit,
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
    /// Domain transfer operations
    Transfer {
        #[command(subcommand)]
        command: TransferCommand,
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
            Command::Audit => "audit",
            Command::Domains { command } => match command {
                DomainsCommand::List => "domains.list",
                DomainsCommand::Check { .. } => "domains.check",
                DomainsCommand::Lock { lock, unlock, .. } => {
                    if *lock || *unlock {
                        "domains.lock.set"
                    } else {
                        "domains.lock"
                    }
                }
                DomainsCommand::Info { .. } => "domains.info",
                DomainsCommand::Contacts { set_from, .. } => {
                    if set_from.is_some() {
                        "domains.contacts.set"
                    } else {
                        "domains.contacts"
                    }
                }
                DomainsCommand::Register { .. } => "domains.register",
                DomainsCommand::Renew { .. } => "domains.renew",
            },
            Command::Account { command } => match command {
                AccountCommand::Balances { .. } => "account.balances",
                AccountCommand::Pricing { .. } => "account.pricing",
            },
            Command::Dns { command } => match command {
                DnsCommand::Get { .. } => "dns.get",
                DnsCommand::Add { .. } => "dns.add",
                DnsCommand::Remove { .. } => "dns.remove",
                DnsCommand::Set { .. } => "dns.set",
                DnsCommand::SetDefault { .. } => "dns.set_default",
            },
            Command::Privacy { command } => match command {
                PrivacyCommand::List => "privacy.list",
                PrivacyCommand::Enable { .. } => "privacy.enable",
                PrivacyCommand::Disable { .. } => "privacy.disable",
            },
            Command::Transfer { command } => match command {
                TransferCommand::Create { .. } => "transfer.create",
                TransferCommand::Status { .. } => "transfer.status",
            },
            Command::Raw { .. } => "raw",
        }
    }
}

#[derive(Subcommand)]
pub enum TransferCommand {
    /// Start an inbound transfer (mutating, charges money; price-guarded)
    Create {
        domain: String,
        /// EPP/auth code from the current registrar (note: visible in
        /// process listings and shell history)
        #[arg(long)]
        epp_code: String,
        /// Ceiling on the live LISTED transfer price
        #[arg(long)]
        max_price: f64,
        /// Confirm the mutation (required for non-interactive use)
        #[arg(long)]
        yes: bool,
    },
    /// Check the status of a transfer by its TransferID
    Status { transfer_id: String },
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
    /// Add one host record (mutating; setHosts is a full-zone rewrite)
    Add {
        domain: String,
        /// Record type: A, AAAA, ALIAS, CAA, CNAME, MX, MXE, NS, TXT, URL, URL301, FRAME
        #[arg(long = "type")]
        record_type: String,
        /// Host name ("@" for the apex, "www", ...)
        #[arg(long)]
        name: String,
        /// Record value (IP, hostname, text — per record type)
        #[arg(long)]
        address: String,
        /// TTL in seconds (60–60000; API default 1800)
        #[arg(long)]
        ttl: Option<u32>,
        /// MX preference (required for MX records)
        #[arg(long)]
        mx_pref: Option<u32>,
        /// Confirm the mutation (required for non-interactive use)
        #[arg(long)]
        yes: bool,
    },
    /// Remove matching host records (mutating; full-zone rewrite)
    Remove {
        domain: String,
        /// Record type of the records to remove
        #[arg(long = "type")]
        record_type: String,
        /// Host name of the records to remove
        #[arg(long)]
        name: String,
        /// Only remove records with this exact value
        #[arg(long)]
        address: Option<String>,
        /// Confirm the mutation (required for non-interactive use)
        #[arg(long)]
        yes: bool,
    },
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
    /// Revert a domain to Namecheap default DNS (mutating)
    SetDefault {
        domain: String,
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
    /// Show — or with --lock/--unlock, set — the registrar transfer lock
    Lock {
        domain: String,
        /// Turn the transfer lock on (mutating)
        #[arg(long, conflicts_with = "unlock")]
        lock: bool,
        /// Turn the transfer lock off (mutating)
        #[arg(long)]
        unlock: bool,
        /// Confirm the mutation (required for non-interactive use)
        #[arg(long)]
        yes: bool,
    },
    /// Show registration, privacy, and DNS details for a domain
    Info { domain: String },
    /// Show domain contacts (PII redacted unless --full), or replace them
    /// with another owned domain's via --set-from (mutating)
    Contacts {
        domain: String,
        /// Show the actual contact fields
        #[arg(long)]
        full: bool,
        /// Replace all four contact sets with this owned domain's (mutating)
        #[arg(long, value_name = "DOMAIN", conflicts_with = "full")]
        set_from: Option<String>,
        /// Confirm the mutation (required for non-interactive use)
        #[arg(long)]
        yes: bool,
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
