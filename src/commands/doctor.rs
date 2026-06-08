use std::path::Path;

use serde::Serialize;

use crate::api::{Client, Error, Transport};
use crate::commands::account;
use crate::config;

#[derive(Debug, Serialize)]
pub struct Check {
    pub check: &'static str,
    /// ok | warn | fail
    pub status: &'static str,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    /// True when no check failed: the tool can operate against this profile.
    /// Check results are data (exit stays 0); gate programmatically on this.
    pub ready: bool,
    pub checks: Vec<Check>,
}

/// Read-only environment preflight: can ncheap actually operate against the
/// resolved profile, and is the setup safe? One live read (users.getBalances)
/// proves the key works and the IP is whitelisted; every other check is
/// local. The live call's errors are caught and turned into a check rather
/// than propagated, so the full report always prints.
pub fn run<T: Transport>(client: &Client<T>) -> DoctorReport {
    let mut checks = Vec::new();
    let profile = client.profile();

    // Config source. By the time doctor runs, config has already loaded
    // (and a file, if used, already passed the 0600 gate in main), so this
    // is a positive affirmation of where credentials came from.
    checks.push(match config::config_path() {
        Some(p) if p.exists() => Check {
            check: "config",
            status: "ok",
            detail: format!("config file present, 0600 enforced ({})", p.display()),
        },
        _ => Check {
            check: "config",
            status: "ok",
            detail: "pure-env configuration (no config file)".into(),
        },
    });

    checks.push(Check {
        check: "profile",
        status: "ok",
        detail: format!(
            "active profile {:?} ({})",
            profile.name,
            if profile.sandbox {
                "sandbox"
            } else {
                "production"
            }
        ),
    });

    checks.push(if profile.sandbox {
        Check {
            check: "production_gate",
            status: "ok",
            detail: "sandbox profile; mutations always permitted (no production risk)".into(),
        }
    } else if profile.allow_production_mutations {
        Check {
            check: "production_gate",
            status: "warn",
            detail: "allow_production_mutations is ON; this host holds armed registrar authority"
                .into(),
        }
    } else {
        Check {
            check: "production_gate",
            status: "ok",
            detail: "production mutations disabled (read-only unless armed in config)".into(),
        }
    });

    // Spend cap only matters once production mutations are armed.
    if !profile.sandbox && profile.allow_production_mutations {
        checks.push(match profile.max_daily_spend {
            Some(cap) => Check {
                check: "spend_cap",
                status: "ok",
                detail: format!("max_daily_spend set to {cap:.2}"),
            },
            None => Check {
                check: "spend_cap",
                status: "warn",
                detail:
                    "max_daily_spend unset; production purchases are refused until a cap is set"
                        .into(),
            },
        });
    }

    checks.push(match client.journal_dir() {
        None => Check {
            check: "state_dir",
            status: "warn",
            detail: "no state directory available; mutations cannot be journaled".into(),
        },
        Some(dir) => match probe_writable(dir) {
            Ok(()) => Check {
                check: "state_dir",
                status: "ok",
                detail: format!("writable ({})", dir.display()),
            },
            Err(e) => Check {
                check: "state_dir",
                status: "fail",
                detail: format!(
                    "not writable ({e}); mutations will be refused ({})",
                    dir.display()
                ),
            },
        },
    });

    // The one live call: proves the key is valid and the IP whitelisted.
    checks.push(match account::balances(client) {
        Ok(_) => Check {
            check: "api_auth",
            status: "ok",
            detail: "users.getBalances succeeded; key valid and IP whitelisted".into(),
        },
        // The single most common live failure; the message already carries
        // the whitelist hint (see api::xml).
        Err(Error::Api { code, message }) if code == "1011150" => Check {
            check: "api_auth",
            status: "fail",
            detail: format!("IP whitelist: {message}"),
        },
        Err(Error::RateLimited(m)) => Check {
            check: "api_auth",
            status: "warn",
            detail: format!("rate limited ({m}); auth unconfirmed, retry later"),
        },
        Err(e) => Check {
            check: "api_auth",
            status: "fail",
            detail: format!("live read failed: {e}"),
        },
    });

    let ready = !checks.iter().any(|c| c.status == "fail");
    DoctorReport { ready, checks }
}

/// Write-and-remove probe: the surest cross-platform test that mutations
/// will be journalable. Creates the state dir if absent (benign — the
/// journal would create it on first mutation anyway).
fn probe_writable(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let probe = dir.join(".doctor-probe");
    std::fs::write(&probe, b"")?;
    std::fs::remove_file(&probe)
}

pub fn render(report: &DoctorReport) {
    for c in &report.checks {
        crate::safe_println!("[{}] {}: {}", c.status.to_uppercase(), c.check, c.detail);
    }
    if report.ready {
        crate::safe_println!("doctor: ready");
    } else {
        let n = report.checks.iter().filter(|c| c.status == "fail").count();
        crate::safe_println!("doctor: NOT ready ({n} failing)");
    }
}
