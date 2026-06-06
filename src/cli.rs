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
}

#[derive(Subcommand)]
pub enum DomainsCommand {
    /// List all domains in the account (auto-paginated)
    List,
}
