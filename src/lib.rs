// The credential-protection model (0600 config enforcement) is unix-only;
// a non-unix build would silently compile that check away and ship without
// it. Refuse rather than degrade.
#[cfg(not(unix))]
compile_error!(
    "ncheap's config-file permission enforcement is unix-only; non-unix builds are unsupported"
);

pub mod api;
pub mod cli;
pub mod commands;
pub mod config;
pub mod domain;
pub mod output;
